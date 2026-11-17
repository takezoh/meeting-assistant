// Runs in a page that is never shown and records the tab's video and audio.
// Video is the tab's display as is. Audio mixes the tab's sound and the microphone into one track.
// Recorded data is sent to background.js every second.

/** Prefer MP4, which players support widely; fall back to WebM if unavailable. */
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

async function start({ streamId, tab, size }) {
  if (session) throw new Error("Already recording");

  const source = { chromeMediaSource: "tab", chromeMediaSourceId: streamId };
  const tabStream = await navigator.mediaDevices.getUserMedia({
    audio: { mandatory: source },
    video: {
      mandatory: { ...source, maxFrameRate: 30, ...(size ? { maxWidth: size.width, maxHeight: size.height } : {}) },
    },
  });

  const port = chrome.runtime.connect({ name: "recording" });
  const audio = new AudioContext();
  const mixed = audio.createMediaStreamDestination();
  const tabAudio = audio.createMediaStreamSource(tabStream);
  tabAudio.connect(mixed);
  // Capturing a tab silences it for the user, so play it back so it stays audible.
  tabAudio.connect(audio.destination);

  let micStream = null;
  try {
    micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    audio.createMediaStreamSource(micStream).connect(mixed);
  } catch (error) {
    port.postMessage({
      type: "warning",
      message: `Your own voice will not be recorded because the microphone could not be captured. Allow the microphone on the extension's options page (${error?.name ?? error})`,
    });
  }

  const stream = new MediaStream([...tabStream.getVideoTracks(), ...mixed.stream.getAudioTracks()]);
  const mimeType = MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type));
  const recorder = new MediaRecorder(stream, {
    mimeType,
    videoBitsPerSecond: VIDEO_BITS_PER_SECOND,
    audioBitsPerSecond: AUDIO_BITS_PER_SECOND,
    videoKeyFrameIntervalDuration: KEY_FRAME_INTERVAL_MS,
  });

  // Send data in order.
  let sending = Promise.resolve();
  const enqueue = (task) => {
    sending = sending.then(task).catch(() => {});
  };
  const finish = (result) => {
    if (!session) return;
    session = null;
    enqueue(() => {
      port.postMessage(result);
      for (const track of [...tabStream.getTracks(), ...(micStream?.getTracks() ?? [])]) track.stop();
      audio.close();
    });
  };

  recorder.ondataavailable = (event) => {
    if (event.data.size === 0) return;
    enqueue(async () => port.postMessage({ type: "chunk", tab, data: toBase64(await event.data.arrayBuffer()) }));
  };
  recorder.onstop = () => finish({ type: "stopped", tab });
  recorder.onerror = (event) => finish({ type: "error", tab, message: `Recording failed (${event.error?.name ?? "unknown"})` });
  // The tab was closed or navigated to another site.
  tabStream.getVideoTracks()[0].onended = () => {
    if (recorder.state !== "inactive") recorder.stop();
  };

  recorder.start(1000);
  session = { recorder };
  return { mime: recorder.mimeType };
}

function stop() {
  if (session && session.recorder.state !== "inactive") session.recorder.stop();
}

function toBase64(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}
