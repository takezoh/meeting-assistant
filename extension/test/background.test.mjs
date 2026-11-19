import assert from "node:assert/strict";
import { test } from "node:test";

import { event, load, plain, settle } from "./harness.mjs";

const EXTENSION_ID = "lnplioaidiniokndppmckdaflgefgale";

/** background.js with a fake browser: `steps` is what it asked of the browser, in order; `posted` what it told the app. */
function background() {
  const steps = [];
  const posted = [];
  const browser = {
    /** Makes `getMediaStreamId` fail with this message. */
    streamIdError: null,
    /** What the recording page answers to "start". */
    pageReply: {},
    pageOpen: false,
  };
  const port = { onMessage: event(), onDisconnect: event(), postMessage: (message) => posted.push(message) };
  const runtime = {
    id: EXTENSION_ID,
    connectNative: () => port,
    onMessage: event(),
    onConnect: event(),
    onStartup: event(),
    onInstalled: event(),
    openOptionsPage: () => {},
    sendMessage: async (message) => {
      steps.push(`tell the page: ${message.type}`);
      assert.ok(browser.pageOpen, "the recording page exists");
      return message.type === "start" ? browser.pageReply : {};
    },
  };
  const chrome = {
    runtime,
    alarms: { create: () => {}, onAlarm: event() },
    tabs: {
      get: async () => {
        steps.push("read the tab's size");
        return { width: 1280, height: 720 };
      },
      onRemoved: event(),
    },
    tabCapture: {
      getMediaStreamId: async ({ targetTabId }) => {
        steps.push("ask for a stream ID");
        if (browser.streamIdError) throw new Error(browser.streamIdError);
        return `stream-of-${targetTabId}`;
      },
    },
    offscreen: {
      hasDocument: async () => browser.pageOpen,
      createDocument: async () => {
        steps.push("open the recording page");
        browser.pageOpen = true;
      },
      closeDocument: async () => {
        steps.push("close the recording page");
        browser.pageOpen = false;
      },
    },
  };
  const script = load("background.js", { chrome });
  const errors = () => posted.filter((message) => message.type === "error");
  return { ...script, steps, posted, errors, browser, runtime };
}

test("the stream ID is asked for last, once the page that uses it exists, and handed over at once", async () => {
  const b = background();
  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");

  assert.deepEqual(b.steps, [
    "open the recording page",
    "read the tab's size",
    "ask for a stream ID",
    "tell the page: start",
  ]);
  assert.deepEqual(plain(b.errors()), []);
  assert.equal(b.evaluate("recordingTab"), 7);
});

test("a start that fails tells the app why, closes the page and leaves the next start free", async () => {
  const b = background();
  b.browser.pageReply = { error: "NotReadableError: Could not start video source" };

  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");

  assert.deepEqual(plain(b.errors()), [
    { type: "error", tab: 7, message: "NotReadableError: Could not start video source" },
  ]);
  assert.equal(b.steps.at(-1), "close the recording page");
  assert.equal(b.browser.pageOpen, false);
  assert.equal(b.evaluate("recordingTab"), null);

  b.browser.pageReply = {};
  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");
  assert.equal(b.errors().length, 1, "the retry starts");
  assert.equal(b.evaluate("recordingTab"), 7);
});

test("a capture the browser still holds is explained, with what releases it", async () => {
  const b = background();
  b.browser.streamIdError = "Cannot capture a tab with an active stream.";

  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");

  const [error] = b.errors();
  assert.match(error.message, /lets it go only when that capture ends or the tab is closed/);
  assert.match(error.message, /Close the tab and open the meeting again/);
  assert.equal(b.browser.pageOpen, false);
});

test("a missing user gesture is explained with the switch that allows recording without one", async () => {
  const b = background();
  b.browser.streamIdError = "Extension has not been invoked for the current page (see activeTab permission).";

  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");

  assert.match(b.errors()[0].message, new RegExp(`--allowlisted-extension-id=${EXTENSION_ID}`));
});

test("a start while recording is refused without touching the recording under way", async () => {
  const b = background();
  await b.context.startRecording(7, "ws://127.0.0.1:5000/", "token");
  const before = b.steps.length;

  await b.context.startRecording(8, "ws://127.0.0.1:5000/", "token");

  assert.deepEqual(plain(b.errors()), [{ type: "error", tab: 8, message: "Another tab is already being recorded" }]);
  assert.equal(b.steps.length, before, "nothing was asked of the browser");
  assert.equal(b.browser.pageOpen, true, "the recording page stays");
  assert.equal(b.evaluate("recordingTab"), 7);
});

test("a meeting is reported with what its service tells of it", async () => {
  const b = background();
  const report = (tab, url, title) =>
    b.runtime.onMessage.fire(
      { type: "call-state", inCall: true, host: new URL(url).hostname, title, pixelRatio: 2 },
      { tab: { id: tab }, frameId: 0, url },
    );

  report(1, "https://meet.google.com/abc-defg-hij?authuser=0", "Meet - abc-defg-hij");
  report(2, "https://teams.microsoft.com/v2/", " Standup | Microsoft Teams ");
  await settle();

  const meetings = b.posted.filter((message) => message.type === "meeting");
  assert.deepEqual(plain(meetings), [
    {
      type: "meeting",
      tab: 1,
      active: true,
      service: "GoogleMeet",
      metadata: [
        ["meeting_code", "abc-defg-hij"],
        ["page_title", "Meet - abc-defg-hij"],
      ],
    },
    {
      type: "meeting",
      tab: 2,
      active: true,
      service: "TeamsWeb",
      metadata: [["page_title", "Standup | Microsoft Teams"]],
    },
  ]);
});
