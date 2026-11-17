mod com;
mod install;
mod recorder;
mod registry;
mod session;
mod shell;
mod signals;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType};
use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW};
use windows::core::{HSTRING, w};

pub use self::install::PlatformInstaller;
pub use self::recorder::{PlatformRecorder, recorder};
pub use self::session::{request_quit, watch_exit_requests};
pub use self::signals::WindowsSignalSource as PlatformSignalSource;

/// Whether a command-line output exists.
///
/// The app is started as a GUI app, so unless output is redirected to a file or pipe,
/// this tries to attach to the caller's console as the output.
/// When launched from Explorer or Settings > Apps, there is no output.
pub fn has_output() -> bool {
    unsafe {
        let redirected = GetStdHandle(STD_OUTPUT_HANDLE)
            .is_ok_and(|handle| !handle.is_invalid() && GetFileType(handle) != FILE_TYPE_UNKNOWN);
        redirected || AttachConsole(ATTACH_PARENT_PROCESS).is_ok()
    }
}

fn message_box(message: &str, style: MESSAGEBOX_STYLE) {
    unsafe {
        MessageBoxW(None, &HSTRING::from(message), w!("Meeting Assistant"), MB_OK | style);
    }
}

/// Reports the reason for a failure in a dialog when there is no output.
pub fn show_error(message: &str) {
    message_box(message, MB_ICONERROR);
}

/// Reports the result in a dialog when there is no output.
pub fn show_info(message: &str) {
    message_box(message, MB_ICONINFORMATION);
}

/// Shows a notification.
///
/// The installer does not register the app, so the sender is shown as "Windows PowerShell".
pub fn notify(title: &str, body: &str) {
    if let Err(error) = notify_rust::Notification::new().summary(title).body(body).show() {
        log::warn!("Cannot show the notification: {error}");
    }
}

/// Limits the resident instance to one per user session.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    /// `None` if another instance is already running.
    pub fn acquire() -> Option<Self> {
        unsafe {
            let handle = CreateMutexW(None, true, w!("Local\\MeetingAssistant.SingleInstance")).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Self(handle))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
