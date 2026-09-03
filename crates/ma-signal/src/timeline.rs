//! A recorded, replayable timeline: header plus signals ordered by monotonic time, with
//! duplicate signal ids collapsed (idempotent ingestion).

use crate::envelope::Signal;
use crate::source::{SignalSource, TimelineHeader};
use ma_core_types::SignalId;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalTimeline {
    pub header: TimelineHeader,
    signals: Vec<Signal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineError {
    MissingHeader,
    Line { line: usize, message: String },
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimelineError::MissingHeader => write!(f, "timeline has no header record"),
            TimelineError::Line { line, message } => write!(f, "line {line}: {message}"),
        }
    }
}

impl SignalTimeline {
    pub fn new(header: TimelineHeader) -> Self {
        Self {
            header,
            signals: Vec::new(),
        }
    }

    /// Merge sources in monotonic order. `wall_utc_ms` never participates in ordering.
    pub fn merge(header: TimelineHeader, sources: &mut [&mut dyn SignalSource]) -> Self {
        let mut timeline = Self::new(header);
        for source in sources.iter_mut() {
            while let Some(signal) = source.next_signal() {
                timeline.push(signal);
            }
        }
        timeline
    }

    /// Insert one signal keeping monotonic order; a duplicate `signal_id` is ignored.
    pub fn push(&mut self, signal: Signal) -> bool {
        if self.signals.iter().any(|s| s.signal_id == signal.signal_id) {
            return false;
        }
        let key = (signal.observed_at.monotonic_ns, signal.signal_id);
        let at = self
            .signals
            .partition_point(|s| (s.observed_at.monotonic_ns, s.signal_id) <= key);
        self.signals.insert(at, signal);
        true
    }

    pub fn signals(&self) -> &[Signal] {
        &self.signals
    }

    pub fn ids(&self) -> BTreeSet<SignalId> {
        self.signals.iter().map(|s| s.signal_id).collect()
    }

    /// Parse the JSONL fixture format: header record first, then one signal per line.
    pub fn from_jsonl(text: &str) -> Result<Self, TimelineError> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let (_, header_line) = lines.next().ok_or(TimelineError::MissingHeader)?;
        let header: TimelineHeader =
            serde_json::from_str(header_line).map_err(|e| TimelineError::Line {
                line: 1,
                message: e.to_string(),
            })?;
        let mut timeline = Self::new(header);
        for (index, line) in lines {
            let signal: Signal = serde_json::from_str(line).map_err(|e| TimelineError::Line {
                line: index + 1,
                message: e.to_string(),
            })?;
            timeline.push(signal);
        }
        Ok(timeline)
    }

    /// Serialize to the JSONL fixture format.
    pub fn to_jsonl(&self) -> String {
        let mut out = serde_json::to_string(&self.header).expect("header serializes");
        out.push('\n');
        for signal in &self.signals {
            out.push_str(&serde_json::to_string(signal).expect("signal serializes"));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Authority, ObservedAt, Payload, SignalKind, Subject, SCHEMA_VERSION};
    use ma_core_types::id::TypedId;

    fn signal(t: u64, wall: i64) -> Signal {
        Signal {
            signal_id: SignalId::new(),
            source_id: "s".into(),
            kind: SignalKind::AudioActivity,
            subject: Subject::System,
            observed_at: ObservedAt {
                monotonic_ns: t,
                wall_utc_ms: wall,
            },
            payload: Payload::default(),
            authority: Authority::Os,
            schema_version: SCHEMA_VERSION,
        }
    }

    #[test]
    fn ordering_uses_monotonic_time_only_and_duplicates_are_idempotent() {
        let header = TimelineHeader {
            schema_version: SCHEMA_VERSION,
            adapter_table_version: 1,
            machine_profile: "redacted".into(),
            created: "2026-09-03".into(),
        };
        let mut timeline = SignalTimeline::new(header);
        let later = signal(2_000, 10); // wall clock jumped backwards
        let earlier = signal(1_000, 5_000);
        assert!(timeline.push(later.clone()));
        assert!(timeline.push(earlier.clone()));
        assert!(
            !timeline.push(earlier.clone()),
            "duplicate signal_id is ignored"
        );
        assert_eq!(timeline.signals(), &[earlier, later]);
        let text = timeline.to_jsonl();
        assert_eq!(SignalTimeline::from_jsonl(&text).unwrap(), timeline);
    }
}
