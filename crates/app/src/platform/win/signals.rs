//! Gathers detection signals from Windows.
//! Audio sessions are read from WASAPI, processes from Toolhelp and windows from EnumWindows. Each is read only when
//! detection asks for it.

use std::collections::HashMap;

use anyhow::Context;
use meeting_assistant_core::{AudioSession, ProcessEntry, SignalSource, WindowEntry};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, S_OK};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromRect,
};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, EDataFlow, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WINDOWPLACEMENT, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, Interface, PWSTR};

use super::com::ComApartment;

/// Must keep being used on the thread that created it (COM initialization is bound to the thread).
pub struct WindowsSignalSource {
    enumerator: IMMDeviceEnumerator,
    /// The session manager of each input and each output device, by device ID, kept between reads: activating one
    /// is the costly part of reading the sessions, which happens every second.
    input_managers: HashMap<String, IAudioSessionManager2>,
    output_managers: HashMap<String, IAudioSessionManager2>,
    _com: ComApartment,
}

impl WindowsSignalSource {
    pub fn new() -> anyhow::Result<Self> {
        let com = ComApartment::enter()?;
        let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .context("Cannot start enumerating audio devices")?;
        Ok(Self {
            enumerator,
            input_managers: HashMap::new(),
            output_managers: HashMap::new(),
            _com: com,
        })
    }

    fn active_sessions(&mut self, flow: EDataFlow) -> anyhow::Result<Vec<AudioSession>> {
        let managers = if flow == eCapture {
            &mut self.input_managers
        } else {
            &mut self.output_managers
        };
        let mut result = Vec::new();
        let mut present = Vec::new();
        unsafe {
            let devices = self.enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
            for index in 0..devices.GetCount()? {
                // A device removed during enumeration has no sessions.
                let Ok(device) = devices.Item(index) else { continue };
                let Ok(device_id) = device.GetId().map(|id| take_string(id)) else {
                    continue;
                };
                let manager = match managers.get(&device_id) {
                    Some(manager) => manager.clone(),
                    None => match device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                        Ok(manager) => {
                            managers.insert(device_id.clone(), manager.clone());
                            manager
                        }
                        Err(_) => continue,
                    },
                };
                match device_sessions(&manager, &device_id) {
                    Ok(sessions) => result.extend(sessions),
                    // A manager that stopped answering (the audio service restarted) is made again next time.
                    Err(_) => {
                        managers.remove(&device_id);
                    }
                }
                present.push(device_id);
            }
        }
        managers.retain(|device_id, _| present.contains(device_id));
        Ok(result)
    }
}

impl SignalSource for WindowsSignalSource {
    fn capture_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.active_sessions(eCapture).context("Cannot get microphone usage")
    }

    fn render_sessions(&mut self) -> anyhow::Result<Vec<AudioSession>> {
        self.active_sessions(eRender)
            .context("Cannot get audio playback status")
    }

    fn processes(&mut self) -> anyhow::Result<Vec<ProcessEntry>> {
        processes().context("Cannot list processes")
    }

    fn windows(&mut self) -> anyhow::Result<Vec<WindowEntry>> {
        windows().context("Cannot list windows")
    }
}

unsafe fn device_sessions(
    manager: &IAudioSessionManager2,
    device_id: &str,
) -> windows::core::Result<Vec<AudioSession>> {
    unsafe {
        let sessions = manager.GetSessionEnumerator()?;

        let mut result = Vec::new();
        for index in 0..sessions.GetCount()? {
            let control: IAudioSessionControl2 = sessions.GetSession(index)?.cast()?;
            if control.GetState()? != AudioSessionStateActive || control.IsSystemSoundsSession() == S_OK {
                continue;
            }
            result.push(AudioSession {
                process_id: control.GetProcessId()?,
                device_id: device_id.to_owned(),
            });
        }
        Ok(result)
    }
}

/// Takes the string allocated by COM and frees it.
pub(super) unsafe fn take_string(value: PWSTR) -> String {
    unsafe {
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.as_ptr().cast()));
        text
    }
}

fn processes() -> windows::core::Result<Vec<ProcessEntry>> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut result = Vec::with_capacity(512);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut next = Process32FirstW(snapshot, &mut entry);
        while next.is_ok() {
            result.push(ProcessEntry {
                process_id: entry.th32ProcessID,
                parent_process_id: entry.th32ParentProcessID,
                name: utf16_until_nul(&entry.szExeFile),
            });
            next = Process32NextW(snapshot, &mut entry);
        }
        let _ = CloseHandle(snapshot);
        Ok(result)
    }
}

