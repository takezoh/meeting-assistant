//! Operations the installer performs on Windows. The procedure itself is decided by `crate::install`.
//! Needs no administrator rights; changes only the current user's scope.
//!
//! | Operation | Implementation |
//! |---|---|
//! | Deployment | `%LOCALAPPDATA%\Programs\MeetingAssistant` |
//! | Listing in the app list | Start menu shortcut and a Settings > Apps entry |
//! | Start at logon | Registry Run key |
//! | Registration with browsers | Per-browser registry key and the manifest file it points to |
//! | Browser launchers | Shortcuts in the Start menu and on the desktop |

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Context;

use super::{registry, session, shell};
use crate::browser::protocol::HOST_NAME;
use crate::install::{APP_NAME, InstallSystem};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "MeetingAssistant";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MeetingAssistant";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A supported browser.
struct Browser {
    name: &'static str,
    /// Registry key (under HKEY_CURRENT_USER) where the relay process is registered.
    native_hosts_key: &'static str,
    /// Candidate executable locations (environment variable, path relative to it).
    executables: &'static [(&'static str, &'static str)],
}

const BROWSERS: [Browser; 2] = [
    Browser {
        name: "Google Chrome",
        native_hosts_key: r"Software\Google\Chrome\NativeMessagingHosts",
        executables: &[
            ("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
            ("ProgramFiles(x86)", r"Google\Chrome\Application\chrome.exe"),
            ("LOCALAPPDATA", r"Google\Chrome\Application\chrome.exe"),
        ],
    },
    Browser {
        name: "Microsoft Edge",
        native_hosts_key: r"Software\Microsoft\Edge\NativeMessagingHosts",
        executables: &[
            ("ProgramFiles(x86)", r"Microsoft\Edge\Application\msedge.exe"),
            ("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
        ],
    },
];

impl Browser {
    fn named(name: &str) -> anyhow::Result<&'static Self> {
        BROWSERS
            .iter()
            .find(|browser| browser.name == name)
            .with_context(|| format!("Unsupported browser: {name}"))
    }

    fn executable(&self) -> Option<PathBuf> {
        self.executables
            .iter()
            .filter_map(|(variable, relative)| Some(PathBuf::from(std::env::var_os(variable)?).join(relative)))
            .find(|path| path.is_file())
    }

    fn native_host_key(&self) -> String {
        format!(r"{}\{HOST_NAME}", self.native_hosts_key)
    }

    fn shortcut_name(&self) -> String {
        format!("{} ({APP_NAME}).lnk", self.name)
    }

    /// Where the launcher shortcuts go (Start menu and desktop).
    fn shortcuts(&self) -> Vec<PathBuf> {
        [Some(start_menu()), dirs::desktop_dir()]
            .into_iter()
            .flatten()
            .map(|folder| folder.join(self.shortcut_name()))
            .collect()
    }
}

fn install_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("Programs")
        .join("MeetingAssistant")
}

fn start_menu() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs")
}

fn app_shortcut() -> PathBuf {
    start_menu().join(format!("{APP_NAME}.lnk"))
}

/// Manifest file handed to the browsers. All browsers point to the same one.
fn native_host_manifest_file() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("MeetingAssistant")
        .join("native-host.json")
}

fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Cannot delete: {}", path.display())),
    }
}

/// Install operations for Windows.
pub struct PlatformInstaller;

impl InstallSystem for PlatformInstaller {
    fn current_executable(&self) -> anyhow::Result<PathBuf> {
        std::env::current_exe().context("Cannot determine the executable's location")
    }

    fn installed_executable(&self) -> PathBuf {
        install_directory().join("meeting-assistant.exe")
    }

    fn extension_directory(&self) -> PathBuf {
        install_directory().join("extension")
    }

    fn is_same_file(&self, a: &Path, b: &Path) -> bool {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
    }

    fn request_quit(&mut self) -> bool {
        session::request_quit()
    }

