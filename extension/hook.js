// Runs in the same world as the page and watches the call connections (WebRTC).
// While a call is connected it counts as "in a meeting"; changes are reported to bridge.js.
// This also detects joining without a microphone, and while muted.
(() => {
  const Native = window.RTCPeerConnection;
  if (!Native || window.__meetingAssistantHooked) return;
  window.__meetingAssistantHooked = true;

  const connections = new Set();
  let reported = false;

  const isConnected = (pc) =>
    pc.connectionState === "connected" || pc.iceConnectionState === "connected" || pc.iceConnectionState === "completed";

  const update = () => {
    for (const pc of connections) {
      if (pc.connectionState === "closed" || pc.signalingState === "closed") connections.delete(pc);
    }
    const inCall = [...connections].some(isConnected);
    if (inCall !== reported) {
      reported = inCall;
      window.postMessage({ source: "meeting-assistant", inCall }, window.location.origin);
    }
  };

  class HookedPeerConnection extends Native {
    constructor(...args) {
      super(...args);
      connections.add(this);
      this.addEventListener("connectionstatechange", update);
      this.addEventListener("iceconnectionstatechange", update);
    }
  }
  Object.defineProperty(HookedPeerConnection, "name", { value: "RTCPeerConnection" });
  window.RTCPeerConnection = HookedPeerConnection;
  if (window.webkitRTCPeerConnection) window.webkitRTCPeerConnection = HookedPeerConnection;

  // close() fires no state-change event, so also check periodically.
  setInterval(update, 1000);
})();
