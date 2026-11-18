// The extension's options page. Obtains microphone permission so your own voice can be recorded.
const state = document.getElementById("state");
const button = document.getElementById("allow");
const settings = document.getElementById("settings");
const problem = document.getElementById("problem");
document.getElementById("id").textContent = chrome.runtime.id;

async function refresh() {
  const permission = await navigator.permissions.query({ name: "microphone" });
  const granted = permission.state === "granted";
  state.textContent = granted
    ? "The microphone is allowed."
    : permission.state === "denied"
      ? "The microphone is blocked for this extension."
      : "The microphone has not been allowed yet.";
  button.hidden = granted;
  settings.hidden = permission.state !== "denied";
  if (granted) problem.hidden = true;
}

/** What went wrong when asking for the microphone, and what to do about it. */
function explain(error) {
  const name = error?.name ?? "Error";
  const detail = `(${name}${error?.message ? `: ${error.message}` : ""})`;
  if (name === "NotAllowedError" && /system/i.test(error?.message ?? "")) {
    return `The operating system does not let the browser use the microphone. On macOS open System Settings > Privacy & Security > Microphone, turn on the browser, then quit and restart it. ${detail}`;
  }
  if (name === "NotAllowedError") {
    return `The browser refused the microphone for this extension. Choose "Open microphone settings" and set Microphone to Allow, or allow it when the browser asks. ${detail}`;
  }
  if (name === "NotFoundError") return `No microphone was found. ${detail}`;
  if (name === "NotReadableError") {
    return `The microphone could not be opened. Another app may hold it, or the operating system blocks the browser from it. ${detail}`;
  }
  return `The microphone could not be allowed. ${detail}`;
}

button.addEventListener("click", async () => {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of stream.getTracks()) track.stop();
    problem.hidden = true;
  } catch (error) {
    problem.textContent = explain(error);
    problem.hidden = false;
    settings.hidden = false;
  }
  refresh();
});

settings.addEventListener("click", () => {
  chrome.tabs.create({ url: `chrome://settings/content/siteDetails?site=${encodeURIComponent(location.origin)}` });
});

refresh();
