//! Operations the installer performs on macOS. The procedure itself is decided by `crate::install`.
//! Changes only things under the current user's home.
//!
//! | Operation | Implementation |
//! |---|---|
//! | Deployment | The app bundle, as `packaging/macos/build.sh` built and signed it, copied to `~/Applications/Meeting Assistant.app`; the extension in `~/Library/Application Support/MeetingAssistant` |
//! | Listing in the app list | The bundle's own `Info.plist`, which makes it an app for Finder, Launchpad and Spotlight |
//! | Start at login | Definition file in `~/Library/LaunchAgents` |
//! | Registration with browsers | Manifest file in each browser's `NativeMessagingHosts` folder |
//! | Browser launchers | Small apps in `~/Applications` |

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Context;

use super::{request_quit, support_directory};
use crate::browser::protocol::HOST_NAME;
use crate::install::{APP_NAME, InstallSystem, Signature};

const LAUNCH_AGENT_LABEL: &str = "com.meeting-assistant.agent";
/// The identifier in the bundle's `Info.plist` (`packaging/macos/Info.plist`), which the build also signs the app as.
const APP_BUNDLE_IDENTIFIER: &str = "com.meeting-assistant.app";
/// The executable in the bundle, named by `CFBundleExecutable`.
const EXECUTABLE_NAME: &str = "meeting-assistant";

/// Runs a command-line tool of macOS without input. Returns what it printed, standard output and then standard error
/// (`codesign -d` prints its details to standard error), or an error with its standard error.
fn run_tool(program: &str, args: &[&std::ffi::OsStr]) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Cannot run {program}"))?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// The designated requirement in what `codesign -d -r-` prints, which is the identity macOS ties permissions to. An
/// implicit one, as a linker-signed executable has, is marked with `#`.
fn designated_requirement(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, requirement) = line.trim_start_matches(['#', ' ']).split_once("designated => ")?;
        Some(requirement.trim().to_owned()).filter(|requirement| !requirement.is_empty())
    })
}

/// Who signed the app, from what `codesign -d -vv` prints: the first `Authority` is the certificate that signed it.
fn signer(details: &str) -> Signature {
    let lines = || details.lines().map(str::trim);
    if lines().any(|line| line == "Signature=adhoc") {
        return Signature::AdHoc("it was built without a signing identity".to_owned());
    }
    match lines().find_map(|line| line.strip_prefix("Authority=")) {
        Some(name) => Signature::Lasting(name.to_owned()),
        None => Signature::AdHoc("its signature names no signer".to_owned()),
    }
}

/// A supported browser.
struct Browser {
    /// App name (also the name passed to `open -a`).
    name: &'static str,
    /// That browser's folder under `~/Library/Application Support`.
    support_directory: &'static str,
}

const BROWSERS: [Browser; 2] = [
    Browser {
        name: "Google Chrome",
        support_directory: "Google/Chrome",
    },
    Browser {
        name: "Microsoft Edge",
        support_directory: "Microsoft Edge",
    },
];

impl Browser {
    fn named(name: &str) -> anyhow::Result<&'static Self> {
        BROWSERS
            .iter()
            .find(|browser| browser.name == name)
            .with_context(|| format!("Unsupported browser: {name}"))
    }

    fn support_directory(&self) -> PathBuf {
        dirs::data_dir().unwrap_or_default().join(self.support_directory)
    }

    fn native_host_manifest(&self) -> PathBuf {
        self.support_directory()
            .join("NativeMessagingHosts")
            .join(format!("{HOST_NAME}.json"))
    }

    fn launcher(&self) -> PathBuf {
        applications_directory().join(format!("{} ({APP_NAME}).app", self.name))
    }

    /// The browser's app bundle, if it is in the system or the user's Applications folder.
    fn installed_app(&self) -> Option<PathBuf> {
        [PathBuf::from("/Applications"), applications_directory()]
            .into_iter()
            .map(|folder| folder.join(format!("{}.app", self.name)))
            .find(|app| app.is_dir())
    }
}

/// A string value from an app bundle's Info.plist, read with `plutil`.
fn info_plist_string(app: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}

