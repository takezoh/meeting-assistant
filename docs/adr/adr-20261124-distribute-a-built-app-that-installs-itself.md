---
id: adr-20261124-distribute-a-built-app-that-installs-itself
kind: adr
title: Distribute a built and signed app that installs itself
status: proposed
created: '2026-11-24'
decision_makers:
- user
tags: []
owners: []
relations:
- {type: supersedes, target: adr-20261124-sign-the-macos-app-with-a-lasting-identity}
- {type: references, target: adr-20261117-self-installing-per-user-executable}
- {type: references, target: design-installation}
source_paths:
- packaging/macos/build.sh
- crates/app/src/install/mod.rs
- crates/app/src/platform/mac/install.rs
- crates/app/src/main.rs
summary: 'Each platform gets one downloadable file built and signed by the build:
  a disk image with the app bundle on macOS, the executable on Windows. Opened from
  elsewhere, the app asks and installs itself per user; the installer never re-signs
  it.'
---

## Context

Until now Meeting Assistant was installed from a checkout: `cargo build --release`, then `meeting-assistant --install`. No builds were published. On macOS the installer assembled the app bundle on the user's Mac and signed it there with a self-signed identity that each user had to create in Keychain Access first ([the earlier decision](adr-20261124-sign-the-macos-app-with-a-lasting-identity.md)). Someone without Rust, a terminal and that keychain step could not install the app, and no app the user knew asked for such steps. The user asked for an installer design for Windows and macOS, and for the macOS installer to be built.

What the platforms require:

- macOS ties the screen recording and microphone permissions to the app's designated requirement, so every build must be signed by the same identity for the permissions to survive an update. Gatekeeper checks an app that was downloaded: one that is not signed with a Developer ID and notarized by Apple does not open until the user allows it under System Settings > Privacy & Security. The development Mac had no Developer ID identity on 2026-11-24, only the self-signed `Meeting Assistant Local Signing`.
- Windows ties no permission to a signature. SmartScreen warns about an unsigned executable that was downloaded, and lets the user run it anyway.

What stays fixed from [the installation design](../design/design-installation.md): installation is per user and needs no elevation, the app never runs as a service, the running app is stopped (and its recording finalized) before anything is replaced, and the procedure is shared code over the `InstallSystem` port.

## Decision

### One file per platform, made by the build

- **macOS:** `Meeting-Assistant-<version>.dmg`, a disk image holding `Meeting Assistant.app`, made by `packaging/macos/build.sh`. The script builds the release executable and puts it into a bundle with `packaging/macos/Info.plist`. It signs the whole bundle as `com.meeting-assistant.app`, with the hardened runtime and the microphone entitlement in `packaging/macos/entitlements.plist`. The disk image holds only the app, with no alias to `/Applications`.
- **Windows:** the executable, as it is today, with version information and an icon in its resources. This is not built yet ([the issue](../issue/issue-20261124-the-windows-installer-is-not-built.md)).

### The build signs, the installer does not

- **macOS:** the build signs with an identity of whoever builds the releases.
  - By default this is `Meeting Assistant Local Signing`, the self-signed identity of the earlier decision. It is now created once by the builder instead of by every user.
  - `--identity` names a Developer ID Application identity once one exists, and `--notary-profile` then also notarizes the disk image and staples the ticket.
  - `--ad-hoc` builds an app whose permissions are dropped at every update, and must be asked for.
  - The user decided on 2026-11-25 not to get a Developer ID. Releases are therefore signed with the self-signed identity. A user who downloads the disk image allows the app once under System Settings > Privacy & Security, which is part of the install steps. The two options stay for a later change of mind.
- **What the installer does with a macOS app:**
  - It refuses, before changing anything, an app that is not a bundle whose signature verifies.
  - It copies the bundle with `ditto` as it is, writes nothing into it, and reports who signed it.
  - As before, it compares the installed app's designated requirement before and after. When it changed, it clears the screen recording permission, so that macOS asks for it again instead of refusing it silently.
- **Windows:** the executable is unsigned until a code-signing certificate is available. Then the build signs it with `signtool`, timestamped. The installer signs nothing.

