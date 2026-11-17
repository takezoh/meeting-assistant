// Probe: tests whether a tab showing https://example.com/ can be captured with tabCapture without a user gesture.
// The result is written to the title of an extension page opened in a new tab (so it can be read from outside).
const TARGET = "https://example.com/";
const done = new Set();

async function report(text) {
  await chrome.tabs.create({ url: chrome.runtime.getURL("result.html") + "#" + encodeURIComponent(text) });
}

async function check(tab) {
  if (tab.status !== "complete" || !(tab.url || "").startsWith(TARGET) || done.has(tab.id)) return;
  done.add(tab.id);
  try {
    const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id });
    await chrome.offscreen.createDocument({
      url: "offscreen.html",
      reasons: ["USER_MEDIA"],
      justification: "tabCapture probe",
    });
    const result = await chrome.runtime.sendMessage({ type: "capture", streamId });
    await report("OK " + JSON.stringify(result));
  } catch (e) {
    await report("ERR " + ((e && e.message) || e));
  }
}

chrome.tabs.onUpdated.addListener((_tabId, _info, tab) => check(tab));
// Also cover tabs that finished loading before the extension started.
chrome.tabs.query({}).then((tabs) => tabs.forEach(check));
setInterval(() => chrome.tabs.query({}).then((tabs) => tabs.forEach(check)), 1000);
