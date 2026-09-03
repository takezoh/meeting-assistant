//! Retry classification, backoff with jitter, and the backlog-capped persistent export queue.

use crate::identity::ExportKey;
use crate::DestError;
use serde::{Deserialize, Serialize};

pub const MAX_ATTEMPTS: u32 = 5;
pub const BACKLOG_CAP: usize = 500;
const SCHEDULE_MS: [u64; 5] = [1_000, 4_000, 16_000, 64_000, 256_000];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryClass {
    Retryable,
    /// 401/403: surfaced, never retried blindly.
    NeedsReauthentication,
    Permanent,
}

pub fn classify(error: &DestError) -> RetryClass {
    match error {
        DestError::Network => RetryClass::Retryable,
        DestError::Http { status } => match status {
            401 | 403 => RetryClass::NeedsReauthentication,
            429 => RetryClass::Retryable,
            500..=599 => RetryClass::Retryable,
            _ => RetryClass::Permanent,
        },
        DestError::Crashed => RetryClass::Retryable,
    }
}

/// Exponential backoff plus jitter in `[0, 25%)` from an injected jitter fraction.
pub fn backoff_with_jitter_ms(attempts: u32, jitter: f64) -> Option<u64> {
    if attempts == 0 || attempts >= MAX_ATTEMPTS {
        return None;
    }
    let base = SCHEDULE_MS[attempts as usize - 1];
    Some(base + (base as f64 * 0.25 * jitter.clamp(0.0, 0.999)) as u64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExportStatus {
    Queued,
    Retrying { attempts: u32, not_before_ms: u64 },
    NeedsReauthentication { attempts: u32 },
    Succeeded,
    FailedPermanent { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportEntry {
    pub key: ExportKey,
    pub destination_id: String,
    pub enqueued_at_ms: u64,
    pub attempts: u32,
    pub status: ExportStatus,
}

/// The persistent export queue's rules; persistence itself is the engine's port.
#[derive(Debug, Default, Clone)]
pub struct ExportQueue {
    pub entries: Vec<ExportEntry>,
    /// Exports the cap forced out, surfaced to the user.
    pub dropped: Vec<ExportKey>,
}

impl ExportQueue {
    fn pending(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    ExportStatus::Queued | ExportStatus::Retrying { .. }
                )
            })
            .count()
    }

    /// At the cap the oldest queued-but-never-attempted entry becomes failed_permanent{backlog_full}
    /// and is surfaced; the queue never grows without bound and never silently refuses work.
    pub fn enqueue(&mut self, key: ExportKey, destination_id: &str, now_ms: u64) {
        if self
            .entries
            .iter()
            .any(|e| e.key == key && !matches!(e.status, ExportStatus::FailedPermanent { .. }))
        {
            return;
        }
        if self.pending() >= BACKLOG_CAP {
            let victim = self
                .entries
                .iter_mut()
                .filter(|e| e.status == ExportStatus::Queued && e.attempts == 0)
                .min_by_key(|e| e.enqueued_at_ms);
            match victim {
                Some(victim) => {
                    victim.status = ExportStatus::FailedPermanent {
                        reason: "backlog_full".into(),
                    };
                    self.dropped.push(victim.key.clone());
                }
                None => {
                    // everything pending has been attempted: the new export is the one that cannot be
                    // taken, and it is surfaced rather than silently exceeding the cap
                    self.entries.push(ExportEntry {
                        key: key.clone(),
                        destination_id: destination_id.to_string(),
                        enqueued_at_ms: now_ms,
                        attempts: 0,
                        status: ExportStatus::FailedPermanent {
                            reason: "backlog_full".into(),
                        },
                    });
                    self.dropped.push(key);
                    return;
                }
            }
        }
        self.entries.push(ExportEntry {
            key,
            destination_id: destination_id.to_string(),
            enqueued_at_ms: now_ms,
            attempts: 0,
            status: ExportStatus::Queued,
        });
    }

    pub fn record_attempt(
        &mut self,
        key: &ExportKey,
        result: Result<(), RetryClass>,
        now_ms: u64,
        jitter: f64,
    ) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.key == *key) else {
            return;
        };
        entry.attempts += 1;
        entry.status = match result {
            Ok(()) => ExportStatus::Succeeded,
            Err(RetryClass::NeedsReauthentication) => ExportStatus::NeedsReauthentication {
                attempts: entry.attempts,
            },
            Err(RetryClass::Permanent) => ExportStatus::FailedPermanent {
                reason: "permanent".into(),
            },
            Err(RetryClass::Retryable) => match backoff_with_jitter_ms(entry.attempts, jitter) {
                Some(delay) => ExportStatus::Retrying {
                    attempts: entry.attempts,
                    not_before_ms: now_ms + delay,
                },
                None => ExportStatus::FailedPermanent {
                    reason: "attempts_exhausted".into(),
                },
            },
        };
    }

    pub fn due(&self, now_ms: u64) -> Vec<&ExportEntry> {
        self.entries
            .iter()
            .filter(|e| match e.status {
                ExportStatus::Queued => true,
                ExportStatus::Retrying { not_before_ms, .. } => not_before_ms <= now_ms,
                _ => false,
            })
            .collect()
    }
}
