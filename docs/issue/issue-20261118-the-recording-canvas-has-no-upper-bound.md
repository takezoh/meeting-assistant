---
id: issue-20261118-the-recording-canvas-has-no-upper-bound
kind: issue
title: The recording canvas has no upper bound
status: open
created: '2026-11-18'
issue_type: plan
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-measured}
subject_paths:
- crates/app/src/recording/plan.rs
summary: A display or window is encoded at its full size; widths over 4096 exceed
  many hardware H.264 encoders. Whether to bound the long side is a product decision.
---

## Opportunity

A recording is encoded at the size of its display or window, however large. `canvas_size` in `crates/app/src/recording/plan.rs` rounds the size down to even numbers and raises it to a minimum, but sets no maximum.

## Evidence

- On one PC on 2026-11-18 a self-test of a 3840x2160 display was encoded at 3840x2160 and 30 fps; ten minutes of a mostly still desktop came to 96 MB, and the hardware encoder was busy about a third of the time ([the measurement note](../note/note-20261118-windows-recording-measured.md)).
- H.264 level 5.1, the highest that common hardware encoders support, allows at most 4096 pixels across. A wider display or window (an ultrawide or 5K monitor) was not tried; whether recording then fails to start or falls back to a software encoder is unknown.

## Context for follow-up

Whether to bound the long side, and at what size, is the user's decision: a smaller canvas keeps shared slides readable only down to a point. The canvas size is fixed when a recording opens ([capture pipeline design](../design/design-capture-pipeline.md), INV-001), and pictures of another size are scaled into it by `recording::fit`.
