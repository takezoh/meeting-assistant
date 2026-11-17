// Probe: actually captures the tab's video and audio with the received stream ID and returns the track counts and size.
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type !== "capture") return;
  const source = { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: message.streamId } };
  navigator.mediaDevices
    .getUserMedia({ audio: source, video: source })
    .then((stream) => {
      const video = stream.getVideoTracks()[0];
      const settings = video ? video.getSettings() : {};
      sendResponse({
        video: stream.getVideoTracks().length,
        audio: stream.getAudioTracks().length,
        width: settings.width,
        height: settings.height,
      });
      setTimeout(() => stream.getTracks().forEach((track) => track.stop()), 1000);
    })
    .catch((e) => sendResponse({ error: String(e) }));
  return true;
});
