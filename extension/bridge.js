// Relays notifications from hook.js (the page's own world) to the extension's main script (background.js).
let inCall = false;

const send = () => {
  try {
    chrome.runtime.sendMessage({
      type: "call-state",
      inCall,
      host: location.hostname,
      // Told to the app as the meeting's page title, which also tells two meetings of a service apart.
      title: document.title,
      // With the tab's zoom taken out, gives the tab's size in device pixels, which bounds the recording's picture.
      pixelRatio: window.devicePixelRatio,
    });
  } catch {
    // Cannot send from a page that outlived an extension update or disable.
  }
};

window.addEventListener("message", (event) => {
  if (event.source !== window || event.data?.source !== "meeting-assistant") return;
  inCall = Boolean(event.data.inCall);
  send();
});

window.addEventListener("pagehide", () => {
  if (!inCall) return;
  inCall = false;
  send();
});

// The extension's main script may be suspended and lose its state, so resend periodically while in a call.
setInterval(() => {
  if (inCall) send();
}, 5000);
