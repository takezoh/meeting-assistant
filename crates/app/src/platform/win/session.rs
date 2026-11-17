//! Port for receiving quit requests.
//!
//! Owns one invisible window and calls the "finalize the recording and exit" handler in these cases:
//! - Logoff or shutdown (Windows sends WM_ENDSESSION)
//! - A quit request from another process ([`request_quit`] sends WM_CLOSE; used by update and uninstall)

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, MSG, PostMessageW, RegisterClassW,
    TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_ENDSESSION, WM_QUERYENDSESSION, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

const CLASS_NAME: PCWSTR = w!("MeetingAssistant.Control");
/// Upper bound on waiting for the other process to exit after asking it to quit (includes finalizing the recording).
const QUIT_TIMEOUT: Duration = Duration::from_secs(90);

static ON_EXIT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Arranges for `on_exit` to be called when a quit request arrives.
/// `on_exit` must finalize the recording before returning.
/// For a request from another process, this process exits after `on_exit` returns.
pub fn watch_exit_requests(on_exit: impl Fn() + Send + Sync + 'static) {
    if ON_EXIT.set(Box::new(on_exit)).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new().name("session".into()).spawn(|| unsafe {
        let Ok(instance) = GetModuleHandleW(None) else { return };
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class);
        // Never shown. Logoff notifications do not reach message-only windows, so create it as a regular window.
        let created = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            w!("Meeting Assistant"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        );
        if created.is_err() {
            log::warn!("Cannot receive quit notifications. The recording may not be finalized at logoff.");
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    if spawned.is_err() {
        log::warn!("Cannot start the thread that receives quit notifications.");
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            // If wparam is 0, the logoff was canceled.
            if wparam.0 != 0 {
                log::info!("Exiting because of logoff or shutdown.");
                run_on_exit();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            log::info!("Received a quit request.");
            run_on_exit();
            std::process::exit(0);
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn run_on_exit() {
    if let Some(on_exit) = ON_EXIT.get() {
        on_exit();
    }
}

fn find_window() -> Option<HWND> {
    unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()) }
        .ok()
        .filter(|window| !window.is_invalid())
}

/// Asks the resident instance to finalize its recording and quit, and waits for it to finish.
/// Returns `true` if it was not running or its exit was confirmed.
pub fn request_quit() -> bool {
    let Some(window) = find_window() else { return true };
    if unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_err() {
        return false;
    }
    let deadline = Instant::now() + QUIT_TIMEOUT;
    while Instant::now() < deadline {
        if find_window().is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}
