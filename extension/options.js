// The extension's options page. Obtains microphone permission so your own voice can be recorded.
const state = document.getElementById("state");
const button = document.getElementById("allow");
document.getElementById("id").textContent = chrome.runtime.id;

async function refresh() {
  const permission = await navigator.permissions.query({ name: "microphone" });
  const granted = permission.state === "granted";
  state.textContent = granted
    ? "The microphone is allowed."
    : permission.state === "denied"
      ? "The microphone is blocked. Change it to allowed in the site settings from the address bar."
      : "The microphone has not been allowed yet.";
  button.hidden = granted;
}

button.addEventListener("click", async () => {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of stream.getTracks()) track.stop();
  } catch {
    // If denied, the message below tells the user.
  }
  refresh();
});

refresh();
