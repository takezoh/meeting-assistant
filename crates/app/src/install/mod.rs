//! Install and uninstall procedures.
//!
//! This module decides what to do and in what order; all interaction with the OS goes through [`InstallSystem`].
//! Each OS implements only the small operations of this port.

use std::path::{Path, PathBuf};

use crate::browser::protocol::{HOST_NAME, browser_launch_option, extension_origin};
use crate::bundle::EXTENSION_FILES;

pub const APP_NAME: &str = "Meeting Assistant";

/// Operations the installer performs on the OS.
pub trait InstallSystem {
    /// Location of this running program.
    fn current_executable(&self) -> anyhow::Result<PathBuf>;
    /// Location of the installed executable.
    fn installed_executable(&self) -> PathBuf;
    /// Directory holding the extension that the browser loads.
    fn extension_directory(&self) -> PathBuf;
    /// Whether two paths refer to the same file.
    fn is_same_file(&self, a: &Path, b: &Path) -> bool;

    /// Asks the resident app to finalize its recording and quit. True if it was not running or its exit was confirmed.
    fn request_quit(&mut self) -> bool;
    fn copy_executable(&mut self, from: &Path, to: &Path) -> anyhow::Result<()>;
    fn write_file(&mut self, path: &Path, content: &str) -> anyhow::Result<()>;
    /// Deletes the installed executable and extension.
    fn remove_installed_files(&mut self) -> anyhow::Result<()>;
    fn launch(&mut self, executable: &Path) -> anyhow::Result<()>;

    /// Lists the app in the OS app lists (Start menu, uninstall list, etc.).
    fn register_app(&mut self, executable: &Path) -> anyhow::Result<()>;
    fn unregister_app(&mut self) -> anyhow::Result<()>;

    fn autostart_enabled(&self) -> bool;
    /// Starts `executable` at logon. `None` disables autostart.
    fn set_autostart(&mut self, executable: Option<&Path>) -> anyhow::Result<()>;

    /// Names of the supported browsers.
    fn supported_browsers(&self) -> Vec<&'static str>;
    /// Whether that browser is in use on this PC.
    fn is_browser_present(&self, browser: &str) -> bool;
    /// Registers this app so the browser can launch it as the extension's relay process.
    fn register_native_host(&mut self, browser: &str, manifest: &str) -> anyhow::Result<()>;
    fn unregister_native_host(&mut self, browser: &str) -> anyhow::Result<()>;
    /// Creates something (e.g. a shortcut) that launches the browser with the launch option, and returns its name.
    fn create_browser_launcher(&mut self, browser: &str, option: &str) -> anyhow::Result<String>;
    fn remove_browser_launcher(&mut self, browser: &str) -> anyhow::Result<()>;
}

/// Definition of the relay process, handed to the browser.
pub fn native_host_manifest(executable: &Path) -> String {
    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": APP_NAME,
        "path": executable,
        "type": "stdio",
        "allowed_origins": [extension_origin()],
    });
    serde_json::to_string_pretty(&manifest).expect("the manifest can always be serialized to JSON")
}

/// Whether the program is running from the install location.
pub fn is_installed(system: &dyn InstallSystem) -> bool {
    system
        .current_executable()
        .is_ok_and(|current| system.is_same_file(&current, &system.installed_executable()))
}

/// Toggles whether the current executable is started at logon.
pub fn set_autostart(system: &mut dyn InstallSystem, enabled: bool) -> anyhow::Result<()> {
    if enabled {
        let executable = system.current_executable()?;
        system.set_autostart(Some(&executable))
    } else {
        system.set_autostart(None)
    }
}

fn quit_running_instance(system: &mut dyn InstallSystem) -> anyhow::Result<()> {
    anyhow::ensure!(
        system.request_quit(),
        "Could not quit the running {APP_NAME}. Quit it and try again."
    );
    Ok(())
}

/// Installs. Overwrites an existing installation (update). Returns a list of what was done.
pub fn install(system: &mut dyn InstallSystem) -> anyhow::Result<Vec<String>> {
    let source = system.current_executable()?;
    let target = system.installed_executable();
    let mut report = Vec::new();

    // Quit the resident app before changing anything, so a file being recorded is not corrupted.
    quit_running_instance(system)?;
    if !system.is_same_file(&source, &target) {
        system.copy_executable(&source, &target)?;
    }
    report.push(format!("Installed the app: {}", target.display()));

    let extension = system.extension_directory();
    for (name, content) in EXTENSION_FILES {
        system.write_file(&extension.join(name), content)?;
    }
    report.push(format!("Installed the browser extension: {}", extension.display()));

    system.register_app(&target)?;
    system.set_autostart(Some(&target))?;
    report.push("Registered in the app list and set to start at logon.".to_owned());

    let manifest = native_host_manifest(&target);
    for browser in system.supported_browsers() {
        if !system.is_browser_present(browser) {
            continue;
        }
        system.register_native_host(browser, &manifest)?;
        let launcher = system.create_browser_launcher(browser, &browser_launch_option())?;
        report.push(format!(
            "Registered for use with {browser} and created a launcher with the launch option: {launcher}"
        ));
    }

    system.launch(&target)?;
    report.push(format!("Started {APP_NAME}."));
    Ok(report)
}

