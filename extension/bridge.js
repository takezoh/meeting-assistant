// Relays notifications from hook.js (the page's own world) to the extension's main script (background.js).
let inCall = false;

const send = () => {
  try {
    chrome.runtime.sendMessage({
      type: "call-state",
      inCall,
      host: location.hostname,
      // Tells two meetings of one service apart in the app (Google Meet uses its meeting code instead).
      title: document.title,
      // Sent so the recording resolution can match the tab's actual size.
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
