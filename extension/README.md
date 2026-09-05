# Meeting Assistant detection-only extension (Phase 1 PoC)

A manifest-v3 service worker that reports the **hostname** and **audible** state of the active tab
to the local Meeting Assistant engine over the loopback channel (`contract-extension-signal-delivery`,
`adr-20260903-extension-localhost-channel-trust`, `adr-20260904-extension-endpoint-provisioning-poc`).

It is detection only:

- no content script, no `scripting`, no `nativeMessaging`, no `storage`, no `<all_urls>`;
- permissions are exactly `["tabs"]` plus the host permission `http://127.0.0.1/*`;
- the only thing derived from a tab URL is its hostname; the path, query string and title never
  leave the worker;
- no tab audio is captured (an explicit PLAN non-goal).

## Provisioning

The worker imports `./endpoint.js`, a **generated, untracked** file the diagnostic harness
(`ma-diag`) writes into this directory at every engine start:

```js
export const ENDPOINT = {
  port: 49152,                    // the listener's ephemeral port
  token: "<64 hex chars>",        // the per-start token, rotates on every engine start
  meeting_hosts: ["..."],         // meeting hostnames from the adapter tables
};
```

Without `endpoint.js` the worker fails to load and posts nothing. Do not commit the file.

## Loading

1. Start the engine harness with the path of this directory, so it writes `endpoint.js`.
2. `chrome://extensions` → Developer mode → *Load unpacked* → this directory.
3. Give the engine the extension id Chrome assigned (the listener pins
   `chrome-extension://<id>` as the only accepted origin).

## Wire message

Exactly the fields of `contracts/extension-channel/message.schema.json`, posted as JSON to
`POST http://127.0.0.1:<port>/report` with the token in the `X-MA-Token` header. The listener
answers with a status only: `204` accepted, `401` token rejected, `403` origin rejected, `400`
malformed, `409` stale, `429` rate limited.

On `401` (the engine restarted and the token rotated) the worker stops posting and logs the
condition instead of retrying with a dead token. Reload the extension after the harness has
re-provisioned `endpoint.js`.

## What the tests check

`crates/ma-ext-channel/tests/extension_poc.rs` reads `manifest.json` and `background.js` and
asserts the permission set, the absence of a content script, that the message field list equals the
schema, and that `tab.url` is only ever reduced to its hostname.
