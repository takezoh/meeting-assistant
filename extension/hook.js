// Runs in the same world as the page and watches the call connections (WebRTC).
// While a call is connected it counts as "in a meeting"; changes are reported to bridge.js.
// This also detects joining without a microphone, and while muted.
(() => {
  const Native = window.RTCPeerConnection;
  if (!Native || window.__meetingAssistantHooked) return;
  window.__meetingAssistantHooked = true;

  const connections = new Set();
  /** Connections that have connected at least once. */
  const established = new WeakSet();
  let reported = false;

  const isConnected = (pc) =>
    pc.connectionState === "connected" || pc.iceConnectionState === "connected" || pc.iceConnectionState === "completed";
  // A connected call that drops briefly goes "disconnected" while it tries to recover, and "failed" if it cannot.
  // Only "failed" and "closed" end it, so the app can end the meeting as soon as the call is really over.
  const isReconnecting = (pc) =>
    established.has(pc) && (pc.connectionState === "disconnected" || pc.iceConnectionState === "disconnected");

  const update = () => {
    for (const pc of connections) {
      if (pc.connectionState === "closed" || pc.signalingState === "closed") connections.delete(pc);
      else if (isConnected(pc)) established.add(pc);
    }
    const inCall = [...connections].some((pc) => isConnected(pc) || isReconnecting(pc));
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
