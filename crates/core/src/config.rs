use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::rules::{MeetingRule, RuleEvaluator, default_rules};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot parse the configuration: {0}")]
    Parse(String),
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("rule '{rule}' is invalid: {reason}")]
    InvalidRule { rule: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VideoConfig {
    pub framerate: u32,
    /// Quality (1-100). Higher means better image quality and larger files.
    pub quality: u32,
    /// Average bitrate (kbps) for encoders that do not support a quality setting.
    pub fallback_bitrate_kbps: u32,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            framerate: 30,
            quality: 70,
            fallback_bitrate_kbps: 4000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    /// Record the sound going out of the speakers (the other party's voice).
    pub record_system_audio: bool,
    /// Record the microphone (the user's own voice).
    pub record_microphone: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            record_system_audio: true,
            record_microphone: true,
        }
    }
}

/// What to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureScope {
    /// Only the meeting app: its window for video, and the sound the app plays for the other party's audio.
    #[default]
    App,
    /// The whole display that holds the meeting window, and all sound going out of the output device.
    Screen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where recordings are saved. Defaults to MeetingAssistant under the OS "Videos" folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_directory: Option<PathBuf>,
    /// A meeting is considered started once the start condition holds for this many seconds.
    pub start_debounce_seconds: f64,
    /// A meeting is considered ended once microphone use stops for this many seconds.
    pub end_grace_seconds: f64,
    pub poll_interval_milliseconds: u64,
    /// Recording scope: "app" (the meeting app only) or "screen" (the whole display and all audio output).
    pub capture_scope: CaptureScope,
    pub video: VideoConfig,
    pub audio: AudioConfig,
    pub rules: Vec<MeetingRule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_directory: None,
            start_debounce_seconds: 3.0,
            end_grace_seconds: 15.0,
            poll_interval_milliseconds: 1000,
            capture_scope: CaptureScope::default(),
            video: VideoConfig::default(),
            audio: AudioConfig::default(),
            rules: default_rules(),
        }
    }
}

const HEADER: &str = "\
# Meeting Assistant settings. Changes take effect the next time the app starts.
#
# To change where recordings are saved, uncomment the next line and edit it
# (defaults to MeetingAssistant under the Videos folder).
# output_directory = 'D:\\Recordings'

";

impl Config {
    /// Parses a TOML string and validates it. Omitted fields take their default values.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Renders the default settings as TOML with explanatory comments. Used when first creating the settings file.
    pub fn default_toml() -> String {
        let body = toml::to_string_pretty(&Self::default()).expect("default settings always serialize to TOML");
        format!("{HEADER}{body}")
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let invalid = |message: &str| Err(ConfigError::Invalid(message.into()));

        if self.output_directory.as_ref().is_some_and(|d| d.as_os_str().is_empty()) {
            return invalid("output_directory is empty");
        }
        if !(self.start_debounce_seconds.is_finite() && self.start_debounce_seconds >= 0.0) {
            return invalid("start_debounce_seconds must be 0 or greater");
        }
        if !(self.end_grace_seconds.is_finite() && self.end_grace_seconds >= 0.0) {
            return invalid("end_grace_seconds must be 0 or greater");
        }
        if self.poll_interval_milliseconds < 100 {
            return invalid("poll_interval_milliseconds must be 100 or greater");
        }
        if !(1..=60).contains(&self.video.framerate) {
            return invalid("video.framerate must be between 1 and 60");
        }
        if !(1..=100).contains(&self.video.quality) {
            return invalid("video.quality must be between 1 and 100");
        }
        if !(100..=100_000).contains(&self.video.fallback_bitrate_kbps) {
            return invalid("video.fallback_bitrate_kbps must be between 100 and 100000");
        }
        if self.rules.is_empty() {
            return invalid("rules is empty");
        }
        RuleEvaluator::new(&self.rules).map(|_| ())
    }
}
