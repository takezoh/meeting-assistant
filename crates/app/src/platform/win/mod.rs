mod com;
mod install;
mod peers;
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
pub use self::peers::WindowsLoopbackPeers as PlatformLoopbackPeers;
pub use self::recorder::{PlatformRecorder, recorder};
pub use self::session::{request_quit, watch_exit_requests};
pub use self::signals::WindowsSignalSource as PlatformSignalSource;
use crate::panel::view::Surface;

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

/// Whether the whole screen can be recorded.
pub const SCREEN_CAPTURE: bool = true;

/// The status panel's window is opaque, has no taskbar button and keeps the system's shadow without a frame.
pub fn panel_window(builder: tao::window::WindowBuilder) -> tao::window::WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows;
    builder.with_skip_taskbar(true).with_undecorated_shadow(true)
}

/// Asks the window manager to round the status panel's corners (Windows 11; earlier versions ignore it).
/// The panel is opaque.
pub fn style_panel(window: &tao::window::Window) -> Surface {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};

    let preference = DWMWCP_ROUND;
    // SAFETY: the window handle is valid while `window` lives, and the attribute is read from `preference`, whose
    // size is passed with it.
    let rounded = unsafe {
        DwmSetWindowAttribute(
            HWND(window.hwnd() as *mut std::ffi::c_void),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&preference).cast(),
            std::mem::size_of_val(&preference) as u32,
        )
    };
    if let Err(error) = &rounded {
        log::info!("The panel keeps square corners: {error}");
    }
    Surface {
        translucent: false,
        rounded_by_system: true,
    }
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