/// The file named by `CFBundleIconFile`, which may omit its extension, kept to a plain file name.
fn icon_file_name(value: &str) -> Option<String> {
    let name = Path::new(value).file_name()?.to_str()?;
    Some(if Path::new(name).extension().is_some() {
        name.to_owned()
    } else {
        format!("{name}.icns")
    })
}

/// Copies the icon of the app bundle `from` into the bundle `to` and returns the Info.plist entries that name it:
/// the icon file, and the icon in the asset catalog that recent macOS versions prefer.
fn copy_app_icon(from: &Path, to: &Path) -> anyhow::Result<Vec<(&'static str, String)>> {
    let source = from.join("Contents/Resources");
    let target = to.join("Contents/Resources");
    std::fs::create_dir_all(&target).with_context(|| format!("Cannot create: {}", target.display()))?;
    let copy = |name: &str| {
        std::fs::copy(source.join(name), target.join(name))
            .with_context(|| format!("Cannot copy the icon {}", source.join(name).display()))
    };

    let mut entries = Vec::new();
    if let Some(file) = info_plist_string(from, "CFBundleIconFile").and_then(|value| icon_file_name(&value)) {
        copy(&file)?;
        entries.push(("CFBundleIconFile", file));
    }
    if let Some(name) = info_plist_string(from, "CFBundleIconName") {
        copy("Assets.car")?;
        entries.push(("CFBundleIconName", name));
    }
    Ok(entries)
}

/// The user's own Applications folder, which Finder, Launchpad and Spotlight list like `/Applications`.
fn applications_directory() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Applications")
}

fn app_bundle() -> PathBuf {
    applications_directory().join(format!("{APP_NAME}.app"))
}

/// The app bundle that contains `executable`, laid out as `<name>.app/Contents/MacOS/<executable>`.
fn enclosing_bundle(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let laid_out = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents";
    (laid_out && bundle.extension()? == "app").then_some(bundle)
}

fn launch_agent() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Cannot delete: {}", path.display())),
    }
}

/// LaunchAgent definition that starts `executable` at login.
///
/// When the executable is inside an app bundle, `AssociatedBundleIdentifiers` lets System Settings show the
/// login item under the app's name.
fn launch_agent_plist(executable: &Path) -> String {
    let association = if enclosing_bundle(executable).is_some() {
        format!(
            "    <key>AssociatedBundleIdentifiers</key>\n    <array>\n        <string>{APP_BUNDLE_IDENTIFIER}</string>\n    </array>\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCH_AGENT_LABEL}</string>
{association}    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(&executable.to_string_lossy()),
    )
}

/// Info.plist of the launcher app.
/// `icon` holds the Info.plist entries that name the icon copied into the launcher; it may be empty.
fn launcher_info_plist(browser: &str, icon: &[(&str, String)]) -> String {
    let identifier = format!(
        "com.meeting-assistant.launcher.{}",
        browser.to_lowercase().replace(' ', "-")
    );
    let icon: String = icon
        .iter()
        .map(|(key, value)| format!("    <key>{key}</key>\n    <string>{}</string>\n", xml_escape(value)))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>launcher</string>
{icon}    <key>CFBundleIdentifier</key>
    <string>{identifier}</string>
    <key>CFBundleName</key>
    <string>{} ({APP_NAME})</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
"#,
        xml_escape(browser),
    )
}

/// Install operations for macOS.
pub struct PlatformInstaller;

impl InstallSystem for PlatformInstaller {
    fn current_executable(&self) -> anyhow::Result<PathBuf> {
        std::env::current_exe().context("Cannot determine the executable's location")
    }

    fn installed_executable(&self) -> PathBuf {
        app_bundle().join("Contents/MacOS").join(EXECUTABLE_NAME)
    }

    fn extension_directory(&self) -> PathBuf {
        support_directory().join("extension")
    }

    fn is_same_file(&self, a: &Path, b: &Path) -> bool {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
    }

    /// When it runs from an app bundle, which is how the build makes it; an executable built by `cargo build` alone
    /// runs where it is.
    fn installs_when_opened(&self) -> bool {
        self.current_executable()
            .is_ok_and(|executable| enclosing_bundle(&executable).is_some())
    }

