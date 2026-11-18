use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::rules::{MeetingRule, RuleEvaluator, default_rules};

/// The format of the configuration file. Raise it whenever a file of the current format would read differently or
/// not at all: a key removed, renamed or given another meaning. A file of any other version is not read; it is
/// replaced with the defaults. There is no conversion from an older format.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file is for another format of the configuration, or names none, as files did before formats were
    /// numbered. `found` is the version it names, if it names one as a number.
    #[error(
        "the file is for {}, and this app reads configuration version {CONFIG_VERSION}",
        found.map_or("no configuration version".to_owned(), |version| format!("configuration version {version}"))
    )]
    UnsupportedVersion { found: Option<i64> },
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
    /// The format of the file, [`CONFIG_VERSION`].
    pub version: u32,
    /// Where recordings are saved. Defaults to MeetingAssistant under the OS "Videos" folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_directory: Option<PathBuf>,
    /// A desktop app's meeting is considered started once the app has used the microphone for this many seconds.
    pub start_debounce_seconds: f64,
    /// A desktop app's meeting is considered ended once its microphone use has stopped for this many seconds.
    pub end_grace_seconds: f64,
    /// A browser meeting is considered started once the extension has reported its call for this many seconds.
    pub browser_start_debounce_seconds: f64,
    /// A browser meeting is considered ended once the extension has reported no call for this many seconds.
    pub browser_end_grace_seconds: f64,
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
            version: CONFIG_VERSION,
            output_directory: None,
            start_debounce_seconds: 3.0,
            end_grace_seconds: 15.0,
            // A connected call is a firmer signal than microphone use, which apps drop while muted.
            browser_start_debounce_seconds: 1.0,
            browser_end_grace_seconds: 5.0,
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
# `version` is the format of this file; leave it as it is. A file of another version is not read:
# the app keeps it beside this one and starts again from the defaults.
#
# To change where recordings are saved, uncomment the next line and edit it
# (defaults to MeetingAssistant under the Videos folder).
# output_directory = 'D:\\Recordings'

";

impl Config {
    /// Parses a TOML string and validates it. Omitted fields take their default values. A file that does not name
    /// [`CONFIG_VERSION`] as its version is not read further.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let table: toml::Table = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        match table.get("version") {
            Some(toml::Value::Integer(version)) if *version == i64::from(CONFIG_VERSION) => {}
            Some(toml::Value::Integer(version)) => {
                return Err(ConfigError::UnsupportedVersion { found: Some(*version) });
            }
            _ => return Err(ConfigError::UnsupportedVersion { found: None }),
        }
        // Parsed again from the text, so that an error names its line.
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
        if !(self.browser_start_debounce_seconds.is_finite() && self.browser_start_debounce_seconds >= 0.0) {
            return invalid("browser_start_debounce_seconds must be 0 or greater");
        }
        if !(self.browser_end_grace_seconds.is_finite() && self.browser_end_grace_seconds >= 0.0) {
            return invalid("browser_end_grace_seconds must be 0 or greater");
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
