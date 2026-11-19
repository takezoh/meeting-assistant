//! Finding what to capture with ScreenCaptureKit: a window by its Core Graphics window ID, a display by its display
//! ID, an app by its process ID. Reading them needs the screen recording permission.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{AnyThread, Message};
use objc2_core_graphics::CGMainDisplayID;
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent, SCWindow};

const CONTENT_TIMEOUT: Duration = Duration::from_secs(10);

/// What can be captured right now.
pub struct Content(Retained<SCShareableContent>);

/// The content is immutable once delivered, so it may move to the recording thread.
struct Delivered(Result<Retained<SCShareableContent>, String>);
// SAFETY: `SCShareableContent` is an immutable snapshot.
unsafe impl Send for Delivered {}

impl Content {
    pub fn current() -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let handler = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: ScreenCaptureKit passes either valid objects or null.
            let result = unsafe { Retained::retain(content) }.ok_or_else(|| {
                // SAFETY: as above.
                unsafe { Retained::retain(error) }.map_or_else(|| "unknown error".to_owned(), |error| error.to_string())
            });
            let _ = sender.send(Delivered(result));
        });
        // SAFETY: the handler lives until ScreenCaptureKit has called it.
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                true, false, &handler,
            );
        }
        let Delivered(result) = receiver
            .recv_timeout(CONTENT_TIMEOUT)
            .context("the list of windows and displays did not arrive")?;
        result.map(Self).map_err(|error| {
            anyhow::anyhow!(
                "cannot list windows and displays ({error}). Allow Meeting Assistant in System Settings > Privacy & \
                 Security > Screen & System Audio Recording, then restart it"
            )
        })
    }

    pub fn window(&self, window_id: u64) -> anyhow::Result<Retained<SCWindow>> {
        // SAFETY: reading immutable properties.
        unsafe { self.0.windows() }
            .iter()
            .find(|window| u64::from(unsafe { window.windowID() }) == window_id)
            .with_context(|| format!("window {window_id:#x} is not available for capture"))
    }

    /// The display with this display ID, or the main display.
    pub fn display(&self, display_id: Option<&str>) -> anyhow::Result<Retained<SCDisplay>> {
        let wanted = display_id
            .and_then(|id| id.parse::<u32>().ok())
            .unwrap_or_else(|| CGMainDisplayID());
        // SAFETY: reading immutable properties.
        let displays = unsafe { self.0.displays() };
        displays
            .iter()
            .find(|display| unsafe { display.displayID() } == wanted)
            .or_else(|| displays.firstObject())
            .context("no display is available for capture")
    }

    /// The running app whose process ID is `process_id`.
    pub fn application(&self, process_id: u32) -> anyhow::Result<Retained<SCRunningApplication>> {
        // SAFETY: reading immutable properties.
        unsafe { self.0.applications() }
            .iter()
            .find(|app| unsafe { app.processID() } as u32 == process_id)
            .with_context(|| format!("the app with process {process_id} is not available for capture"))
    }
}

pub fn window_filter(window: &SCWindow) -> Retained<SCContentFilter> {
    // SAFETY: a valid window from the current content.
    unsafe { SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window) }
}

pub fn display_filter(display: &SCDisplay) -> Retained<SCContentFilter> {
    // SAFETY: a valid display and an empty list.
    unsafe { SCContentFilter::initWithDisplay_excludingWindows(SCContentFilter::alloc(), display, &NSArray::new()) }
}

/// Everything one app shows and plays on `display`: used to capture that app's sound.
pub fn application_filter(display: &SCDisplay, application: &SCRunningApplication) -> Retained<SCContentFilter> {
    // SAFETY: valid objects from the current content.
    unsafe {
        SCContentFilter::initWithDisplay_includingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            display,
            &NSArray::from_retained_slice(&[application.retain()]),
            &NSArray::new(),
        )
    }
}
