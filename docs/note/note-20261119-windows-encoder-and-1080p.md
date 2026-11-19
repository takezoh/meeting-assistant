---
id: note-20261119-windows-encoder-and-1080p
kind: note
title: Windows encoder input and the 1080p limit, 2026-11-19
status: draft
created: '2026-11-19'
tags: []
owners: []
relations:
- {type: references, target: note-20261118-windows-recording-measured}
source_paths:
- crates/app/src/platform/win/recorder/scale.rs
- crates/app/src/platform/win/recorder/writer.rs
- crates/app/src/recording/plan.rs
summary: Which GPUs and H.264 encoders this PC exposes, NV12 pictures encoded by the
  NVIDIA hardware encoder and by Windows' software encoder, and a 4K display recorded
  at 1080p.
topic: verification
---

## Summary

On 2026-11-19 the user asked for recordings of at most 1080p and for hardware encoding on Intel and AMD GPUs as well as NVIDIA. The app now writes at most 1920 by 1080 and hands the encoder NV12 pictures, which every H.264 encoder takes, instead of BGRA, which only some do. On the PC used (the same as in [the measurement note](note-20261118-windows-recording-measured.md)) NV12 was encoded by the NVIDIA hardware encoder and by Windows' software encoder, and a 3840x2160 display was recorded at 1920x1080. No Intel or AMD encoder was available on that PC, so neither was run.

## Notes

### What the PC exposes

The CPU is an Intel Core i7-14700, which has integrated graphics, but Windows listed no Intel display adapter: `pnputil /enum-devices /class Display` showed the NVIDIA GeForce RTX 4060 Ti, the Remote Desktop display adapter and a virtual monitor, and no disconnected display device. The integrated GPU appears to be turned off in the firmware. DXGI listed the NVIDIA adapter three times and Microsoft's software adapter; Media Foundation listed one hardware H.264 encoder, `NVIDIA H.264 Encoder MFT`, registering NV12 and ARGB32 among its inputs.

### Encoding NV12

| Check | Observed |
| --- | --- |
| `pictures_in_nv12_are_encoded_by_the_gpus_encoder_and_by_the_software_encoder` | A red 1280x720 capture became a 640x360 NV12 picture with luma 63 and black luma 16 (BT.709, limited range); `NVIDIA H.264 Encoder MFT (hardware, takes NV12)` and, with hardware transforms turned off, Windows' software encoder `(software, takes NV12)` each wrote 30 pictures to a file |
| `window_pictures_are_cropped_when_odd_and_scaled_when_resized_and_written` | Passed on NV12. The first run failed: the black around a scaled picture came out as luma 0 when the video processor's background color was given in RGB; given in YCbCr it is 16 |
| 30-second self-test of the 3840x2160 display | Recorded at 1920x1080; `NVIDIA H.264 Encoder MFT (hardware, takes NV12)`; no late frame; the encoder engine busy 6.7% of the time, against 32% at 3840x2160 the day before; GPU use 2.4%; sound read 37 ms behind the picture by `tools/avsync`, the same as before within its frame of resolution |

### Not checked

- An Intel or AMD hardware encoder. To run one here, the integrated GPU would have to be turned on in the firmware; a PC with an AMD GPU was not available.
- Which encoder the sink writer chooses on a PC with two GPUs that both have one, and whether it matches the adapter the app captures with.
- A GPU whose video processor cannot write NV12, which falls back to BGRA.
