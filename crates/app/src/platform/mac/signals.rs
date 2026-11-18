//! What meeting detection reads on macOS: which processes use the microphone and play sound (Core Audio process
//! objects, macOS 14.2 and later), the process tree (libproc) and the windows (Core Graphics).
//!
//! These are small reads only. Which of them makes a meeting is decided in core.

use std::ffi::{CStr, c_int, c_void};
use std::mem::size_of;
use std::ptr::{NonNull, null, null_mut};

use anyhow::Context;
use meeting_assistant_core::{AudioSession, ProcessEntry, SignalSnapshot, SignalSource, WindowEntry};
use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertyScope, AudioObjectPropertySelector, kAudioDevicePropertyDeviceUID, kAudioDevicePropertyStreams,
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
    kAudioProcessPropertyDevices, kAudioProcessPropertyIsRunningInput, kAudioProcessPropertyIsRunningOutput,
    kAudioProcessPropertyPID,
};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGGetDisplaysWithPoint, CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo,
    CGWindowListOption, kCGWindowBounds, kCGWindowLayer, kCGWindowName, kCGWindowNumber, kCGWindowOwnerPID,
};

/// Windows smaller than this on either side are toolbars, badges and the like, not something to record.
const MIN_WINDOW_SIDE: f64 = 100.0;

/// Reads the signals for meeting detection.
pub struct MacSignalSource;

impl MacSignalSource {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }
}

impl SignalSource for MacSignalSource {
    fn capture(&mut self) -> anyhow::Result<SignalSnapshot> {
        let mut snapshot = SignalSnapshot {
            processes: processes(),
            windows: windows(),
            ..Default::default()
        };
        for process in audio_processes().context("cannot read which processes use audio")? {
            if let Some(device_id) = process.input {
                snapshot.capture_sessions.push(AudioSession {
                    process_id: process.process_id,
                    device_id,
                });
            }
            if let Some(device_id) = process.output {
                snapshot.render_sessions.push(AudioSession {
                    process_id: process.process_id,
                    device_id,
                });
            }
        }
        Ok(snapshot)
    }
}

/// A process doing audio I/O, with the device it records from and the one it plays to (Core Audio device UIDs;
/// empty when the device cannot be told).
struct AudioProcess {
    process_id: u32,
    input: Option<String>,
    output: Option<String>,
}

fn audio_processes() -> anyhow::Result<Vec<AudioProcess>> {
    let objects: Vec<AudioObjectID> = read_array(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyProcessObjectList,
        kAudioObjectPropertyScopeGlobal,
    )?;
    let mut found = Vec::new();
    for object in objects {
        // A process that exits between the list and these reads is skipped.
        let Ok(process_id) = read_value::<i32>(object, kAudioProcessPropertyPID, kAudioObjectPropertyScopeGlobal)
        else {
            continue;
        };
        let running =
            |selector| read_value::<u32>(object, selector, kAudioObjectPropertyScopeGlobal).is_ok_and(|v| v != 0);
        let (input, output) = (
            running(kAudioProcessPropertyIsRunningInput),
            running(kAudioProcessPropertyIsRunningOutput),
        );
        if !input && !output {
            continue;
        }
        let devices: Vec<AudioObjectID> =
            read_array(object, kAudioProcessPropertyDevices, kAudioObjectPropertyScopeGlobal).unwrap_or_default();
        let device_with = |scope| {
            devices
                .iter()
                .find(|&&device| has_streams(device, scope))
                .and_then(|&device| device_uid(device))
                .unwrap_or_default()
        };
        found.push(AudioProcess {
            process_id: process_id as u32,
            input: input.then(|| device_with(kAudioObjectPropertyScopeInput)),
            output: output.then(|| device_with(kAudioObjectPropertyScopeOutput)),
        });
    }
    Ok(found)
}

fn has_streams(device: AudioObjectID, scope: AudioObjectPropertyScope) -> bool {
    read_array::<AudioObjectID>(device, kAudioDevicePropertyStreams, scope).is_ok_and(|streams| !streams.is_empty())
}

fn device_uid(device: AudioObjectID) -> Option<String> {
    let uid: *const c_void = read_value(device, kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal).ok()?;
    // SAFETY: the property is a CFString the caller owns.
    let uid = unsafe { CFRetained::from_raw(NonNull::new(uid.cast_mut())?.cast::<CFString>()) };
    Some(uid.to_string())
}

