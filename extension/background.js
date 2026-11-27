// The extension's main script.
// - Collects each tab's call state and reports meeting start/end to the resident app.
// - Starts and stops tab recording on the resident app's command. The recording itself is done by offscreen.js.
const HOST = "com.meeting_assistant.host";
const RECONNECT_ALARM = "reconnect";

let port = null;
/** Tab ID -> set of frame IDs that are in a call */
const callFrames = new Map();
/** Tab ID -> { service, metadata } (tabs already reported to the app as in a meeting) */
const meetings = new Map();
/** ID of the tab being recorded */
let recordingTab = null;
/** Tab ID -> how many device pixels the tab has per pixel of its size as `chrome.tabs.get` reports it */
const deviceScales = new Map();

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
  if (message.type === "start") startRecording(message.tab, message.upload, message.token, message.max_size);
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
 * What the extension tells the app of a meeting, which differs per service, as [key, value] pairs in order. The first
 * tells the meeting apart from others of the same service: the meeting code for Google Meet, otherwise the page's
 * title. The app keeps keys of lowercase letters, digits and "_", and drops empty values.
 */
function meetingMetadata(host, url, title) {
  const pageTitle = ["page_title", String(title ?? "").trim()];
  if (host === "meet.google.com") {
    let path = "";
    try {
      path = new URL(url).pathname;
    } catch {
      // No usable URL; the page's title is all there is.
    }
    const code = /^\/([a-z]{3}-[a-z]{4}-[a-z]{3})(?:\/|$)/.exec(path);
    if (code) return [["meeting_code", code[1]], pageTitle];
  }
  return [pageTitle];
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
    const meeting = { service: serviceName(host), metadata: meetingMetadata(host, url, title) };
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
    if (sender.frameId === 0 && message.pixelRatio > 0) noteDeviceScale(sender.tab.id, message.pixelRatio);
    setCallState(sender.tab.id, sender.frameId ?? 0, message.inCall, message.host, sender.url, message.title);
  }
});

/**
 * Notes the device pixels per pixel of the tab's size from its page's `devicePixelRatio`. That ratio includes the
 * tab's zoom, which does not change the tab's size, so the zoom is taken out of it.
 */
async function noteDeviceScale(tab, pixelRatio) {
  try {
    const zoom = await chrome.tabs.getZoom(tab);
    deviceScales.set(tab, zoom > 0 ? pixelRatio / zoom : pixelRatio);
  } catch {
    // The tab has closed meanwhile.
  }
}

chrome.tabs.onRemoved.addListener((tab) => {
  deviceScales.delete(tab);
  if (!meetings.has(tab)) return;
  callFrames.delete(tab);
  meetings.delete(tab);
  post({ type: "meeting", tab, active: false });
});

// ---- Recording

/**
 * The size a tab is captured at: its size in device pixels, scaled down to fit in `maxSize` with its aspect ratio
 * kept, and never up. `tab` is its size as `chrome.tabs.get` reports it and `scale` the device pixels per pixel of it.
 *
 * The browser delivers every picture at exactly the size it is given, for the whole capture, and fits the tab into it
 * with black bars (Chrome and Edge 158, 2026-11-25). So the app's largest picture is not passed as it is: a tab of
 * another shape would get bars, and a smaller tab would be scaled up. Given no size, the browser captures at the size of
 * the screen, so a tab whose size cannot be read is captured at `maxSize`.
 */
function captureSize(tab, scale, maxSize) {
  if (!(tab.width > 0 && tab.height > 0)) return maxSize;
  const device = { width: Math.round(tab.width * scale), height: Math.round(tab.height * scale) };
  const fit = Math.min(1, maxSize.width / device.width, maxSize.height / device.height);
  return {
    width: Math.min(maxSize.width, Math.max(1, Math.round(device.width * fit))),
    height: Math.min(maxSize.height, Math.max(1, Math.round(device.height * fit))),
  };
}

/** Whether `size` is a picture size: a whole positive width and height. */
function isPictureSize(size) {
  return Number.isInteger(size?.width) && size.width > 0 && Number.isInteger(size?.height) && size.height > 0;
}

/**
 * Records `tab`, at most `maxSize`; the recording page sends the recording to `upload` itself, presenting `token`.
 *
 * Asking for a stream ID registers a capture of the tab in the browser, which refuses another capture of it while that
 * one is pending or running. So everything that can fail on its own comes first, the stream ID is used as soon as it is
 * given, and whatever started is stopped on failure (offscreen.js stops its tracks; this closes the page).
 */
async function startRecording(tab, upload, token, maxSize) {
  // Refused before anything is touched. A start without the largest picture comes from an app of another version,
  // which would have the tab recorded at whatever size it is.
  if (!isPictureSize(maxSize)) {
    post({
      type: "error",
      tab,
      message:
        "The app and the extension are of different versions: the app did not say the largest picture to record " +
        "(max_size). Install the app again and press Reload for the extension on the browser's extensions page.",
    });
    return;
  }
  // The recording under way must not be cleaned up as a failed start.
  if (recordingTab !== null) {
    post({ type: "error", tab, message: "Another tab is already being recorded" });
    return;
  }
  recordingTab = tab;
  try {
    if (!(await chrome.offscreen.hasDocument())) {
      await chrome.offscreen.createDocument({
        url: "offscreen.html",
        reasons: ["USER_MEDIA"],
        justification: "To record the meeting tab",
      });
    }
    const { width, height } = await chrome.tabs.get(tab);
    const size = captureSize({ width, height }, deviceScales.get(tab) ?? 1, maxSize);
    const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: tab });
    const result = await chrome.runtime.sendMessage({
      target: "offscreen",
      type: "start",
      streamId,
      tab,
      size,
      upload,
      token,
    });
    if (result?.error) throw new Error(result.error);
  } catch (error) {
    recordingTab = null;
    post({ type: "error", tab, message: startFailure(error) });
    await closeOffscreen();
  }
}

/** What the app is told when recording a tab cannot start: the browser's reason, explained where it says too little. */
function startFailure(error) {
  const message = String(error?.message ?? error);
  if (message.includes("has not been invoked")) {
    return (
      "The browser does not allow recording a tab without a user gesture. Start the browser with --allowlisted-extension-id=" +
      chrome.runtime.id +
      " added."
    );
  }
  if (message.includes("active stream")) {
    // The browser keeps a capture it considers pending or running until it ends or the tab closes; nothing here can
    // release it. This extension holds none at this point, so it is an earlier attempt's, another extension's or a cast.
    return (
      "The browser is still capturing this tab elsewhere, or still holds a capture from an attempt that did not start, " +
      "and lets it go only when that capture ends or the tab is closed. Close the tab and open the meeting again to record it."
    );
  }
  return message;
}

function stopRecording() {
  if (recordingTab === null) return;
  // offscreen.js reports "finished" after it has sent the remaining data and closed its upload.
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

// The recording itself goes from offscreen.js to the app directly; here only its end is noted.
chrome.runtime.onConnect.addListener((recording) => {
  if (recording.name !== "recording") return;
  recording.onMessage.addListener((message) => {
    if (message.type === "finished") {
      recordingTab = null;
      closeOffscreen();
    }
  });
});