/// Uninstalls. Keeps settings, logs and recordings. Returns a list of what was done.
pub fn uninstall(system: &mut dyn InstallSystem) -> anyhow::Result<Vec<String>> {
    quit_running_instance(system)?;

    system.set_autostart(None)?;
    system.unregister_app()?;
    // Undo for every supported browser, so nothing is left behind for browsers that were used before but are not now.
    for browser in system.supported_browsers() {
        system.unregister_native_host(browser)?;
        system.remove_browser_launcher(browser)?;
    }
    system.remove_installed_files()?;

    Ok(vec![
        format!("Uninstalled {APP_NAME}."),
        "Settings, logs and recordings were kept. Remove the extension from the browser's extensions page.".to_owned(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "downloads/meeting-assistant";
    const TARGET: &str = "installed/meeting-assistant";

    /// A fake that only records the operations performed.
    struct FakeSystem {
        actions: Vec<String>,
        current: &'static str,
        quit_succeeds: bool,
        present: Vec<&'static str>,
        autostart: Option<PathBuf>,
        fails_on: Option<&'static str>,
    }

    impl FakeSystem {
        fn new() -> Self {
            Self {
                actions: Vec::new(),
                current: SOURCE,
                quit_succeeds: true,
                present: vec!["Chrome"],
                autostart: None,
                fails_on: None,
            }
        }

        fn act(&mut self, action: String) -> anyhow::Result<()> {
            let name = action.split_whitespace().next().unwrap_or_default().to_owned();
            self.actions.push(action);
            anyhow::ensure!(self.fails_on != Some(name.as_str()), "{name} failed");
            Ok(())
        }

        /// The kinds of operations performed, in order.
        fn kinds(&self) -> Vec<&str> {
            self.actions
                .iter()
                .map(|action| action.split_whitespace().next().unwrap())
                .collect()
        }

        fn count(&self, kind: &str) -> usize {
            self.kinds().iter().filter(|k| **k == kind).count()
        }
    }

    impl InstallSystem for FakeSystem {
        fn current_executable(&self) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from(self.current))
        }
        fn installed_executable(&self) -> PathBuf {
            PathBuf::from(TARGET)
        }
        fn extension_directory(&self) -> PathBuf {
            PathBuf::from("installed/extension")
        }
        fn is_same_file(&self, a: &Path, b: &Path) -> bool {
            a == b
        }
        fn request_quit(&mut self) -> bool {
            self.actions.push("quit".into());
            self.quit_succeeds
        }
        fn copy_executable(&mut self, from: &Path, to: &Path) -> anyhow::Result<()> {
            self.act(format!("copy {} -> {}", from.display(), to.display()))
        }
        fn write_file(&mut self, path: &Path, _content: &str) -> anyhow::Result<()> {
            self.act(format!("write {}", path.display()))
        }
        fn remove_installed_files(&mut self) -> anyhow::Result<()> {
            self.act("remove-files".into())
        }
        fn launch(&mut self, executable: &Path) -> anyhow::Result<()> {
            self.act(format!("launch {}", executable.display()))
        }
        fn register_app(&mut self, executable: &Path) -> anyhow::Result<()> {
            self.act(format!("register-app {}", executable.display()))
        }
        fn unregister_app(&mut self) -> anyhow::Result<()> {
            self.act("unregister-app".into())
        }
        fn autostart_enabled(&self) -> bool {
            self.autostart.is_some()
        }
        fn set_autostart(&mut self, executable: Option<&Path>) -> anyhow::Result<()> {
            self.autostart = executable.map(Path::to_owned);
            self.act(format!(
                "autostart {}",
                executable.map_or("off".into(), |e| e.display().to_string())
            ))
        }
        fn supported_browsers(&self) -> Vec<&'static str> {
            vec!["Chrome", "Edge"]
        }
        fn is_browser_present(&self, browser: &str) -> bool {
            self.present.contains(&browser)
        }
        fn register_native_host(&mut self, browser: &str, manifest: &str) -> anyhow::Result<()> {
            assert!(
                manifest.contains(TARGET),
                "the manifest points at the installed executable"
            );
            self.act(format!("register-host {browser}"))
        }
        fn unregister_native_host(&mut self, browser: &str) -> anyhow::Result<()> {
            self.act(format!("unregister-host {browser}"))
        }
        fn create_browser_launcher(&mut self, browser: &str, option: &str) -> anyhow::Result<String> {
            self.act(format!("launcher {browser} {option}"))?;
            Ok(format!("{browser} launcher"))
        }
        fn remove_browser_launcher(&mut self, browser: &str) -> anyhow::Result<()> {
            self.act(format!("remove-launcher {browser}"))
        }
    }

    #[test]
    fn install_quits_the_resident_app_then_deploys_and_launches_last() {
        let mut system = FakeSystem::new();
        install(&mut system).unwrap();

        let kinds = system.kinds();
        assert_eq!(kinds.first(), Some(&"quit"));
        assert_eq!(kinds.get(1), Some(&"copy"));
        assert_eq!(kinds.last(), Some(&"launch"));
        assert!(system.actions.contains(&format!("copy {SOURCE} -> {TARGET}")));
        assert!(system.actions.contains(&format!("launch {TARGET}")));
    }

    #[test]
    fn writes_all_extension_files_to_the_extension_directory() {
        let mut system = FakeSystem::new();
        install(&mut system).unwrap();

        assert_eq!(system.count("write"), EXTENSION_FILES.len());
        assert!(
            system
                .actions
                .iter()
                .any(|a| a.starts_with("write") && a.ends_with("manifest.json"))
        );
        assert!(
            system
                .actions
                .iter()
                .filter(|a| a.starts_with("write"))
                .all(|a| a.contains("installed"))
        );
    }

    #[test]
    fn autostart_points_at_the_installed_executable() {
        let mut system = FakeSystem::new();
        install(&mut system).unwrap();

        assert_eq!(system.autostart, Some(PathBuf::from(TARGET)));
        assert!(system.actions.contains(&format!("register-app {TARGET}")));
    }

    #[test]
    fn registers_and_creates_launchers_only_for_browsers_in_use() {
        let mut system = FakeSystem::new();
        let report = install(&mut system).unwrap();

        assert!(system.actions.contains(&"register-host Chrome".to_owned()));
        assert!(
            system
                .actions
                .contains(&format!("launcher Chrome {}", browser_launch_option()))
        );
        assert!(!system.actions.iter().any(|a| a.contains("Edge")));
        assert!(report.iter().any(|line| line.contains("Chrome launcher")));
    }

    #[test]
    fn does_not_copy_the_executable_when_run_from_the_install_location() {
        let mut system = FakeSystem::new();
        system.current = TARGET;
        install(&mut system).unwrap();

        assert_eq!(system.count("copy"), 0);
        assert_eq!(system.count("launch"), 1, "relaunch after an update too");
    }

    #[test]
    fn changes_nothing_if_the_resident_app_cannot_be_quit() {
        let mut system = FakeSystem::new();
        system.quit_succeeds = false;

        assert!(install(&mut system).is_err());
        assert_eq!(system.kinds(), ["quit"]);

        system.actions.clear();
        assert!(uninstall(&mut system).is_err());
        assert_eq!(system.kinds(), ["quit"]);
    }

    #[test]
    fn stops_at_the_first_failure_and_does_not_launch_the_app() {
        let mut system = FakeSystem::new();
        system.fails_on = Some("register-host");

        assert!(install(&mut system).is_err());
        assert_eq!(system.count("launch"), 0);
    }

    #[test]
    fn uninstall_undoes_all_registrations_before_deleting_files() {
        let mut system = FakeSystem::new();
        system.autostart = Some(PathBuf::from(TARGET));
        uninstall(&mut system).unwrap();

        assert_eq!(system.kinds().first(), Some(&"quit"));
        assert_eq!(system.kinds().last(), Some(&"remove-files"));
        assert_eq!(system.autostart, None);
        assert_eq!(system.count("unregister-app"), 1);
        for browser in ["Chrome", "Edge"] {
            assert!(
                system.actions.contains(&format!("unregister-host {browser}")),
                "also undo for browsers that are not in use"
            );
            assert!(system.actions.contains(&format!("remove-launcher {browser}")));
        }
    }

    #[test]
    fn detects_whether_running_from_the_install_location() {
        let mut system = FakeSystem::new();
        assert!(!is_installed(&system));

        system.current = TARGET;
        assert!(is_installed(&system));
    }

    #[test]
    fn autostart_toggle_targets_the_current_executable() {
        let mut system = FakeSystem::new();

        set_autostart(&mut system, true).unwrap();
        assert_eq!(system.autostart, Some(PathBuf::from(SOURCE)));
        assert!(system.autostart_enabled());

        set_autostart(&mut system, false).unwrap();
        assert!(!system.autostart_enabled());
    }

    #[test]
    fn relay_manifest_points_only_at_the_executable_and_this_extension() {
        let manifest: serde_json::Value = serde_json::from_str(&native_host_manifest(Path::new(TARGET))).unwrap();

        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["path"], TARGET);
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(manifest["allowed_origins"], serde_json::json!([extension_origin()]));
    }
}