fn address(selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Reads a property whose value is one fixed-size value.
fn read_value<T: Copy + Default>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> anyhow::Result<T> {
    let address = address(selector, scope);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` has room for `size` bytes and the address outlives the call.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    anyhow::ensure!(
        status == 0,
        "Core Audio property {selector:#x} could not be read ({status})"
    );
    Ok(value)
}

/// Reads a property whose value is an array of fixed-size values.
fn read_array<T: Copy + Default>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> anyhow::Result<Vec<T>> {
    let address = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: the address outlives the call and `size` is a valid output.
    let status =
        unsafe { AudioObjectGetPropertyDataSize(object, NonNull::from(&address), 0, null(), NonNull::from(&mut size)) };
    anyhow::ensure!(
        status == 0,
        "Core Audio property {selector:#x} could not be sized ({status})"
    );
    let mut values = vec![T::default(); size as usize / size_of::<T>()];
    if values.is_empty() {
        return Ok(values);
    }
    let mut size = (values.len() * size_of::<T>()) as u32;
    // SAFETY: `values` has room for `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(values.as_mut_ptr())
                .expect("a vector's pointer is not null")
                .cast(),
        )
    };
    anyhow::ensure!(
        status == 0,
        "Core Audio property {selector:#x} could not be read ({status})"
    );
    values.truncate(size as usize / size_of::<T>());
    Ok(values)
}

/// Every process with its parent and executable name.
fn processes() -> Vec<ProcessEntry> {
    // SAFETY: a null buffer asks for the number of processes.
    let count = unsafe { libc::proc_listallpids(null_mut(), 0) };
    if count <= 0 {
        return Vec::new();
    }
    // Room for processes started since the count.
    let mut pids = vec![0 as libc::pid_t; count as usize + 64];
    // SAFETY: the buffer is as large as stated.
    let filled = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * size_of::<libc::pid_t>()) as c_int,
        )
    };
    pids.truncate(filled.max(0) as usize);
    pids.into_iter().filter(|&pid| pid > 0).filter_map(process).collect()
}

fn process(pid: libc::pid_t) -> Option<ProcessEntry> {
    // SAFETY: `proc_bsdinfo` is plain data; zeroed is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_bsdinfo>() as c_int;
    // SAFETY: `info` has room for `size` bytes.
    let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if read != size {
        return None;
    }
    let text = |field: &[libc::c_char]| {
        // SAFETY: the kernel writes a NUL-terminated name into the field.
        unsafe { CStr::from_ptr(field.as_ptr()) }.to_string_lossy().into_owned()
    };
    let name = Some(text(&info.pbi_name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| text(&info.pbi_comm));
    Some(ProcessEntry {
        process_id: pid as u32,
        parent_process_id: info.pbi_ppid,
        name,
    })
}

/// The windows of apps, with their owner, size and display. Titles need the screen recording permission; without
/// it they are empty, which detection does not use.
fn windows() -> Vec<WindowEntry> {
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionAll | CGWindowListOption::ExcludeDesktopElements,
        0,
    ) else {
        return Vec::new();
    };
    // SAFETY: the window list is an array of dictionaries with string keys.
    let list = unsafe { CFRetained::cast_unchecked::<CFArray<CFDictionary<CFString, CFType>>>(list) };
    list.iter().filter_map(|window| window_entry(&window)).collect()
}

fn window_entry(window: &CFDictionary<CFString, CFType>) -> Option<WindowEntry> {
    // SAFETY: the keys are constants Core Graphics exports.
    let (layer_key, number_key, owner_key, bounds_key, name_key) = unsafe {
        (
            kCGWindowLayer,
            kCGWindowNumber,
            kCGWindowOwnerPID,
            kCGWindowBounds,
            kCGWindowName,
        )
    };
    let number = |key: &CFString| window.get(key)?.downcast_ref::<CFNumber>()?.as_i64();
    // Only ordinary app windows; menus, the Dock and overlays sit on other layers.
    if number(layer_key)? != 0 {
        return None;
    }
    let bounds = window.get(bounds_key)?;
    let mut rect = CGRect::default();
    // SAFETY: `rect` is a valid output.
    if !unsafe { CGRectMakeWithDictionaryRepresentation(bounds.downcast_ref::<CFDictionary>(), &mut rect) } {
        return None;
    }
    if rect.size.width < MIN_WINDOW_SIDE || rect.size.height < MIN_WINDOW_SIDE {
        return None;
    }
    let title = window
        .get(name_key)
        .and_then(|name| name.downcast_ref::<CFString>().map(ToString::to_string))
        .unwrap_or_default();
    Some(WindowEntry {
        id: number(number_key)? as u64,
        process_id: number(owner_key)? as u32,
        title,
        display: display_at(CGPoint {
            x: rect.origin.x + rect.size.width / 2.0,
            y: rect.origin.y + rect.size.height / 2.0,
        }),
        area: (rect.size.width * rect.size.height) as u64,
    })
}

/// The display that holds a point, as its display ID.
fn display_at(point: CGPoint) -> Option<String> {
    let mut display: CGDirectDisplayID = 0;
    let mut count = 0u32;
    // SAFETY: room for one display and its count.
    let error = unsafe { CGGetDisplaysWithPoint(point, 1, &mut display, &mut count) };
    (error.0 == 0 && count > 0).then(|| display.to_string())
}
