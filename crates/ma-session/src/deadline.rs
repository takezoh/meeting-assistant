//! Deadline semantics on a suspend-excluding monotonic clock. Every bound is a fixed number.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Milliseconds on a clock that does not advance while the system is suspended
/// (`QueryUnbiasedInterruptTime`-shaped). Produced by the engine; never read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Unbiased(pub u64);

impl Unbiased {
    pub fn plus_ms(self, ms: u64) -> Unbiased {
        Unbiased(self.0 + ms)
    }
    pub fn ms_since(self, earlier: Unbiased) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// Automatic recording countdown (contract-recording-mode-policy).
pub const COUNTDOWN_MS: u64 = 10_000;
/// Quiet period after a cancel before the same meeting identity may re-arm.
pub const CANCEL_QUIET_MS: u64 = 60_000;
/// End hysteresis: a continuing signal within this window returns to recording.
pub const END_HYSTERESIS_MS: u64 = 60_000;
/// "Still in the meeting?" prompt window.
pub const PROMPT_MS: u64 = 30_000;
/// The single extension granted per ending episode.
pub const EXTENSION_MS: u64 = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineKind {
    Countdown,
    Hysteresis,
    Prompt,
    Extension,
}

impl DeadlineKind {
    pub fn duration_ms(self) -> u64 {
        match self {
            DeadlineKind::Countdown => COUNTDOWN_MS,
            DeadlineKind::Hysteresis => END_HYSTERESIS_MS,
            DeadlineKind::Prompt => PROMPT_MS,
            DeadlineKind::Extension => EXTENSION_MS,
        }
    }
}

/// Pending deadlines. While suspended, the remaining time of each deadline is frozen so that a
/// resume recomputes them instead of letting them fire on wake.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deadlines {
    pending: BTreeMap<DeadlineKind, Unbiased>,
    frozen_remaining: BTreeMap<DeadlineKind, u64>,
}

impl Deadlines {
    pub fn set(&mut self, kind: DeadlineKind, now: Unbiased) -> Unbiased {
        let at = now.plus_ms(kind.duration_ms());
        self.pending.insert(kind, at);
        at
    }
    pub fn clear(&mut self, kind: DeadlineKind) {
        self.pending.remove(&kind);
        self.frozen_remaining.remove(&kind);
    }
    pub fn clear_all(&mut self) {
        self.pending.clear();
        self.frozen_remaining.clear();
    }
    pub fn at(&self, kind: DeadlineKind) -> Option<Unbiased> {
        self.pending.get(&kind).copied()
    }
    pub fn is_pending(&self, kind: DeadlineKind) -> bool {
        self.pending.contains_key(&kind)
    }
    /// Freeze remaining durations at suspend time.
    pub fn suspend(&mut self, now: Unbiased) {
        for (kind, at) in std::mem::take(&mut self.pending) {
            self.frozen_remaining.insert(kind, at.ms_since(now));
        }
    }
    /// Recompute every frozen deadline against the resume instant.
    pub fn resume(&mut self, now: Unbiased) -> Vec<(DeadlineKind, Unbiased)> {
        let mut out = Vec::new();
        for (kind, remaining) in std::mem::take(&mut self.frozen_remaining) {
            let at = now.plus_ms(remaining);
            self.pending.insert(kind, at);
            out.push((kind, at));
        }
        out
    }
    pub fn is_suspended(&self) -> bool {
        !self.frozen_remaining.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_fixed_numbers() {
        assert_eq!(COUNTDOWN_MS, 10_000);
        assert_eq!(CANCEL_QUIET_MS, 60_000);
        assert_eq!(END_HYSTERESIS_MS, 60_000);
        assert_eq!(PROMPT_MS, 30_000);
        assert_eq!(EXTENSION_MS, 300_000);
    }

    #[test]
    fn suspend_freezes_and_resume_recomputes() {
        let mut d = Deadlines::default();
        d.set(DeadlineKind::Countdown, Unbiased(1_000));
        d.suspend(Unbiased(4_000));
        assert!(d.at(DeadlineKind::Countdown).is_none());
        let recomputed = d.resume(Unbiased(1_000_000));
        assert_eq!(
            recomputed,
            vec![(DeadlineKind::Countdown, Unbiased(1_007_000))]
        );
    }
}
