//! OS-independent logic that detects meeting start/end and coordinates recording.
//!
//! This crate does not touch the file system, clock, network, or OS APIs directly.
//! All interaction with the outside world goes through the ports (traits) below; callers supply the implementations.
//!
//! | Port | Role |
//! |---|---|
//! | [`SignalSource`] | Acquiring signals for detection |
//! | [`Recorder`] | Recording |
//! | [`Clock`], [`OutputFolder`], [`Pause`] | Clock, recordings folder and its in-progress folder, wait between retries |
//!
//! With the `fakes` feature enabled, fakes of these (the `fakes` module) are available.

mod apps;
mod config;
mod coordinator;
mod detector;
mod engine;
#[cfg(feature = "fakes")]
pub mod fakes;
pub mod handover;
mod metadata;
pub mod naming;
mod ports;
mod recording;
mod rules;
mod signals;
mod target;

pub use apps::{MeetingApp, WindowRole};
pub use config::{AudioConfig, CONFIG_VERSION, CaptureScope, Config, ConfigError, VideoConfig};
pub use coordinator::{
    CoordinatorStatus, MeetingStatus, Notice, NoticeLevel, RecordingCoordinator, RecordingStatus, RecordingTrigger,
};
pub use detector::{DetectorEvent, MeetingDetector, MeetingInfo, MeetingKey, Timing};
pub use engine::Engine;
pub use metadata::{MAX_ENTRIES, MAX_KEY_LEN, MAX_VALUE_CHARS, MeetingMetadata, RecordingMetadata};
pub use ports::{Clock, CoordinatorPorts, LocalDateTime, OutputFolder, Pause};
pub use recording::{MissingSound, Recorder, RecordingRequest, Sound, Started, VideoTarget};
pub use rules::{AppWindow, MeetingRule, RuleEvaluator, RuleEvidence, default_rules};
pub use signals::{
    AudioSession, BrowserLink, BrowserMeeting, BrowserTab, ProcessEntry, SignalSnapshot, SignalSource, WindowEntry,
    full_snapshot,
};
pub use target::{RecordingTarget, WindowChoice, window_choices};
