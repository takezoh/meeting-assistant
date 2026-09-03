// The frontend renders the engine's view and forwards two commands. It keeps no timer of its own:
// the countdown number is whatever the last `arming.tick` said, refreshed by polling the backend view.
const invoke = window.__TAURI__ ? window.__TAURI__.core.invoke : async () => null;

function render(view) {
  if (!view) return;
  const connection = document.getElementById("connection");
  connection.dataset.state = view.connected ? "connected" : "disconnected";
  connection.textContent = view.connected ? "エンジンに接続中" : "エンジンに接続していません";
  document.getElementById("session-state").textContent = view.session_state ?? "—";
  const countdown = document.getElementById("countdown");
  const arming = view.countdown_remaining_ms !== null && view.countdown_remaining_ms !== undefined;
  countdown.hidden = !arming;
  if (arming) document.getElementById("remaining").textContent = String(Math.ceil(view.countdown_remaining_ms / 1000));
  document.getElementById("indicator").hidden = !view.recording;
  const error = document.getElementById("error");
  error.hidden = !view.last_error;
  error.textContent = view.last_error ?? "";
}

document.getElementById("cancel").addEventListener("click", () => invoke("cancel_arming"));

async function refresh() {
  render(await invoke("view"));
}
refresh();
setInterval(refresh, 250);
