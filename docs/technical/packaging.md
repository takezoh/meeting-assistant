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

`packaging\windows\build.ps1` makes `target\windows\Meeting-Assistant-<version>.exe`, the executable users install from. Opened from anywhere but the install location, it asks whether to install itself. The installer copies it as `meeting-assistant.exe`, so the file may be renamed.

It needs:

- Rust with the MSVC toolchain, as any build on Windows does
- Windows PowerShell 5.1, which comes with Windows, or PowerShell 7
- to sign: a code-signing certificate in the current user's certificate store, and `signtool` from the Windows SDK

### The executable's resources

`crates/app/build.rs` gives every build of the executable its resources, also one made by `cargo build` alone, so Explorer, Settings > Apps, Task Manager and SmartScreen show a name and an icon instead of a bare file name:

- version information: the product name, the version from `Cargo.toml` and the file name `meeting-assistant.exe`
- the app icon in six sizes from 16 to 256 pixels, drawn by `crates/app/windows/resources.rs`: a red dot inside a light ring on a dark disc
- the application manifest in `crates/app/windows/app.manifest`: the app runs with the user's own rights and is never elevated, it is aware of each monitor's scaling from the start, and its dialogs have the current look

`build.rs` writes them as a compiled resource file, which the MSVC linker takes as an input, so no resource compiler is needed. With another linker the executable has none, and the build warns.

### Build

```
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
```

Windows runs no PowerShell script by default; `-ExecutionPolicy Bypass` lets this one run, for this command only. The script does this:

1. When a certificate is named, checks that the store has it and that `signtool` can be found, and stops before building if not.
2. Runs `cargo build --release --locked`.
3. Copies the executable to `target\windows\Meeting-Assistant-<version>.exe`, and checks that the version it carries is the one in `Cargo.toml`.
4. When a certificate is named, signs the copy with SHA-256 and a timestamp, then verifies the signature.

| Option | Effect |
| --- | --- |
| `-Certificate <subject>` | Sign with the code-signing certificate in the current user's store whose subject contains this |
| `-TimestampUrl <url>` | The RFC 3161 timestamp server of the certificate's authority, needed with `-Certificate`. The signature then stays valid after the certificate expires |

### Signing

Windows ties no permission to the signature, so an unsigned build installs and runs, and an update keeps the app's settings whoever signed it. SmartScreen warns about a downloaded program it does not know, saying "Windows protected your PC" with **Run anyway** under **More info**, and a signature lets it build a reputation for the publisher instead of for each file. The releases are not signed: no code-signing certificate is available. Signing has not been run.

### Check a build

```
(Get-Item target\windows\Meeting-Assistant-<version>.exe).VersionInfo | Format-List ProductName, ProductVersion, OriginalFilename
signtool verify /pa target\windows\Meeting-Assistant-<version>.exe
```

A good build names Meeting Assistant, the version and `meeting-assistant.exe`, and Explorer shows its icon. The second command applies to a signed build. The test `the_executable_carries_its_version_icon_and_manifest` reads the same resources from the test executable, which `build.rs` gives them as well.

### Install a build on the development PC

Open the executable, or run it with `--install` in a terminal. `cargo build --release` followed by `target\release\meeting-assistant.exe --install` installs the same program under the same name.

### Not done on Windows

- The executable is built for x64 only. An ARM64 build needs the `aarch64-pc-windows-msvc` target.
- Nothing is published. Where users download the executable from is not decided.
