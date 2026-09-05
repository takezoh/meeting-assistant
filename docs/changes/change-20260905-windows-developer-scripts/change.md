---
id: change-20260905-windows-developer-scripts
kind: change
title: Windows developer bootstrap build and install scripts
status: active
created: '2026-09-05'
profile: sdd@1
intent: Make a fresh Windows development machine reproducibly able to bootstrap Rust
  prerequisites, build repository binaries, and install development artifacts.
outcomes:
- Bootstrap detects or installs rustup and the MSVC C++ build workload, then verifies
  required commands and components.
- Build produces engine, diagnostic, processor-host, and optional UI release artifacts
  from the repository root.
- Install copies verified artifacts to a user-local development directory without
  claiming to be the signed product installer.
scope:
- scripts/windows/
non_goals:
- Code signing, production packaging, browser managed policy, system-wide Program
  Files installation, and automatic startup registration.
change_classes:
- capability
governance:
  gate: auto
  reasons: []
members:
- role: requirements
  path: changes/change-20260905-windows-developer-scripts/requirements.md
  required: true
- role: implementation
  path: changes/change-20260905-windows-developer-scripts/implementation.md
  required: true
- role: verification
  path: changes/change-20260905-windows-developer-scripts/verification.md
  required: true
promotion: []
unresolved_decisions: []
tags:
- windows
- developer-experience
- build
owners:
- take
relations:
- {type: references, target: change-20260904-phase1-windows-detection-and-capture}
source_paths: []
summary: Provide idempotent PowerShell entrypoints for prerequisites, release builds,
  and user-local developer installation.
updated: '2026-09-05'
---

## Summary

Windows setup is currently tribal knowledge. A new developer encounters `rustup` not
found before any repository command can run, and there is no single definition of
which binaries constitute a development build or where a non-production install may
be placed.

## Closure Notes

Implementation is complete. Portable, cross-target, and PowerShell parser checks
pass; Windows PowerShell 5.1 native bootstrap/build/install smoke testing remains.


{% transition from="draft" to="ready" date="2026-09-05" %}
Requirements and implementation boundary are documented.
{% /transition %}


{% transition from="ready" to="active" date="2026-09-05" %}
Implementation started after documentation lint passed.
{% /transition %}
