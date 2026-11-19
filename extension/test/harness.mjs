// Loads one of the extension's scripts as the browser would, into a context of its own whose browser APIs are fakes.
// The scripts are plain files the browser runs without a build step, so they are run here the same way.

import fs from "node:fs";
import vm from "node:vm";

/** An event a script can listen to, as `chrome.*.onX`. */
export function event() {
  const listeners = [];
  return {
    listeners,
    addListener: (listener) => listeners.push(listener),
    /** Calls every listener with `args` and returns what the first one returned. */
    fire: (...args) => listeners.map((listener) => listener(...args))[0],
  };
}

/** Runs `file` (relative to the extension folder) in a fresh context holding `globals`. */
export function load(file, globals) {
  const context = vm.createContext({ console, setTimeout, clearTimeout, URL, queueMicrotask, ...globals });
  const source = fs.readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
  vm.runInContext(source, context, { filename: file });
  return {
    context,
    /** The value of an expression in the script's scope, its top-level `let` and `const` included. */
    evaluate: (expression) => vm.runInContext(expression, context),
  };
}

/** Lets every pending promise callback run. */
export function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

/** `value` rebuilt from JSON, so that objects made in a script's context compare equal to ones made in a test. */
export function plain(value) {
  return JSON.parse(JSON.stringify(value));
}
