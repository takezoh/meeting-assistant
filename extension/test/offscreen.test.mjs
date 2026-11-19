import assert from "node:assert/strict";
import { test } from "node:test";

import { event, load, plain } from "./harness.mjs";

/** offscreen.js with fake media and network: `stopped` counts the tab's tracks stopped, `closed` the audio closed. */
function page({ audioFails = false, uploadFails = false, captureFails = false } = {}) {
  const state = { stopped: 0, audioClosed: 0, sent: [] };
  const track = () => ({ stop: () => (state.stopped += 1), onended: null });
  const tracks = [track(), track()];
  const stream = { getTracks: () => tracks, getVideoTracks: () => [tracks[1]] };

  class AudioContext {
    constructor() {
      if (audioFails) throw new Error("NotSupportedError: no audio output");
      this.destination = {};
    }
    createMediaStreamSource() {
      return { connect: () => {} };
    }
    close() {
      state.audioClosed += 1;
    }
  }
  class MediaRecorder {
    static isTypeSupported = () => true;
    constructor() {
      this.state = "inactive";
      this.mimeType = "video/mp4";
    }
    start() {
      this.state = "recording";
    }
    stop() {
      this.state = "inactive";
    }
  }
  class WebSocket {
    static OPEN = 1;
    static CLOSED = 3;
    constructor() {
      this.readyState = 0;
      setTimeout(() => {
        if (uploadFails) {
          this.onerror?.();
          return;
        }
        this.readyState = WebSocket.OPEN;
        this.onopen?.();
      }, 0);
    }
    send(data) {
      state.sent.push(data);
    }
    close() {
      this.readyState = WebSocket.CLOSED;
    }
  }
  const chrome = {
    runtime: { onMessage: event(), connect: () => ({ postMessage: () => {} }) },
  };
  const navigator = {
    mediaDevices: {
      getUserMedia: async () => {
        if (captureFails) throw new Error("NotAllowedError: Permission denied");
        return stream;
      },
    },
  };
  const script = load("offscreen.js", { chrome, navigator, AudioContext, MediaRecorder, WebSocket });
  const start = () =>
    script.context.start({ streamId: "s", tab: 7, size: null, upload: "ws://127.0.0.1:5000/", token: "t" });
  return { start, state };
}

test("a page that cannot be set up after the tab was captured stops the tab's tracks", async () => {
  const p = page({ audioFails: true });
  await assert.rejects(p.start(), /no audio output/);
  assert.equal(p.state.stopped, 2, "a running capture would refuse every later attempt");
});

test("an upload that cannot connect stops the tab's tracks and closes the audio", async () => {
  const p = page({ uploadFails: true });
  await assert.rejects(p.start(), /Cannot connect to the app/);
  assert.equal(p.state.stopped, 2);
  assert.equal(p.state.audioClosed, 1);
});

test("a capture the browser refuses has nothing to stop", async () => {
  const p = page({ captureFails: true });
  await assert.rejects(p.start(), /Permission denied/);
  assert.equal(p.state.stopped, 0);
});

test("a recording that starts sends the hello first and keeps its tracks", async () => {
  const p = page();
  assert.deepEqual(plain(await p.start()), { mime: "video/mp4" });
  assert.equal(p.state.stopped, 0);
  assert.deepEqual(JSON.parse(p.state.sent[0]), { token: "t", mime: "video/mp4" });
});
