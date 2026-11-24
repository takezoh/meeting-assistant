# Packaging

This reference is for whoever builds the file that users install from. Users' steps are in [INSTALL.md](../../INSTALL.md). The reasons are in [the decision](../adr/adr-20261124-distribute-a-built-app-that-installs-itself.md), and the contract is in [the installation design](../design/design-installation.md).

## macOS

`packaging/macos/build.sh` makes `target/macos/Meeting-Assistant-<version>.dmg`, a disk image that holds the signed `Meeting Assistant.app`. Opened from the disk image, the app asks whether to install itself.

It needs:

- the Xcode Command Line Tools
- Rust 1.89 or later
- a code-signing identity in the login keychain

### The signing identity

macOS keeps the screen recording and microphone permissions only while the app's designated requirement stays the same. For an app signed with a certificate, that requirement is its identifier and the certificate. **Every release must therefore be signed by the same certificate.** A release signed by another one makes every user give both permissions again.

The default identity is the self-signed `Meeting Assistant Local Signing`. Create it once, on the Mac that builds the releases:

1. In Keychain Access, choose **Keychain Access > Certificate Assistant > Create a Certificate**.
2. Enter that name.
3. Choose Identity Type **Self-Signed Root** and Certificate Type **Code Signing**, then create it.

When macOS asks whether `codesign` may use the key, enter the login password and choose **Allow**, not **Always Allow**. Its private key is what macOS recognises as Meeting Assistant on every Mac that installs these builds: whoever can sign with it can make macOS give their program Meeting Assistant's permissions. To build on another Mac, export the identity with its key as a `.p12` file protected by a password and import it there. Do not create a new one.

A self-signed identity does not satisfy Gatekeeper. A disk image downloaded from the web does not open until the user allows it under System Settings > Privacy & Security; see [INSTALL.md](../../INSTALL.md#macos). The releases are signed this way: on 2026-11-25 the user decided not to get a Developer ID.

### Build

```
packaging/macos/build.sh
```

The script does this:

1. Checks that the keychain has the identity, and stops before building if it does not.
2. Runs `cargo build --release --locked`.
3. Puts the executable into `target/macos/Meeting Assistant.app` with `packaging/macos/Info.plist`, with the version from `Cargo.toml` filled in.
4. Signs the bundle as `com.meeting-assistant.app`, with the hardened runtime and the entitlements in `packaging/macos/entitlements.plist`. The hardened runtime is what notarization requires; every build gets it, so that a build behaves the same whoever signs it. Under it the app may use the microphone only through `com.apple.security.device.audio-input`.
5. Puts a copy of the bundle into the disk image, with nothing else in it.

| Option | Effect |
| --- | --- |
| `--identity <name>` | Sign with another identity in the keychain, such as `Developer ID Application: <name> (<team>)` |
| `--notary-profile <profile>` | Also sign the disk image, send it to Apple's notary service and staple the ticket to it. Needs a Developer ID Application identity and a profile stored once with `xcrun notarytool store-credentials <profile>` |
| `--ad-hoc` | Sign without an identity. macOS then drops the app's permissions at every update, and the installer warns about it |

The first release signed with a Developer ID after builds signed with the self-signed identity changes the designated requirement once. The installer then clears the screen recording permission, and macOS asks for both permissions again. Notarization has not been run.

### Check a build

```
codesign -d -vv -r- "target/macos/Meeting Assistant.app"
codesign -d --entitlements - "target/macos/Meeting Assistant.app"
```

A good build has these:

- `flags=0x10000(runtime)`
- the identity as the first `Authority`
- `designated => identifier "com.meeting-assistant.app" and certificate leaf = H"…"`
- the microphone entitlement

The installer refuses a bundle whose signature does not verify, which is what `codesign --verify --strict` checks.

### Install a build on the development Mac

Open the disk image and the app in it. Or install from the bundle in Terminal:

```
"target/macos/Meeting Assistant.app/Contents/MacOS/meeting-assistant" --install
```

`cargo build` followed by `target/release/meeting-assistant --install` no longer installs on macOS: that executable is not in a signed bundle, and the installer refuses it before changing anything. The executable from `cargo build` still runs `--self-test` and `--probe`. See [the development guide](../development.md).

### Not done

- The disk image holds an Apple Silicon build only. An Intel build needs the `x86_64-apple-darwin` target and `lipo`.
- The app has no icon of its own; Finder shows the generic one.
- Nothing is published. Where users download the disk image from is not decided.

## Windows

The Windows release is to be the executable itself, signed at build time once a certificate is available. It is not built yet; [the issue](../issue/issue-20261124-the-windows-installer-is-not-built.md) lists the steps. Until then Windows is installed from a checkout as [INSTALL.md](../../INSTALL.md) describes.
