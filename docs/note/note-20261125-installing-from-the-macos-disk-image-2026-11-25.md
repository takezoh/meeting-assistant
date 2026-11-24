---
id: note-20261125-installing-from-the-macos-disk-image-2026-11-25
kind: note
title: Installing from the macOS disk image, 2026-11-25
status: draft
created: '2026-11-25'
tags: []
owners: []
relations:
- {type: references, target: adr-20261124-distribute-a-built-app-that-installs-itself}
source_paths:
- packaging/macos/build.sh
- crates/app/src/install/mod.rs
- crates/app/src/platform/mac/install.rs
summary: The app was installed from the disk image on one Mac, declined once and installed
  over itself; it kept its permissions and recorded the screen and the microphone.
  Gatekeeper was not run.
---

## Summary

On 2026-11-25 the user opened a disk image made by `packaging/macos/build.sh` and installed the app from it. The Mac was an Apple Silicon Mac with macOS 26.6.2, and the build had the uncommitted changes on top of commit 6eb4f22.

- The question appeared. Install replaced the installed app and started the new one, and Cancel changed nothing.
- The app placed was the bundle as it was built.
- macOS kept both permissions: a manual screen recording with the microphone started without a prompt, and its microphone track held sound.

## Notes

### The build

`packaging/macos/build.sh` ran with its default identity, `Meeting Assistant Local Signing`, on 2026-11-24 at 23:58.

- **Output:** `target/macos/Meeting-Assistant-0.1.0.dmg`, about 2 MB, holding only `Meeting Assistant.app`.
- **Signature:** the app showed `flags=0x10000(runtime)` and the identity as its `Authority`, and the entitlement `com.apple.security.device.audio-input`.
- **Designated requirement:** `identifier "com.meeting-assistant.app" and certificate leaf = H"d2cfdf94…"`. This is the same as that of the app installed before, which the earlier installer had signed on the Mac with the same identity.

The disk image was built on this Mac and opened with `open`, so it carried no quarantine and Gatekeeper did not check it.

### Installing

The app's log recorded, at 00:32:59, 00:33:26 and 00:33:47, three openings of the app in the disk image:

1. **Install.** The running installed app received the quit request. The installer placed the bundle, wrote the extension, the login item, the native hosts and both launchers, and started the installed app at 00:33:00. Its report named the signer: `The app is signed by "Meeting Assistant Local Signing", so its permissions are kept across updates.`
2. **Cancel.** The log says `The installation was declined.` The installed app kept running; it was the one that received the next quit request.
3. **Install over the copy installed a minute earlier.** This is the update path. The new copy started at 00:33:47.

Afterwards `codesign --verify --strict` passed for `~/Applications/Meeting Assistant.app`. It had the hardened runtime and the designated requirement above, and its `Info.plist` named version 0.1.0.

### Recording after the installation

At 00:34:52 the user recorded the entire screen from the installed app's panel for 4.3 s:

- The recording started at once, with no prompt.
- No warning about a missing sound was logged.
- `log show` had no entry of the TCC subsystem that named the app from 00:32 on, so macOS neither cleared nor asked for a permission.
- The file `2026-11-25_00-34-52_Manual.mp4` has H.264 video at 1920x1080 and two AAC tracks of 4.3 s.
- The microphone track measured a mean of −50.4 dB and a maximum of −27.9 dB, so it held sound. The output-audio track was silent (−91 dB); nothing was playing.
- So the microphone works under the hardened runtime with the entitlement.

### Not run

- Gatekeeper on a downloaded disk image, which carries quarantine: whether the app in it opens only after **Open Anyway**, and whether the installed copy has to be allowed again.
- Another Mac, an Intel Mac, a Developer ID signature and notarization.
- Running `--install` from the executable of `cargo build`, which should be refused before anything changes. In this session that command was blocked before it ran, so only the test against the fake covers it.
- The browser extension and the launchers after this installation, the login item at the next login, and `--uninstall`.
