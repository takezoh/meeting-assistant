//! Operations the installer performs on macOS. The procedure itself is decided by `crate::install`.
//! Changes only things under the current user's home.
//!
//! | Operation | Implementation |
//! |---|---|
//! | Deployment | The executable in `~/Applications/Meeting Assistant.app`, the extension in `~/Library/Application Support/MeetingAssistant` |
//! | Listing in the app list | The bundle's `Info.plist`, which makes it an app for Finder, Launchpad and Spotlight |
//! | Start at login | Definition file in `~/Library/LaunchAgents` |
//! | Registration with browsers | Manifest file in each browser's `NativeMessagingHosts` folder |
//! | Browser launchers | Small apps in `~/Applications` |

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Context;

use super::{request_quit, support_directory};
use crate::browser::protocol::HOST_NAME;
use crate::install::{APP_NAME, InstallSystem};

const LAUNCH_AGENT_LABEL: &str = "com.meeting-assistant.agent";
const APP_BUNDLE_IDENTIFIER: &str = "com.meeting-assistant.app";
const EXECUTABLE_NAME: &str = "meeting-assistant";

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

/// Info.plist of the app. `LSUIElement` keeps it out of the Dock; it lives in the menu bar.
fn app_info_plist(executable_name: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>{APP_NAME}</string>
    <key>CFBundleExecutable</key>
    <string>{}</string>
    <key>CFBundleIdentifier</key>
    <string>{APP_BUNDLE_IDENTIFIER}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#,
        xml_escape(executable_name),
    )
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
fn launcher_info_plist(browser: &str) -> String {
    let identifier = format!(
        "com.meeting-assistant.launcher.{}",
        browser.to_lowercase().replace(' ', "-")
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>launcher</string>
    <key>CFBundleIdentifier</key>
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

    fn request_quit(&mut self) -> bool {
        request_quit()
    }

    fn copy_executable(&mut self, from: &Path, to: &Path) -> anyhow::Result<()> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Delete first, then copy, so a running executable is not overwritten in place.
        remove_if_exists(to)?;
        std::fs::copy(from, to).with_context(|| format!("Cannot place the executable: {}", to.display()))?;
        Ok(())
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

    fn register_app(&mut self, executable: &Path) -> anyhow::Result<()> {
        let bundle = enclosing_bundle(executable)
            .with_context(|| format!("Not inside an app bundle: {}", executable.display()))?;
        let name = executable.file_name().unwrap_or_default().to_string_lossy();
        self.write_file(&bundle.join("Contents/Info.plist"), &app_info_plist(&name))
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
        self.write_file(&app.join("Contents/Info.plist"), &launcher_info_plist(browser.name))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &str = "/Users/someone/Applications/Meeting Assistant.app/Contents/MacOS/meeting-assistant";

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

    #[test]
    fn the_installed_executable_is_inside_the_app_bundle() {
        let executable = PlatformInstaller.installed_executable();

        assert_eq!(enclosing_bundle(&executable), Some(app_bundle().as_path()));
        assert!(app_info_plist(EXECUTABLE_NAME).contains(&format!("<string>{EXECUTABLE_NAME}</string>")));
    }

    #[test]
    fn app_info_plist_identifies_a_menu_bar_app() {
        let plist = app_info_plist("meeting-assistant");

        assert!(plist.contains(&format!("<string>{APP_BUNDLE_IDENTIFIER}</string>")));
        assert!(plist.contains("<key>LSUIElement</key>\n    <true/>"));
        assert!(plist.contains(&format!("<string>{}</string>", env!("CARGO_PKG_VERSION"))));
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
