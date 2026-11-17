//! Gathers detection signals from Windows.
//! Audio sessions are read from WASAPI, processes from Toolhelp and windows from EnumWindows.

use anyhow::Context;
use meeting_assistant_core::{AudioSession, ProcessEntry, SignalSnapshot, SignalSource, WindowEntry};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, S_OK};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromRect,
};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, EDataFlow, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, WINDOWPLACEMENT,
};
use windows::core::{BOOL, Interface, PWSTR};

use super::com::ComApartment;

/// Must keep being used on the thread that created it (COM initialization is bound to the thread).
pub struct WindowsSignalSource {
    enumerator: IMMDeviceEnumerator,
    _com: ComApartment,
}

impl WindowsSignalSource {
    pub fn new() -> anyhow::Result<Self> {
        let com = ComApartment::enter()?;
        let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .context("Cannot start enumerating audio devices")?;
        Ok(Self { enumerator, _com: com })
    }

    fn active_sessions(&self, flow: EDataFlow) -> anyhow::Result<Vec<AudioSession>> {
        let mut result = Vec::new();
        unsafe {
            let devices = self.enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
            for index in 0..devices.GetCount()? {
                // If a device is removed during enumeration, treat it as having no sessions.
                if let Ok(device) = devices.Item(index)
                    && let Ok(sessions) = device_sessions(&device)
                {
                    result.extend(sessions);
                }
            }
        }
        Ok(result)
    }
}

impl SignalSource for WindowsSignalSource {
    fn capture(&mut self) -> anyhow::Result<SignalSnapshot> {
        Ok(SignalSnapshot {
            capture_sessions: self.active_sessions(eCapture).context("Cannot get microphone usage")?,
            render_sessions: self
                .active_sessions(eRender)
                .context("Cannot get audio playback status")?,
            processes: processes().context("Cannot list processes")?,
            windows: windows().context("Cannot list windows")?,
            // Browser extension information is added by the caller through a separate path.
            ..Default::default()
        })
    }
}

unsafe fn device_sessions(device: &IMMDevice) -> windows::core::Result<Vec<AudioSession>> {
    unsafe {
        let device_id = take_string(device.GetId()?);
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;

        let mut result = Vec::new();
        for index in 0..sessions.GetCount()? {
            let control: IAudioSessionControl2 = sessions.GetSession(index)?.cast()?;
            if control.GetState()? != AudioSessionStateActive || control.IsSystemSoundsSession() == S_OK {
                continue;
            }
            result.push(AudioSession {
                process_id: control.GetProcessId()?,
                device_id: device_id.clone(),
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
        })
    }
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
