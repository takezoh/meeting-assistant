---
change: change-20260904-phase1-capture-runtime-remediation
role: implementation
---

<!-- lifecycle is owned by change.md -->

# Implementation

## Responsibility boundaries

1. A source-service worker owns `CaptureSource::next` and never performs durable
   filesystem I/O.
2. A bounded channel carries source events to a writer worker that exclusively
   owns `ChunkWriter` and its filesystem operations.
3. Queue overflow or a disconnected writer becomes an explicit source gap or a
   terminal capture error; it is never treated as successful progress.
4. Device-mode recovery is separate from process/system-loopback recovery. A
   `MicEndpointSource` never delegates invalidation to a mode-agnostic system
   loopback fallback.
5. The diagnostic loop propagates append errors out of the outer loop, rather
   than breaking only the current per-source iteration.

## Intended files

- `crates/ma-capture/src/wasapi/mod.rs`
- `crates/ma-capture/src/wasapi/mic_endpoint.rs`
- `crates/ma-engine/src/bin/ma-diag.rs`
- `crates/ma-engine/tests/diagnostic_cli.rs`
