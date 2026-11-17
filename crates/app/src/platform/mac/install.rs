//! Operations the installer performs on macOS. The procedure itself is decided by `crate::install`.
//! Changes only things under the current user's home.
//!
//! | Operation | Implementation |
//! |---|---|
//! | Deployment | `~/Library/Application Support/MeetingAssistant` |
//! | Listing in the app list | none (it only lives in the menu bar, so there is nowhere to register) |
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
        dirs::home_dir()
            .unwrap_or_default()
            .join("Applications")
            .join(format!("{} ({APP_NAME}).app", self.name))
    }
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
fn launch_agent_plist(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCH_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
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
        support_directory().join("meeting-assistant")
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
        // Settings and logs live in the same folder, so do not delete the whole folder.
        remove_if_exists(&self.installed_executable())?;
        remove_if_exists(&self.extension_directory())
    }

    fn launch(&mut self, executable: &Path) -> anyhow::Result<()> {
        // Do not let it inherit the caller's output (it would stay open as long as the resident app runs).
        Command::new(executable)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Cannot launch the installed app")?;
        Ok(())
    }

    fn register_app(&mut self, _executable: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn unregister_app(&mut self) -> anyhow::Result<()> {
        Ok(())
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