    fn request_quit(&mut self) -> bool {
        request_quit()
    }

    /// Copies the whole bundle with `ditto`, which keeps a bundle's signature, symbolic links and extended attributes
    /// as they are, over a bundle deleted first, so that nothing of an earlier version stays in it.
    fn copy_app(&mut self, from: &Path, to: &Path) -> anyhow::Result<()> {
        let source = enclosing_bundle(from).with_context(|| format!("Not inside an app bundle: {}", from.display()))?;
        let target = enclosing_bundle(to).with_context(|| format!("Not inside an app bundle: {}", to.display()))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("Cannot create: {}", parent.display()))?;
        }
        remove_if_exists(target)?;
        run_tool("/usr/bin/ditto", &[source.as_os_str(), target.as_os_str()])
            .with_context(|| format!("Cannot place the app: {}", target.display()))
            .map(drop)
    }

    fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content).with_context(|| format!("Cannot write: {}", path.display()))
    }

    fn remove_installed_files(&mut self) -> anyhow::Result<()> {
        remove_if_exists(&app_bundle())?;
        // Settings and logs live in the same folder as the extension, so do not delete the whole folder.
        remove_if_exists(&self.extension_directory())
    }

    fn launch(&mut self, executable: &Path) -> anyhow::Result<()> {
        let bundle = enclosing_bundle(executable)
            .with_context(|| format!("Not inside an app bundle: {}", executable.display()))?;
        // Through LaunchServices, as Finder would, so the app does not run as a child of this process or of the
        // terminal it was started from. `-g` keeps the focus where it is.
        let output = Command::new("/usr/bin/open")
            .arg("-g")
            .arg(bundle)
            .stdin(Stdio::null())
            .output()
            .context("Cannot run open to launch the installed app")?;
        anyhow::ensure!(
            output.status.success(),
            "Cannot launch the installed app {}: {}",
            bundle.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    /// Nothing to do: the bundle's own `Info.plist` lists it in Finder, Launchpad and Spotlight once it is in
    /// `~/Applications`, and writing into the bundle would break its signature.
    fn register_app(&mut self, _executable: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn unregister_app(&mut self) -> anyhow::Result<()> {
        // Without its Info.plist the folder is no longer an app; the folder itself goes with the installed files.
        remove_if_exists(&app_bundle().join("Contents/Info.plist"))
    }

    fn autostart_enabled(&self) -> bool {
        launch_agent().is_file()
    }

    fn set_autostart(&mut self, executable: Option<&Path>) -> anyhow::Result<()> {
        match executable {
            Some(executable) => self.write_file(&launch_agent(), &launch_agent_plist(executable)),
            None => remove_if_exists(&launch_agent()),
        }
    }

    fn supported_browsers(&self) -> Vec<&'static str> {
        BROWSERS.iter().map(|browser| browser.name).collect()
    }

    /// Whether the browser has ever been used (even if installed, the folder does not exist until it is first run).
    fn is_browser_present(&self, browser: &str) -> bool {
        Browser::named(browser).is_ok_and(|browser| browser.support_directory().is_dir())
    }

    fn register_native_host(&mut self, browser: &str, manifest: &str) -> anyhow::Result<()> {
        self.write_file(&Browser::named(browser)?.native_host_manifest(), manifest)
    }

    fn unregister_native_host(&mut self, browser: &str) -> anyhow::Result<()> {
        remove_if_exists(&Browser::named(browser)?.native_host_manifest())
    }

    fn create_browser_launcher(&mut self, browser: &str, option: &str) -> anyhow::Result<String> {
        let browser = Browser::named(browser)?;
        let app = browser.launcher();
        // A new bundle, so that Finder and the Dock read its icon again instead of one they remember.
        remove_if_exists(&app)?;

        // The browser's own icon. Without it the launcher still works, so a missing icon is only logged.
        let icon = match browser.installed_app() {
            Some(installed) => copy_app_icon(&installed, &app).unwrap_or_else(|error| {
                log::warn!("The launcher for {} has no icon: {error:#}", browser.name);
                Vec::new()
            }),
            None => {
                log::warn!(
                    "The launcher for {} has no icon: the browser is not in an Applications folder",
                    browser.name
                );
                Vec::new()
            }
        };
        self.write_file(
            &app.join("Contents/Info.plist"),
            &launcher_info_plist(browser.name, &icon),
        )?;

        // The option has no effect on a browser that is already running, so it must be quit before using this.
        let executable = app.join("Contents/MacOS/launcher");
        self.write_file(
            &executable,
            &format!("#!/bin/sh\nexec open -a \"{}\" --args {option}\n", browser.name),
        )?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))?;
        Ok(app.display().to_string())
    }

    fn remove_browser_launcher(&mut self, browser: &str) -> anyhow::Result<()> {
        remove_if_exists(&Browser::named(browser)?.launcher())
    }

    /// The app must be a bundle whose signature seals all of it, as the build makes it: macOS ties the app's
    /// permissions to that signature, and a bundle that is not sealed is not the app as it was built.
    fn app_signature(&self, executable: &Path) -> anyhow::Result<Signature> {
        let bundle = enclosing_bundle(executable).with_context(|| {
            format!(
                "{} is not inside an app bundle. Build the app with packaging/macos/build.sh and install it from the \
                 app or the disk image that makes.",
                executable.display()
            )
        })?;
        run_tool(
            "/usr/bin/codesign",
            &["--verify".as_ref(), "--strict".as_ref(), bundle.as_os_str()],
        )
        .with_context(|| format!("The signature of {} does not verify", bundle.display()))?;
        let details = run_tool(
            "/usr/bin/codesign",
            &["-d".as_ref(), "-vv".as_ref(), bundle.as_os_str()],
        )
        .with_context(|| format!("Cannot read the signature of {}", bundle.display()))?;
        Ok(signer(&details))
    }

    fn app_identity(&self, executable: &Path) -> Option<String> {
        let bundle = enclosing_bundle(executable)?;
        let output = run_tool(
            "/usr/bin/codesign",
            &["-d".as_ref(), "-r-".as_ref(), bundle.as_os_str()],
        )
        .ok()?;
        designated_requirement(&output)
    }

    /// Only the screen recording permission: macOS asks for the microphone again by itself when the identity changes,
    /// but keeps refusing to record the screen while it shows the earlier permission as given.
    fn forget_permissions(&mut self) -> anyhow::Result<()> {
        run_tool(
            "/usr/bin/tccutil",
            &[
                "reset".as_ref(),
                "ScreenCapture".as_ref(),
                APP_BUNDLE_IDENTIFIER.as_ref(),
            ],
        )
        .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &str = "/Users/someone/Applications/Meeting Assistant.app/Contents/MacOS/meeting-assistant";

    #[test]
    fn reads_the_designated_requirement_whether_explicit_or_implicit() {
        assert_eq!(
            designated_requirement(
                "designated => identifier \"com.meeting-assistant.app\" and certificate leaf = H\"d2cf\"\n"
            )
            .as_deref(),
            Some("identifier \"com.meeting-assistant.app\" and certificate leaf = H\"d2cf\"")
        );
        assert_eq!(
            designated_requirement("# designated => cdhash H\"7d0b\"\n").as_deref(),
            Some("cdhash H\"7d0b\"")
        );
        assert_eq!(designated_requirement(""), None);
    }

    #[test]
    fn reads_who_signed_the_app() {
        let lasting = "Executable=/x/Meeting Assistant.app/Contents/MacOS/meeting-assistant\n\
                       Identifier=com.meeting-assistant.app\nCodeDirectory v=20500 size=1 flags=0x10000(runtime)\n\
                       Authority=Meeting Assistant Local Signing\nSignature size=1234\n";
        assert_eq!(
            signer(lasting),
            Signature::Lasting("Meeting Assistant Local Signing".into())
        );

        let developer_id = "Authority=Developer ID Application: Someone (TEAM123456)\n\
                            Authority=Developer ID Certification Authority\nAuthority=Apple Root CA\n";
        assert_eq!(
            signer(developer_id),
            Signature::Lasting("Developer ID Application: Someone (TEAM123456)".into())
        );

        assert!(matches!(
            signer("Identifier=com.meeting-assistant.app\nSignature=adhoc\n"),
            Signature::AdHoc(_)
        ));
    }

    #[test]
    fn finds_the_bundle_only_for_an_executable_laid_out_inside_one() {
        assert_eq!(
            enclosing_bundle(Path::new(BUNDLED)),
            Some(Path::new("/Users/someone/Applications/Meeting Assistant.app"))
        );
        assert_eq!(
            enclosing_bundle(Path::new("/Users/someone/dev/target/release/meeting-assistant")),
            None
        );
        assert_eq!(
            enclosing_bundle(Path::new("/Users/someone/Odd.app/Contents/Resources/meeting-assistant")),
            None
        );
    }

    /// The `Info.plist` the build puts into the bundle.
    const INFO_PLIST: &str = include_str!("../../../../../packaging/macos/Info.plist");

    /// The value of a key in [`INFO_PLIST`], which has one `<key>` line followed by one value line per entry.
    fn info_plist_value(key: &str) -> Option<&'static str> {
        let mut lines = INFO_PLIST.lines().map(str::trim);
        lines.find(|line| *line == format!("<key>{key}</key>"))?;
        lines.next()
    }

    #[test]
    fn the_installed_executable_is_the_one_the_bundle_names() {
        let executable = PlatformInstaller.installed_executable();

        assert_eq!(enclosing_bundle(&executable), Some(app_bundle().as_path()));
        assert_eq!(
            info_plist_value("CFBundleExecutable"),
            Some(format!("<string>{EXECUTABLE_NAME}</string>").as_str())
        );
    }

    #[test]
    fn the_bundle_is_a_menu_bar_app_with_the_identifier_its_permissions_are_kept_under() {
        assert_eq!(
            info_plist_value("CFBundleIdentifier"),
            Some(format!("<string>{APP_BUNDLE_IDENTIFIER}</string>").as_str())
        );
        assert_eq!(info_plist_value("LSUIElement"), Some("<true/>"));
        // Without it macOS ends the app the moment it opens the microphone.
        assert!(info_plist_value("NSMicrophoneUsageDescription").is_some());
        // The build puts the version in.
        assert_eq!(
            info_plist_value("CFBundleShortVersionString"),
            Some("<string>@VERSION@</string>")
        );
    }

    #[test]
    fn icon_file_name_adds_the_extension_and_keeps_only_the_file_name() {
        assert_eq!(icon_file_name("app.icns").as_deref(), Some("app.icns"));
        assert_eq!(icon_file_name("app").as_deref(), Some("app.icns"));
        assert_eq!(icon_file_name("../../evil.icns").as_deref(), Some("evil.icns"));
        assert_eq!(icon_file_name(""), None);
    }

    #[test]
    fn launcher_names_the_copied_icon_only_when_there_is_one() {
        let with_icon = launcher_info_plist(
            "Google Chrome",
            &[
                ("CFBundleIconFile", "app.icns".to_owned()),
                ("CFBundleIconName", "AppIcon".to_owned()),
            ],
        );
        assert!(with_icon.contains("<key>CFBundleIconFile</key>\n    <string>app.icns</string>"));
        assert!(with_icon.contains("<key>CFBundleIconName</key>\n    <string>AppIcon</string>"));

        let without_icon = launcher_info_plist("Google Chrome", &[]);
        assert!(!without_icon.contains("CFBundleIcon"));
        assert!(without_icon.contains("<string>Google Chrome (Meeting Assistant)</string>"));
    }

    #[test]
    fn login_item_names_the_app_only_when_the_executable_is_bundled() {
        let bundled = launch_agent_plist(Path::new(BUNDLED));
        assert!(bundled.contains("AssociatedBundleIdentifiers"));
        assert!(bundled.contains(&format!("<string>{BUNDLED}</string>")));

        let loose = launch_agent_plist(Path::new("/tmp/a&b/meeting-assistant"));
        assert!(!loose.contains("AssociatedBundleIdentifiers"));
        assert!(loose.contains("<string>/tmp/a&amp;b/meeting-assistant</string>"));
    }
}