    fn copy_executable(&mut self, from: &Path, to: &Path) -> anyhow::Result<()> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A process that just exited may still hold the executable, so retry for a short while.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match std::fs::copy(from, to) {
                Ok(_) => return Ok(()),
                Err(error) if Instant::now() >= deadline => {
                    return Err(error).with_context(|| format!("Cannot place the executable: {}", to.display()));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(300)),
            }
        }
    }

    fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content).with_context(|| format!("Cannot write: {}", path.display()))
    }

    fn remove_installed_files(&mut self) -> anyhow::Result<()> {
        let directory = install_directory();
        if !directory.exists() {
            return Ok(());
        }
        if self
            .current_executable()
            .is_ok_and(|current| current.starts_with(&directory))
        {
            // A running executable cannot delete itself, so have it deleted after this process exits.
            Command::new("cmd")
                .args(["/c", "ping", "-n", "3", "127.0.0.1", ">nul", "&", "rmdir", "/s", "/q"])
                .arg(&directory)
                .current_dir(std::env::temp_dir())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .context("Cannot schedule deletion of the install directory")?;
            Ok(())
        } else {
            std::fs::remove_dir_all(&directory).with_context(|| format!("Cannot delete: {}", directory.display()))
        }
    }

    fn launch(&mut self, executable: &Path) -> anyhow::Result<()> {
        shell::launch_detached(executable).context("Cannot launch the installed app")
    }

    fn register_app(&mut self, executable: &Path) -> anyhow::Result<()> {
        shell::create_shortcut(&app_shortcut(), executable, "", APP_NAME)?;
        let entries = [
            ("DisplayName", APP_NAME.to_owned()),
            ("DisplayVersion", env!("CARGO_PKG_VERSION").to_owned()),
            ("DisplayIcon", executable.display().to_string()),
            ("InstallLocation", install_directory().display().to_string()),
            ("UninstallString", format!("\"{}\" --uninstall", executable.display())),
        ];
        for (name, value) in entries {
            registry::set_string(UNINSTALL_KEY, Some(name), &value)?;
        }
        Ok(())
    }

    fn unregister_app(&mut self) -> anyhow::Result<()> {
        registry::delete_tree(UNINSTALL_KEY)?;
        remove_file_if_exists(&app_shortcut())
    }

    fn autostart_enabled(&self) -> bool {
        registry::get_string(RUN_KEY, Some(RUN_VALUE)).is_some()
    }

    fn set_autostart(&mut self, executable: Option<&Path>) -> anyhow::Result<()> {
        match executable {
            Some(executable) => {
                registry::set_string(RUN_KEY, Some(RUN_VALUE), &format!("\"{}\"", executable.display()))
            }
            None => registry::delete_value(RUN_KEY, RUN_VALUE),
        }
    }

    fn supported_browsers(&self) -> Vec<&'static str> {
        BROWSERS.iter().map(|browser| browser.name).collect()
    }

    fn is_browser_present(&self, browser: &str) -> bool {
        Browser::named(browser).is_ok_and(|browser| browser.executable().is_some())
    }

    fn register_native_host(&mut self, browser: &str, manifest: &str) -> anyhow::Result<()> {
        let browser = Browser::named(browser)?;
        let file = native_host_manifest_file();
        self.write_file(&file, manifest)?;
        registry::set_string(&browser.native_host_key(), None, &file.to_string_lossy())
            .with_context(|| format!("Cannot register with {}", browser.name))
    }

    fn unregister_native_host(&mut self, browser: &str) -> anyhow::Result<()> {
        let browser = Browser::named(browser)?;
        registry::delete_tree(&browser.native_host_key())
            .with_context(|| format!("Cannot unregister from {}", browser.name))?;
        remove_file_if_exists(&native_host_manifest_file())
    }

    fn create_browser_launcher(&mut self, browser: &str, option: &str) -> anyhow::Result<String> {
        let browser = Browser::named(browser)?;
        let executable = browser
            .executable()
            .with_context(|| format!("{} not found", browser.name))?;
        for shortcut in browser.shortcuts() {
            shell::create_shortcut(
                &shortcut,
                &executable,
                option,
                "Launches the browser in a state where tabs can be recorded automatically",
            )?;
        }
        Ok(format!(
            "\"{}\" in the Start menu and on the desktop",
            browser.shortcut_name().trim_end_matches(".lnk")
        ))
    }

    fn remove_browser_launcher(&mut self, browser: &str) -> anyhow::Result<()> {
        Browser::named(browser)?
            .shortcuts()
            .iter()
            .try_for_each(|shortcut| remove_file_if_exists(shortcut))
    }
}
