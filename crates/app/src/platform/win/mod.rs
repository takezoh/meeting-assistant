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
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::{HSTRING, w};

pub use self::install::PlatformInstaller;
pub use self::peers::WindowsLoopbackPeers as PlatformLoopbackPeers;
pub use self::recorder::{PlatformRecorder, recorder};
pub use self::session::{request_quit, watch_exit_requests};
pub use self::signals::WindowsSignalSource as PlatformSignalSource;
use crate::panel::view::{Accent, Design, Surface, TrayLook};

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

/// Asks in a dialog whether to go ahead. Its buttons are Windows' OK and Cancel, so `action` names no button.
pub fn confirm(message: &str, _action: &str) -> bool {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            w!("Meeting Assistant"),
            MB_OKCANCEL | MB_ICONQUESTION,
        ) == IDOK
    }
}

/// Whether the whole screen can be recorded.
pub const SCREEN_CAPTURE: bool = true;

/// The status panel's window is transparent, so that the material the window manager draws behind it shows through
/// the page, and has no taskbar button.
///
/// It is created without the frame that keeps a shadow on a borderless window: on that frame the material does not
/// show, and the page stays white. [`style_panel`] puts the frame back where the material is not available.
pub fn panel_window(builder: tao::window::WindowBuilder) -> tao::window::WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows;
    builder
        .with_skip_taskbar(true)
        .with_undecorated_shadow(false)
        .with_transparent(true)
}

/// Gives the status panel the look of a Windows 11 flyout: rounded corners and the acrylic material of transient
/// windows, both drawn by the window manager, and the user's accent color. Where the material is refused (before
/// Windows 11 2022), the window gets the system's shadow instead and the page draws an opaque background.
pub fn style_panel(window: &tao::window::Window) -> Surface {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };

    let hwnd = HWND(window.hwnd() as *mut std::ffi::c_void);
    // SAFETY: the window handle is valid while `window` lives, and each attribute is read from a value whose size is
    // passed with it.
    let set = |attribute, value: &i32| unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            std::ptr::from_ref(value).cast(),
            size_of::<i32>() as u32,
        )
    };
    if let Err(error) = set(DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND.0) {
        log::info!("The panel keeps square corners: {error}");
    }
    let material = set(DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_TRANSIENTWINDOW.0);
    if let Err(error) = &material {
        log::info!("The panel is drawn without the acrylic material: {error}");
        window.set_undecorated_shadow(true);
    }
    Surface {
        design: Design::Windows,
        translucent: material.is_ok(),
        rounded_by_system: true,
        accent: accent(),
    }
}

/// Asks the WebView2 runtime to hold as little memory as it can for the panel's page while the panel is hidden,
/// and to return to normal when it is shown.
pub fn panel_shown(webview: &wry::WebView, shown: bool) {
    use wry::{MemoryUsageLevel, WebViewExtWindows};
    let level = if shown {
        MemoryUsageLevel::Normal
    } else {
        MemoryUsageLevel::Low
    };
    if let Err(error) = webview.set_memory_usage_level(level) {
        log::info!("Cannot set the panel's memory use: {error}");
    }
}

/// The user's accent color in the shades Windows uses for controls on a light and on a dark background.
fn accent() -> Option<Accent> {
    use windows::UI::ViewManagement::{UIColorType, UISettings};

    let settings = UISettings::new().ok()?;
    let color = |kind| {
        settings
            .GetColorValue(kind)
            .ok()
            .map(|color| [color.R, color.G, color.B])
    };
    Some(Accent {
        light: color(UIColorType::AccentDark1)?,
        dark: color(UIColorType::AccentLight2)?,
    })
}

/// Notification-area icons are drawn in one tone that reads on the taskbar: dark on a light taskbar, white on a dark
/// one. Windows shows no text beside the icon.
pub fn tray_look() -> TrayLook {
    let light_taskbar = registry::get_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "SystemUsesLightTheme",
    ) == Some(1);
    TrayLook {
        idle_color: Some(if light_taskbar { [28, 28, 28] } else { [255, 255, 255] }),
        text_beside_icon: false,
    }
}

/// Shows `image` as the tray icon. Windows has no template icons; the image is drawn in the color it was given.
pub fn set_tray_icon(tray: &tray_icon::TrayIcon, image: tray_icon::Icon, _template: bool) -> tray_icon::Result<()> {
    tray.set_icon(Some(image))
}

/// Nothing to prepare on Windows: the app has no taskbar button because it opens no ordinary window.
pub fn prepare_event_loop<T>(_event_loop: &mut tao::event_loop::EventLoop<T>) {}

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
