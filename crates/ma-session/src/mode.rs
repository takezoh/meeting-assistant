//! Automatic recording mode policy: resolution order and the meeting identity used for cancel
//! suppression (contract-recording-mode-policy).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Auto,
    Ask,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppClass {
    Desktop,
    Browser,
}

impl AppClass {
    /// Application-class default: desktop applications `auto`, browser meetings `ask`.
    pub fn default_mode(self) -> Mode {
        match self {
            AppClass::Desktop => Mode::Auto,
            AppClass::Browser => Mode::Ask,
        }
    }
}

/// The identity that cancel suppression and hysteresis reason about: which adapter matched and
/// which subject (process tree, tab) it matched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MeetingIdentity {
    pub adapter_id: String,
    pub subject_key: String,
}

/// Mode settings as read from the store at evaluation time. `readable = false` models an
/// unreadable mode store, which resolves to `manual` and surfaces the degradation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeSettings {
    pub global: Mode,
    pub class_defaults: BTreeMap<AppClass, Mode>,
    pub overrides: BTreeMap<String, Mode>,
    pub readable: bool,
}

impl Default for ModeSettings {
    fn default() -> Self {
        let mut class_defaults = BTreeMap::new();
        class_defaults.insert(AppClass::Desktop, AppClass::Desktop.default_mode());
        class_defaults.insert(AppClass::Browser, AppClass::Browser.default_mode());
        Self {
            global: Mode::Auto,
            class_defaults,
            overrides: BTreeMap::new(),
            readable: true,
        }
    }
}

impl PartialOrd for AppClass {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AppClass {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedMode {
    pub mode: Mode,
    pub source: ResolutionSource,
    pub degraded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    ApplicationOverride,
    ClassDefault,
    Global,
    UnreadableStore,
}

impl ModeSettings {
    /// Resolution order: per-application override, then application-class default, then the
    /// global setting. An unreadable store resolves to `manual` and is marked degraded.
    pub fn resolve(&self, adapter_id: &str, class: AppClass) -> ResolvedMode {
        if !self.readable {
            return ResolvedMode {
                mode: Mode::Manual,
                source: ResolutionSource::UnreadableStore,
                degraded: true,
            };
        }
        if let Some(mode) = self.overrides.get(adapter_id) {
            return ResolvedMode {
                mode: *mode,
                source: ResolutionSource::ApplicationOverride,
                degraded: false,
            };
        }
        if let Some(mode) = self.class_defaults.get(&class) {
            return ResolvedMode {
                mode: *mode,
                source: ResolutionSource::ClassDefault,
                degraded: false,
            };
        }
        ResolvedMode {
            mode: self.global,
            source: ResolutionSource::Global,
            degraded: false,
        }
    }
}