### Opened from elsewhere, the app asks and installs itself

The app remains its own installer ([the decision](adr-20261117-self-installing-per-user-executable.md)). New is what happens when a built app is opened without a command from somewhere other than the install location, as from the disk image. It asks whether to install, naming its version, and says that an installed copy is replaced and that a running recording is finished first.

- **Install** runs the procedure of `--install`, then reports in a dialog:
  1. Quit the running copy.
  2. Place the app in `~/Applications`, or in `%LOCALAPPDATA%\Programs\MeetingAssistant` on Windows.
  3. Write the extension.
  4. Set it to start at login.
  5. Register the native-messaging hosts and create the browser launchers.
  6. Start the installed copy.
- **Cancel** changes nothing, and the app exits.
- On macOS, a built app is one that runs from an app bundle. On Windows the executable will ask too once its installer is done; until then it runs and offers the installation in its tray menu, as before.
- A run with `--config` never asks, so a development build can run beside the installed app.

An update is the same step with a newer disk image or executable. Uninstalling is unchanged.

### Options not taken

| Option | Why not |
| --- | --- |
| macOS installer package (`.pkg`) | It needs a second certificate (Developer ID Installer) to pass Gatekeeper. Its pre- and postinstall scripts would describe the installation a second time beside the shared procedure, and would have to stop a recording app from a script |
| Disk image with an `/Applications` alias to drag the app onto | It installs for every user of the Mac. Nothing is registered until the app is opened. Finder replaces the app without finishing its recording |
| Windows MSI (WiX), per user | Its declarative registrations would duplicate the procedure, and it adds a toolchain |
| MSIX | Registry writes are virtualized, so Chrome would not see the native-messaging registration without extra declarations. Start at logon becomes a startup task. The package must be signed to install at all |
| Inno Setup or NSIS around the executable | It adds a toolchain for a wizard around the same `--install` |
| Keep signing at install time | Every user would need a signing identity, and a Developer ID could never be used, since only the builder holds it |

## Consequences

- Positive: a Mac user installs from one file, without Rust, a terminal or Keychain Access. Updates keep the permissions as long as every build is signed by the same identity.
- Positive: building from a checkout and installing from the disk image run the same procedure, which is tested against a fake.
- Positive: a Developer ID and notarization need only two options of the build script, no change to the app.
- Negative: without a Developer ID, macOS on another Mac does not open the app from a downloaded disk image until the user allows it in Privacy & Security. Whether the installed copy has to be allowed again has not been checked.
- Negative: the private key of the signing identity now stands for the app on every Mac that installs these builds. Whoever holds it can sign a program that macOS gives Meeting Assistant's permissions, so it stays with the builder.
- Negative: the hardened runtime now applies to the app. Under it the microphone is available only through the entitlement.
- Negative: the disk image holds an Apple Silicon build only, because the x86_64 macOS target is not installed on the build Mac. Intel Macs cannot run it.
- Negative: on macOS, `cargo build` followed by `--install` no longer installs. Developers run the build script.
- Negative: on macOS, uninstalling still needs `--uninstall` in Terminal. Moving the app to the Trash leaves its login item, native-host manifests and launchers behind.
- Neutral: nothing is published. The disk image is built on the builder's Mac, and where to offer it for download is not decided.

## Confirmation

These tests check the decision:

- **`crates/app/src/install/mod.rs`:** an app whose signature does not verify changes nothing. The app is placed as it was built and its signer is reported. A built app opened from elsewhere offers to install, and the installed app does not.
- **`crates/app/src/platform/mac/install.rs`:** the bundle's `Info.plist` names the identifier and executable the installer uses, and keeps the microphone usage description.

On 2026-11-24 the build script made a disk image on one Mac. Its app carried the hardened runtime and the microphone entitlement, and had the same designated requirement as the app installed there before.

On 2026-11-25 the app was installed from that disk image, declined once, and installed again over itself. The installed app then recorded the screen and the microphone without a permission prompt ([the note](../note/note-20261125-installing-from-the-macos-disk-image-2026-11-25.md)).
