// The extension's main script.
// - Collects each tab's call state and reports meeting start/end to the resident app.
// - Starts and stops tab recording on the resident app's command. The recording itself is done by offscreen.js.
const HOST = "com.meeting_assistant.host";
const RECONNECT_ALARM = "reconnect";

let port = null;
/** Tab ID -> set of frame IDs that are in a call */
const callFrames = new Map();
/** Tab ID -> { service, title } (tabs already reported to the app as in a meeting) */
const meetings = new Map();
/** ID of the tab being recorded */
let recordingTab = null;
/** Tab ID -> display scale (how many physical pixels one CSS pixel is) */
const pixelRatios = new Map();

function serviceName(host) {
  if (host === "meet.google.com") return "GoogleMeet";
  if (host.startsWith("teams.")) return "TeamsWeb";
  if (host.endsWith("zoom.us")) return "ZoomWeb";
  if (host.endsWith("webex.com")) return "WebexWeb";
  if (host === "app.slack.com") return "SlackWeb";
  if (host.endsWith("whereby.com")) return "Whereby";
  if (host === "meet.jit.si") return "Jitsi";
  return "BrowserMeeting";
}

// ---- Connection to the resident app

function connect() {
  if (port) return;
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    port = null;
    return;
  }
  port.onMessage.addListener(onAppMessage);
  port.onDisconnect.addListener(() => {
    // The app is not running or has exited. Stop recording if in progress.
    void chrome.runtime.lastError;
    port = null;
    stopRecording();
  });
  for (const [tab, meeting] of meetings) post({ type: "meeting", tab, active: true, ...meeting });
}

function post(message) {
  if (!port) return;
  try {
    port.postMessage(message);
  } catch {
    port = null;
  }
}

function onAppMessage(message) {
  if (message.type === "start") startRecording(message.tab);
  else if (message.type === "stop") stopRecording();
}

// Try to connect periodically in case the app starts later.
chrome.alarms.create(RECONNECT_ALARM, { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === RECONNECT_ALARM) connect();
});
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener((details) => {
  connect();
  if (details.reason === "install") chrome.runtime.openOptionsPage();
});
connect();

// ---- Meeting detection

/**
 * What tells this meeting apart from others of the same service: the meeting code for Google Meet, otherwise the
 * page's title.
 */
function meetingTitle(host, url, title) {
  if (host === "meet.google.com") {
    let path = "";
    try {
      path = new URL(url).pathname;
    } catch {
      // No usable URL; fall back to the title.
    }
    const code = /^\/([a-z]{3}-[a-z]{4}-[a-z]{3})(?:\/|$)/.exec(path);
    if (code) return code[1];
  }
  return String(title ?? "").trim().slice(0, 120);
}

function setCallState(tab, frame, inCall, host, url, title) {
  const frames = callFrames.get(tab) ?? new Set();
  if (inCall) frames.add(frame);
  else frames.delete(frame);
  if (frames.size > 0) callFrames.set(tab, frames);
  else callFrames.delete(tab);

  const active = frames.size > 0;
  if (active === meetings.has(tab)) return;
  if (active) {
    const meeting = { service: serviceName(host), title: meetingTitle(host, url, title) };
    meetings.set(tab, meeting);
    connect();
    post({ type: "meeting", tab, active: true, ...meeting });
  } else {
    meetings.delete(tab);
    post({ type: "meeting", tab, active: false });
  }
}

chrome.runtime.onMessage.addListener((message, sender) => {
  if (message.type === "call-state" && sender.tab) {
    if (sender.frameId === 0 && message.pixelRatio > 0) pixelRatios.set(sender.tab.id, message.pixelRatio);
    setCallState(sender.tab.id, sender.frameId ?? 0, message.inCall, message.host, sender.url, message.title);
  }
});

chrome.tabs.onRemoved.addListener((tab) => {
  pixelRatios.delete(tab);
  if (!meetings.has(tab)) return;
  callFrames.delete(tab);
  meetings.delete(tab);
  post({ type: "meeting", tab, active: false });
});

// ---- Recording

async function startRecording(tab) {
  try {
    if (recordingTab !== null) throw new Error("Another tab is already being recorded");
    recordingTab = tab;
    const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: tab });
    if (!(await chrome.offscreen.hasDocument())) {
      await chrome.offscreen.createDocument({
        url: "offscreen.html",
        reasons: ["USER_MEDIA"],
        justification: "To record the meeting tab",
      });
    }
    // Without this the recording uses the full screen size, so pass the tab's actual size as the upper limit.
    const { width, height } = await chrome.tabs.get(tab);
    const ratio = pixelRatios.get(tab) ?? 1;
    const size = width && height ? { width: Math.round(width * ratio), height: Math.round(height * ratio) } : null;
    const result = await chrome.runtime.sendMessage({ target: "offscreen", type: "start", streamId, tab, size });
    if (result?.error) throw new Error(result.error);
    post({ type: "started", tab, mime: result.mime });
  } catch (error) {
    recordingTab = null;
    let message = String(error?.message ?? error);
    if (message.includes("has not been invoked")) {
      message =
        "The browser does not allow recording a tab without a user gesture. Start the browser with --allowlisted-extension-id=" +
        chrome.runtime.id +
        " added.";
    }
    post({ type: "error", tab, message });
    closeOffscreen();
  }
}

function stopRecording() {
  if (recordingTab === null) return;
  // offscreen.js returns "stopped" after it has finished sending the remaining data.
  // If recording had not started, there is nothing to stop, so only clean up.
  const reset = () => {
    recordingTab = null;
    closeOffscreen();
  };
  chrome.runtime
    .sendMessage({ target: "offscreen", type: "stop" })
    .then((result) => result?.idle && reset(), reset);
}

async function closeOffscreen() {
  try {
    if (await chrome.offscreen.hasDocument()) await chrome.offscreen.closeDocument();
  } catch {
    // Already closed.
  }
}

// Forward recording data and results from offscreen.js to the resident app as is.
chrome.runtime.onConnect.addListener((recording) => {
  if (recording.name !== "recording") return;
  recording.onMessage.addListener((message) => {
    post(message);
    if (message.type === "stopped" || message.type === "error") {
      recordingTab = null;
      closeOffscreen();
    }
  });
});
