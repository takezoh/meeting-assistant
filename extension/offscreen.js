// Runs in a page that is never shown and records the tab's video and audio, as they are.
// The user's own voice is not recorded here: the app records the microphone itself and adds it to the file.
// The recording goes straight to the resident app over a WebSocket, as binary messages, every second.

/** Prefer MP4, which players support widely; WebM if the browser cannot produce MP4. */
const MIME_TYPES = [
  "video/mp4;codecs=avc1.640028,mp4a.40.2",
  "video/mp4",
  "video/webm;codecs=vp9,opus",
  "video/webm",
];
const VIDEO_BITS_PER_SECOND = 2_500_000;
const AUDIO_BITS_PER_SECOND = 128_000;
/// Recorded data is only flushed at keyframe boundaries, so insert keyframes at this interval.
/// The longer the interval, the more is lost if the browser crashes during recording.
const KEY_FRAME_INTERVAL_MS = 2000;
/// WebSocket close codes: the recording ended normally, or it failed (the reason says why).
const CLOSE_NORMAL = 1000;
const CLOSE_FAILED = 1011;
/// A close reason may be at most 123 bytes.
const MAX_CLOSE_REASON = 120;
/// How long to wait for the app to acknowledge the end of the upload before letting this page be closed.
const CLOSE_ACKNOWLEDGE_TIMEOUT_MS = 20_000;

let session = null;

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message.target !== "offscreen") return;
  if (message.type === "start") {
    start(message).then(respond, (error) => respond({ error: String(error?.message ?? error) }));
    return true;
  }
  if (message.type === "stop") {
    // If not recording, say so (so background.js knows there is nothing to stop).
    respond({ idle: !session });
    stop();
  }
});

/** Opens the upload to the app and resolves once it is connected. */
function connectUpload(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.onopen = () => resolve(socket);
    socket.onerror = () => reject(new Error("Cannot connect to the app to hand over the recording"));
  });
}

async function start({ streamId, tab, size, upload, token }) {
  if (session) throw new Error("Already recording");

  const source = { chromeMediaSource: "tab", chromeMediaSourceId: streamId };
  const tabStream = await navigator.mediaDevices.getUserMedia({
    audio: { mandatory: source },
    video: {
      mandatory: { ...source, maxFrameRate: 30, ...(size ? { maxWidth: size.width, maxHeight: size.height } : {}) },
    },
  });
  const audio = new AudioContext();
  let socket = null;
  const release = () => {
    for (const track of tabStream.getTracks()) track.stop();
    audio.close();
  };

  try {
    // Capturing a tab silences it for the user, so play it back so it stays audible.
    audio.createMediaStreamSource(tabStream).connect(audio.destination);

    const recorder = new MediaRecorder(tabStream, {
      mimeType: MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type)),
      videoBitsPerSecond: VIDEO_BITS_PER_SECOND,
      audioBitsPerSecond: AUDIO_BITS_PER_SECOND,
      videoKeyFrameIntervalDuration: KEY_FRAME_INTERVAL_MS,
    });

    socket = await connectUpload(upload);

    const port = chrome.runtime.connect({ name: "recording" });
    // background.js closes this page when told the recording finished, which would cut off data still being sent.
    // So it is told only once the app has acknowledged the close, which it sends after everything before it.
    let reported = false;
    const reportFinished = () => {
      if (reported) return;
      reported = true;
      port.postMessage({ type: "finished", tab });
    };
    const finish = (code, reason) => {
      if (!session) return;
      session = null;
      release();
      if (socket.readyState === WebSocket.CLOSED) {
        reportFinished();
        return;
      }
      socket.close(code, reason.slice(0, MAX_CLOSE_REASON));
      // In case the app never answers the close.
      setTimeout(reportFinished, CLOSE_ACKNOWLEDGE_TIMEOUT_MS);
    };

    // Each piece goes as it is, a binary message; the WebSocket keeps them in order.
    recorder.ondataavailable = (event) => {
      if (event.data.size > 0 && socket.readyState === WebSocket.OPEN) socket.send(event.data);
    };
    recorder.onstop = () => finish(CLOSE_NORMAL, "");
    recorder.onerror = (event) => finish(CLOSE_FAILED, `Recording failed (${event.error?.name ?? "unknown"})`);
    // The tab was closed or navigated to another site.
    tabStream.getVideoTracks()[0].onended = () => {
      if (recorder.state !== "inactive") recorder.stop();
    };
    socket.onclose = () => {
      // The app stopped listening (it ended or failed the recording on its side): stop, which finishes.
      if (recorder.state !== "inactive") recorder.stop();
      // Or the close this page started has been acknowledged.
      else if (!session) reportFinished();
    };

    recorder.start(1000);
    // Sent right as recording starts: the app takes its arrival as the recording's time zero, to place the microphone
    // it records on the same timeline. It also proves this page was told about the recording, and says which
    // container follows. The first data comes a second later, so this stays the first message.
    socket.send(JSON.stringify({ token, mime: recorder.mimeType }));
    session = { recorder };
    return { mime: recorder.mimeType };
  } catch (error) {
    socket?.close(CLOSE_FAILED, String(error?.message ?? error).slice(0, MAX_CLOSE_REASON));
    release();
    throw error;
  }
}

function stop() {
  if (session && session.recorder.state !== "inactive") session.recorder.stop();
}
