---
id: issue-20260904-windows-audio-session-clippy
kind: issue
title: Windows audio session code fails strict clippy
status: open
created: '2026-09-04'
issue_type: bug
tags: []
owners: []
relations:
- {type: originatedFrom, target: change-20260904-phase1-capture-runtime-remediation}
subject_paths:
- crates/ma-signals-windows/src/audio_session.rs
summary: Windows-target clippy reports type_complexity and arc_with_non_send_sync
  in the existing audio-session registration storage.
---

## 症状 / 機会

`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
が、既存のWindows audio-session registration storageに対して失敗する。

## 証拠

- `crates/ma-signals-windows/src/audio_session.rs:160,286,339`: `clippy::type_complexity`
- `crates/ma-signals-windows/src/audio_session.rs:355`: `clippy::arc_with_non_send_sync`
- native targetの同一clippy commandは成功し、Windows通常cross-checkも成功した。

## 着手に必要な文脈

Phase 1 validation head `8115039d3f44e08373c4656e77fd66130dc7553e` から存在する
Windows専用コード。capture runtime remediationの宣言scope外であるため、本changeには混ぜない。
