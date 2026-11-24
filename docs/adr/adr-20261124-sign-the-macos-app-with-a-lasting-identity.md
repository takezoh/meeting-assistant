---
id: adr-20261124-sign-the-macos-app-with-a-lasting-identity
kind: adr
title: Sign the macOS app with a lasting local identity so its permissions survive
  updates
status: proposed
created: '2026-11-24'
decision_makers:
- user
tags: []
owners: []
relations: []
source_paths:
- crates/app/src/install/mod.rs
- crates/app/src/platform/mac/install.rs
summary: The installer signs the app with a self-signed code-signing identity from
  the user's keychain, so macOS ties its permissions to the same identity across updates;
  it clears the screen recording permission itself when the identity changes.
---

## Context

macOS ties the screen recording and microphone permissions to an app's code signature, through its designated requirement. Meeting Assistant was installed with only the ad-hoc signature the linker gives its executable, whose designated requirement is the hash of that executable (`cdhash H"…"`). Every build has another hash, so after every `--install` macOS treated the app as a different one:

- The screen recording permission stayed shown as given in System Settings but was refused. Switching it off and on did not help. Only `tccutil reset ScreenCapture com.meeting-assistant.app`, run by the user, followed by granting it again, did ([the issue](../issue/issue-20261119-every-update-drops-the-macos-screen-reco.md)). Until then desktop meetings and manual recordings failed to start.
- macOS asked for the microphone again at the next recording, and that recording waited for the answer.

The user found that no app should make its users run `tccutil`. Ordinary apps are signed with a developer certificate. Their designated requirement names the identifier and the certificate, which stay the same across updates, so macOS keeps their permissions.

## Decision

The installer signs the whole app bundle as `com.meeting-assistant.app` with the code-signing identity named `Meeting Assistant Local Signing` from the user's keychain. This is a self-signed certificate created once on the Mac; INSTALL.md says how. The designated requirement then is `identifier "com.meeting-assistant.app" and certificate leaf = H"…"`, which does not change when the app is rebuilt and installed again.

- Signing comes after the bundle's `Info.plist` is written, which the signature seals, and before anything runs the app.
- Without the identity, or when it cannot be used, the installer signs the bundle ad hoc and says that the permissions will have to be given again after every update.
- The installer reads the app's designated requirement before and after it installs. When it changed, it runs `tccutil reset ScreenCapture com.meeting-assistant.app` itself, so that macOS asks for the screen recording permission again instead of silently refusing it. The microphone is not reset: macOS asks for it again by itself.
- A failure to sign or to reset is reported, and the installation goes on, since the app runs without either.
- The procedure is in `crates/app/src/install/mod.rs` over three new `InstallSystem` operations. Windows ties no permission to a signature, so its operations do nothing.

A Developer ID certificate from the Apple Developer Program would serve the same purpose and is needed to give the app to others (it also allows notarization). It needs a paid membership, so it is left until the app is distributed.

## Consequences

- Updates no longer drop the permissions. On one Mac (macOS 26.6.2) on 2026-11-24, the permissions were given once after switching to the identity. Then a different build was installed: it recorded the screen and the microphone with no prompt, and no change to the app's permissions was logged.
- Switching from the ad-hoc signature to the identity, or to another identity, changes the designated requirement once. The installer clears the screen recording permission then. The user gives it again, and answers macOS's question about the microphone.
- The identity's private key is what macOS recognises as Meeting Assistant. A program that can sign with it can be given Meeting Assistant's permissions. The key stays in the user's login keychain, and using it should need the user's confirmation, not "Always Allow".
- A Mac without the identity behaves as before, except that it is told why and is no longer left with a permission that is shown as given but refused.
