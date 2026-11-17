//! Shell features: creating shortcuts (.lnk) and launching programs.

use std::path::Path;

use anyhow::Context;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::UI::Shell::{IShellLinkW, ShellExecuteW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, Interface, w};

use super::com::ComApartment;

/// Creates a shortcut at `link` that launches `target` with `arguments`. Overwrites an existing one.
pub fn create_shortcut(link: &Path, target: &Path, arguments: &str, description: &str) -> anyhow::Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _com = ComApartment::enter()?;
    unsafe {
        let shortcut: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        shortcut.SetPath(&HSTRING::from(target.as_os_str()))?;
        shortcut.SetArguments(&HSTRING::from(arguments))?;
        shortcut.SetDescription(&HSTRING::from(description))?;
        if let Some(directory) = target.parent() {
            shortcut.SetWorkingDirectory(&HSTRING::from(directory.as_os_str()))?;
        }
        shortcut
            .cast::<IPersistFile>()?
            .Save(&HSTRING::from(link.as_os_str()), true)
            .with_context(|| format!("Cannot create the shortcut: {}", link.display()))?;
    }
    Ok(())
}

/// Launches a program detached from this process.
///
/// Launching as a child process makes it inherit what this process has open (such as output pipes),
/// so the caller cannot close them while the launched program runs. Letting the shell launch it avoids inheritance.
pub fn launch_detached(executable: &Path) -> anyhow::Result<()> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(executable.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // A return value of 32 or less means failure (the value is the error kind).
    anyhow::ensure!(
        result.0 as usize > 32,
        "Cannot launch: {} (ShellExecute = {})",
        executable.display(),
        result.0 as usize
    );
    Ok(())
}
