//! macOS implementation.
//!
//! Supported: meeting detection and tab recording through the browser extension (resident app,
//! connection to the extension, installation, start at login, finalizing the recording on exit).
//! Detecting and recording desktop apps (Zoom, etc.) needs macOS-specific screen and audio capture,
//! which is not implemented yet.
//!
//! Note: this module has only been type-checked on Windows and has never been run on a real Mac.

mod install;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use meeting_assistant_core::{AudioConfig, Recorder, RecordingRequest, SignalSnapshot, SignalSource, VideoConfig};

pub use self::install::PlatformInstaller;
use crate::install::APP_NAME;

/// Upper bound on waiting for the other process to exit after asking it to quit (includes finalizing the recording).
const QUIT_TIMEOUT: Duration = Duration::from_secs(90);

/// Where the app's data lives (`~/Library/Application Support/MeetingAssistant`).
fn support_directory() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("MeetingAssistant")
}

/// There is no OS-level detection yet, so this observes nothing.
/// Meetings reported by the browser extension are added by the caller through a separate path.
pub struct PlatformSignalSource;

impl PlatformSignalSource {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }
}

impl SignalSource for PlatformSignalSource {
    fn capture(&mut self) -> anyhow::Result<SignalSnapshot> {
        Ok(SignalSnapshot::default())
    }
}

/// There is no OS-level recording yet. The caller routes browser tab recording to the extension.
pub struct PlatformRecorder;

/// Creates the recorder.
pub fn recorder(_video: VideoConfig, _audio: AudioConfig) -> PlatformRecorder {
    PlatformRecorder
}

impl Recorder for PlatformRecorder {
    fn start(&mut self, _request: &RecordingRequest) -> anyhow::Result<()> {
        anyhow::bail!("On macOS only tab recording through the browser extension is supported")
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn retarget(&mut self, _window: u64) -> anyhow::Result<()> {
        Ok(())
    }

    fn take_failure(&mut self) -> Option<String> {
        None
    }
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

/// On logout, shutdown, or a quit request from [`request_quit`] (all delivered as SIGTERM),
/// calls `on_exit` and then exits the process. `on_exit` must finalize the recording before returning.
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

/// Whether a command-line output exists. There is none when started by autostart at login or from Finder.
pub fn has_output() -> bool {
    // In those cases the parent process is launchd (process ID 1).
    unsafe { libc::getppid() != 1 }
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

/// Shows a notification.
pub fn notify(title: &str, body: &str) {
    run_applescript(&format!(
        "display notification {} with title {}",
        applescript_string(body),
        applescript_string(title),
    ));
}
