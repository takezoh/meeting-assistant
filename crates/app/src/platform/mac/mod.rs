//! macOS implementation.
//!
//! Detection reads Core Audio, libproc and Core Graphics (`signals`); recording uses ScreenCaptureKit, AVAudioEngine
//! and AVAssetWriter (`recorder`). Browser meetings are recorded by the extension.

mod install;
mod peers;
mod recorder;
mod signals;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

pub use self::install::PlatformInstaller;
pub use self::peers::MacLoopbackPeers as PlatformLoopbackPeers;
pub use self::recorder::{PlatformRecorder, recorder};
pub use self::signals::MacSignalSource as PlatformSignalSource;
use crate::install::APP_NAME;
use crate::panel::view::Surface;

/// Upper bound on waiting for the other process to exit after asking it to quit (includes finalizing the recording).
const QUIT_TIMEOUT: Duration = Duration::from_secs(90);

/// Where the app's data lives (`~/Library/Application Support/MeetingAssistant`).
fn support_directory() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("MeetingAssistant")
}

fn lock_file() -> PathBuf {
    support_directory().join("instance.lock")
}

/// Limits the resident instance to one per user. Writes its own process ID into the locked file.
pub struct SingleInstance(#[allow(dead_code)] File);

impl SingleInstance {
    /// `None` if another instance is already running.
    pub fn acquire() -> Option<Self> {
        let path = lock_file();
        std::fs::create_dir_all(path.parent()?).ok()?;
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .ok()?;
        file.try_lock().ok()?;
        file.set_len(0).ok()?;
        write!(file, "{}", std::process::id()).ok()?;
        file.flush().ok()?;
        Some(Self(file))
    }
}

/// Process ID of the resident instance. `None` if it is not running.
fn running_instance() -> Option<i32> {
    let mut file = File::options().read(true).write(true).open(lock_file()).ok()?;
    // If the lock can be taken, no instance holds it.
    if file.try_lock().is_ok() {
        return None;
    }
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_string(&mut text).ok()?;
    text.trim().parse().ok()
}

/// Asks the resident instance to finalize its recording and quit, and waits for it to finish.
/// Returns `true` if it was not running or its exit was confirmed.
pub fn request_quit() -> bool {
    let Some(process_id) = running_instance() else {
        return true;
    };
    if unsafe { libc::kill(process_id, libc::SIGTERM) } != 0 {
        return false;
    }
    let deadline = Instant::now() + QUIT_TIMEOUT;
    while Instant::now() < deadline {
        if running_instance().is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// On a termination signal, calls `on_exit` and then exits the process. `on_exit` must finalize the recording
/// before returning.
///
/// SIGTERM comes from [`request_quit`] and from launchd when it stops the login agent; SIGINT and SIGHUP come
/// from a terminal the app was started from. A request to terminate the application (logout, shutdown, a quit
/// sent by another app) is not a signal; it ends the tray's event loop, which finalizes the recording itself.
pub fn watch_exit_requests(on_exit: impl Fn() + Send + Sync + 'static) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

    let mut signals = match signal_hook::iterator::Signals::new([SIGTERM, SIGINT, SIGHUP]) {
        Ok(signals) => signals,
        Err(error) => {
            log::warn!("Cannot receive quit notifications: {error}");
            return;
        }
    };
    let spawned = std::thread::Builder::new().name("signals".into()).spawn(move || {
        if signals.forever().next().is_some() {
            log::info!("Received a quit request.");
            on_exit();
            std::process::exit(0);
        }
    });
    if spawned.is_err() {
        log::warn!("Cannot start the thread that receives quit notifications.");
    }
}

/// Whether a command-line output exists: a terminal, or output redirected to a file or pipe.
///
/// There is none when started from Finder, at login or from the tray menu of an app started that way: launchd
/// and LaunchServices connect standard output to `/dev/null`, and a child process inherits it.
pub fn has_output() -> bool {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fstat` fills `status` when it returns 0, and it is read only then.
    let mode = unsafe {
        if libc::fstat(libc::STDOUT_FILENO, status.as_mut_ptr()) != 0 {
            return false;
        }
        status.assume_init().st_mode & libc::S_IFMT
    };
    match mode {
        // `/dev/null` is a character device too; only a terminal counts.
        libc::S_IFCHR => unsafe { libc::isatty(libc::STDOUT_FILENO) == 1 },
        libc::S_IFREG | libc::S_IFIFO | libc::S_IFSOCK => true,
        _ => false,
    }
}

/// Escapes text so it can be safely embedded as an AppleScript string.
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn run_applescript(script: &str) {
    if let Err(error) = Command::new("osascript").arg("-e").arg(script).status() {
        log::warn!("Cannot run osascript: {error}");
    }
}

fn dialog(message: &str, icon: &str) {
    run_applescript(&format!(
        "display dialog {} with title {} buttons {{\"OK\"}} default button 1 with icon {icon}",
        applescript_string(message),
        applescript_string(APP_NAME),
    ));
}

/// Reports the reason for a failure in a dialog when there is no output.
pub fn show_error(message: &str) {
    dialog(message, "stop");
}

/// Reports the result in a dialog when there is no output.
pub fn show_info(message: &str) {
    dialog(message, "note");
}

/// Whether the whole screen can be recorded.
pub const SCREEN_CAPTURE: bool = true;

/// The status panel's window is transparent, so that the system material and the page's rounded edge show.
pub fn panel_window(builder: tao::window::WindowBuilder) -> tao::window::WindowBuilder {
    use tao::platform::macos::WindowBuilderExtMacOS;
    builder.with_transparent(true).with_has_shadow(true)
}

/// Puts the system's translucent popover material behind the status panel, with rounded corners. If the material
/// cannot be applied the page draws an opaque background inside its rounded edge instead.
pub fn style_panel(window: &tao::window::Window) -> Surface {
    use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy};

    let applied = apply_vibrancy(
        window,
        NSVisualEffectMaterial::Popover,
        Some(NSVisualEffectState::Active),
        Some(12.0),
    );
    if let Err(error) = &applied {
        log::warn!("Cannot give the panel a translucent background: {error}");
    }
    Surface {
        translucent: applied.is_ok(),
        rounded_by_system: false,
    }
}

/// Shows a notification.
pub fn notify(title: &str, body: &str) {
    run_applescript(&format!(
        "display notification {} with title {}",
        applescript_string(body),
        applescript_string(title),
    ));
}