fn windows() -> windows::core::Result<Vec<WindowEntry>> {
    let mut result: Vec<WindowEntry> = Vec::with_capacity(64);
    unsafe { EnumWindows(Some(visit_window), LPARAM(&raw mut result as isize))? };
    Ok(result)
}

unsafe extern "system" fn visit_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let result = unsafe { &mut *(lparam.0 as *mut Vec<WindowEntry>) };
    if let Some(entry) = unsafe { describe_window(hwnd) } {
        result.push(entry);
    }
    true.into()
}

unsafe fn describe_window(hwnd: HWND) -> Option<WindowEntry> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let length = GetWindowTextLengthW(hwnd);
        if length <= 0 {
            return None;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let copied = GetWindowTextW(hwnd, &mut buffer);
        if copied <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buffer[..copied as usize]);

        let bounds = window_bounds(hwnd)?;
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        let width = (bounds.right - bounds.left).max(0) as u64;
        let height = (bounds.bottom - bounds.top).max(0) as u64;
        Some(WindowEntry {
            id: hwnd.0 as usize as u64,
            process_id,
            title,
            display: display_name(MonitorFromRect(&bounds, MONITOR_DEFAULTTONEAREST)),
            area: width * height,
            pickable: !is_cloaked(hwnd) && !is_tool_window(hwnd),
        })
    }
}

/// Whether the system keeps the window from being shown although it is visible, as it does a suspended app's window
/// or one on another virtual desktop.
unsafe fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let read =
        unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size_of::<u32>() as u32) };
    read.is_ok() && cloaked != 0
}

/// Whether the window is a tool window, which the taskbar and Alt+Tab leave out, as the desktop's own is.
unsafe fn is_tool_window(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    style & WS_EX_TOOLWINDOW.0 != 0
}

/// Window bounds. If minimized, returns the position it would have when restored.
unsafe fn window_bounds(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let mut placement = WINDOWPLACEMENT {
                length: size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                return Some(placement.rcNormalPosition);
            }
        }
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds).ok().map(|_| bounds)
    }
}

/// Display identifier (of the form `\\.\DISPLAY1`).
pub(super) fn display_name(monitor: HMONITOR) -> Option<String> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe { GetMonitorInfoW(monitor, &raw mut info as *mut _) }
        .as_bool()
        .then(|| utf16_until_nul(&info.szDevice))
}

fn utf16_until_nul(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_TOOLWINDOW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };
    use windows::core::{BOOL, HSTRING, w};

    use super::describe_window;

    /// A visible window placed off the screen, so that it shows nowhere; destroyed when dropped.
    struct OffScreenWindow(HWND);

    impl OffScreenWindow {
        fn open(title: &str, extended: WINDOW_EX_STYLE) -> Self {
            unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
                unsafe { DefWindowProcW(window, message, wparam, lparam) }
            }
            unsafe {
                let instance = GetModuleHandleW(None).unwrap();
                let class = w!("meeting-assistant-test-pickable-window");
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    ..Default::default()
                });
                let window = CreateWindowExW(
                    extended,
                    class,
                    &HSTRING::from(title),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    -32000,
                    -32000,
                    400,
                    300,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .unwrap();
                Self(window)
            }
        }
    }

    impl Drop for OffScreenWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    /// Run by hand: `cargo test -p meeting-assistant -- --ignored only_windows_shown_as_their_own`.
    #[test]
    #[ignore = "opens windows on the desktop"]
    fn only_windows_shown_as_their_own_can_be_picked_to_record() {
        let ordinary = OffScreenWindow::open("ordinary", WINDOW_EX_STYLE::default());
        let tool = OffScreenWindow::open("tool", WS_EX_TOOLWINDOW);
        let cloaked = OffScreenWindow::open("cloaked", WINDOW_EX_STYLE::default());
        let cloak = BOOL::from(true);
        unsafe {
            DwmSetWindowAttribute(
                cloaked.0,
                DWMWA_CLOAK,
                (&raw const cloak).cast(),
                size_of::<BOOL>() as u32,
            )
        }
        .unwrap();

        let pickable = |window: &OffScreenWindow| unsafe { describe_window(window.0) }.map(|entry| entry.pickable);

        assert_eq!(pickable(&ordinary), Some(true));
        assert_eq!(
            pickable(&tool),
            Some(false),
            "a tool window is listed for detection only"
        );
        assert_eq!(
            pickable(&cloaked),
            Some(false),
            "a cloaked window is listed for detection only"
        );
        assert_eq!(
            unsafe { describe_window(ordinary.0) }.map(|entry| entry.title),
            Some("ordinary".to_owned())
        );
    }
}
