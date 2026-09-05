//! Microphone-use and audio-session collector (contract-audio-session-mic-use).
//!
//! Two sources, one of which may emit: the session manager decides, the consent store only
//! corroborates. The outcome partition is total:
//!
//! | outcome | condition | effect |
//! | --- | --- | --- |
//! | determinate | session-manager transition for a matched process | `MicCaptureStarted` / `MicCaptureStopped` |
//! | unknown | neither source reports the process | nothing |
//! | inconclusive | consent-store window with no session-manager transition | nothing, `inconclusive_consent_only` += 1 |
//! | conflicting | consent-store window open while the session manager says `Inactive`/`Expired` | `MicCaptureStopped` (session manager wins), `conflicts` += 1 |
//! | failure | notification registration fails | `MicUseUnavailable` on `CollectorStarted`, consent store disabled |

use crate::audio_session::{SessionEvent, SessionFlow, SessionManager, SessionState};
use crate::endpoint_observation::{EndpointObservation, EndpointObservations};
use crate::package_identity::{PackageIdentity, PackageIdentityProbe};
use crate::process::{ProcessEnumerator, ProcessRecord, TargetApplications};
use crate::Clock;
use ma_core_types::id::TypedId;
use ma_core_types::SignalId;
use ma_signal::{Authority, Payload, Signal, SignalKind, SignalSource, Subject, SCHEMA_VERSION};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The `source_id` every signal of this collector carries.
pub const SOURCE_ID: &str = "os.audio_session";

/// One consent-store entry: an application key (package family name, or the executable path of a
/// non-packaged application) and whether its microphone-use window is currently open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentUse {
    pub app_key: String,
    pub in_use: bool,
}

/// The corroborating source (`CapabilityAccessManager` consent store), polled at one second.
pub trait ConsentStore {
    fn poll(&mut self) -> Vec<ConsentUse>;
}

/// Scripted consent-store views; the last view repeats.
#[derive(Debug, Default, Clone)]
pub struct FakeConsentStore {
    views: VecDeque<Vec<ConsentUse>>,
    last: Vec<ConsentUse>,
}

impl FakeConsentStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn then_view(mut self, view: Vec<ConsentUse>) -> Self {
        self.views.push_back(view);
        self
    }
}

impl ConsentStore for FakeConsentStore {
    fn poll(&mut self) -> Vec<ConsentUse> {
        if let Some(v) = self.views.pop_front() {
            self.last = v;
        }
        self.last.clone()
    }
}

/// Reads `HKCU\...\CapabilityAccessManager\ConsentStore\microphone`: packaged applications are
/// subkeys named by package family, non-packaged ones live under `NonPackaged` keyed by their
/// executable path with `#` for `\`. A window is open when `LastUsedTimeStart` is later than
/// `LastUsedTimeStop`.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsConsentStore;

#[cfg(windows)]
impl ConsentStore for WindowsConsentStore {
    fn poll(&mut self) -> Vec<ConsentUse> {
        use windows::core::{HSTRING, PWSTR};
        use windows::Win32::Foundation::ERROR_SUCCESS;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_READ,
        };
        const ROOT: &str =
            "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";

        fn qword(key: HKEY, name: &str) -> Option<u64> {
            let mut data = [0u8; 8];
            let mut len = 8u32;
            // SAFETY: fixed 8-byte buffer with its length.
            let rc = unsafe {
                RegQueryValueExW(
                    key,
                    &HSTRING::from(name),
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut len),
                )
            };
            (rc == ERROR_SUCCESS && len == 8).then(|| u64::from_le_bytes(data))
        }
        fn open(parent: HKEY, sub: &str) -> Option<HKEY> {
            let mut key = HKEY::default();
            // SAFETY: read-only open; the caller closes the key.
            let rc =
                unsafe { RegOpenKeyExW(parent, &HSTRING::from(sub), None, KEY_READ, &mut key) };
            (rc == ERROR_SUCCESS).then_some(key)
        }
        fn subkeys(key: HKEY) -> Vec<String> {
            let mut out = Vec::new();
            let mut index = 0u32;
            loop {
                let mut buf = vec![0u16; 512];
                let mut len = buf.len() as u32;
                // SAFETY: buffer and length as the API requires.
                let rc = unsafe {
                    RegEnumKeyExW(
                        key,
                        index,
                        Some(PWSTR(buf.as_mut_ptr())),
                        &mut len,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if rc != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
                index += 1;
            }
            out
        }
        fn use_of(key: HKEY) -> bool {
            let start = qword(key, "LastUsedTimeStart").unwrap_or(0);
            let stop = qword(key, "LastUsedTimeStop").unwrap_or(0);
            start != 0 && start > stop
        }

        let mut out = Vec::new();
        let Some(root) = open(HKEY_CURRENT_USER, ROOT) else {
            return out;
        };
        for name in subkeys(root) {
            let Some(key) = open(root, &name) else {
                continue;
            };
            if name.eq_ignore_ascii_case("NonPackaged") {
                for exe in subkeys(key) {
                    if let Some(k) = open(key, &exe) {
                        out.push(ConsentUse {
                            app_key: exe.replace('#', "\\"),
                            in_use: use_of(k),
                        });
                        // SAFETY: closes a key opened above.
                        unsafe {
                            let _ = RegCloseKey(k);
                        }
                    }
                }
            } else {
                out.push(ConsentUse {
                    app_key: name,
                    in_use: use_of(key),
                });
            }
            // SAFETY: closes a key opened above.
            unsafe {
                let _ = RegCloseKey(key);
            }
        }
        // SAFETY: closes the root key.
        unsafe {
            let _ = RegCloseKey(root);
        }
        out
    }
}

/// The typed startup failure reported on `CollectorStarted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupFailure {
    /// Session-manager notification registration failed; no microphone signal will be emitted for
    /// the collector's lifetime and the consent store is not consulted.
    MicUseUnavailable { code: i32 },
}

/// Counters that make the outcome partition observable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MicCollectorDiagnostics {
    pub startup_failure: Option<StartupFailure>,
    /// Consent-store windows open for a target application with no session-manager transition.
    pub inconclusive_consent_only: u32,
    /// Consent-store window still open while the session manager reported Inactive/Expired.
    pub conflicts: u32,
    /// Determinate microphone transitions emitted.
    pub determinate: u32,
    pub polls: u32,
}

#[derive(Debug, Clone)]
struct TrackedSession {
    pid: u32,
    flow: SessionFlow,
    endpoint_id: String,
    state: SessionState,
    subject: Subject,
    tree_root_pid: u32,
}

/// The collector: session manager (emitting), consent store (corroborating), process enumerator
/// and package probe (subject construction and the process-tree lookup), clock.
pub struct AudioSessionMicCollector<M, S, E, P, C> {
    sessions: M,
    consent: S,
    processes: E,
    probe: P,
    clock: C,
    targets: TargetApplications,
    tracked: BTreeMap<String, TrackedSession>,
    queue: VecDeque<Signal>,
    started: bool,
    subscribed: bool,
    diagnostics: MicCollectorDiagnostics,
    endpoints: EndpointObservations,
    /// Applications for which the session manager reported an Inactive/Expired capture transition
    /// on the latest poll (for conflict counting).
    stopped_now: BTreeSet<String>,
}

impl<M, S, E, P, C> AudioSessionMicCollector<M, S, E, P, C>
where
    M: SessionManager,
    S: ConsentStore,
    E: ProcessEnumerator,
    P: PackageIdentityProbe,
    C: Clock,
{
    pub fn new(
        sessions: M,
        consent: S,
        processes: E,
        probe: P,
        clock: C,
        targets: TargetApplications,
    ) -> Self {
        Self {
            sessions,
            consent,
            processes,
            probe,
            clock,
            targets,
            tracked: BTreeMap::new(),
            queue: VecDeque::new(),
            started: false,
            subscribed: false,
            diagnostics: MicCollectorDiagnostics::default(),
            endpoints: EndpointObservations::default(),
            stopped_now: BTreeSet::new(),
        }
    }

    pub fn diagnostics(&self) -> &MicCollectorDiagnostics {
        &self.diagnostics
    }

    /// The non-Signal endpoint observation API (adr-20260904-mic-endpoint-observed-outside-the-signal-envelope).
    pub fn endpoint_observations(&self) -> &EndpointObservations {
        &self.endpoints
    }

    /// Performs at most one subscription/poll and returns every signal it produced in callback
    /// order. The live harness uses this instead of one `SignalSource` read per second, so a
    /// created+active session cannot defer `MicCaptureStarted` into the next cycle.
    pub fn observe_batch(&mut self) -> Vec<Signal> {
        let mut out = Vec::new();
        if !self.started {
            if let Some(started) = self.next_signal() {
                out.push(started);
            }
        } else if self.queue.is_empty() {
            self.poll();
        }
        out.extend(self.queue.drain(..));
        out
    }

    fn signal(&mut self, kind: SignalKind, subject: Subject, payload: Payload) -> Signal {
        Signal {
            signal_id: SignalId::new(),
            source_id: SOURCE_ID.to_string(),
            kind,
            subject,
            observed_at: self.clock.now(),
            payload,
            authority: Authority::Os,
            schema_version: SCHEMA_VERSION,
        }
    }

    /// Resolves the subject of a pid against the current process view; `None` for a process that
    /// is not a target application.
    fn subject_for(
        &mut self,
        pid: u32,
        view: &BTreeMap<u32, ProcessRecord>,
    ) -> Option<(Subject, u32)> {
        let record = view.get(&pid)?;
        let identity = self.probe.probe(pid);
        let matches_image = self
            .targets
            .image_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&record.image_name));
        let matches_package = matches!(&identity, PackageIdentity::Packaged(name) if self.targets.package_family_names.iter().any(|n| n == name));
        if !(matches_image || matches_package) {
            return None;
        }
        let subject = Subject::Process {
            pid,
            image_name: record.image_name.clone(),
            package_family_name: identity.family_name(),
        };
        Some((subject, tree_root(view, record)))
    }

    fn apply_view(&mut self, events: Vec<SessionEvent>, resync: bool) {
        let view: BTreeMap<u32, ProcessRecord> = self
            .processes
            .snapshot()
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.pid, r))
            .collect();
        self.stopped_now.clear();
        let seen: BTreeSet<String> = events.iter().map(|e| e.session_key.clone()).collect();
        for event in events {
            match self.tracked.get(&event.session_key).cloned() {
                None => {
                    if event.state == SessionState::Expired {
                        continue;
                    }
                    let Some((subject, tree_root_pid)) = self.subject_for(event.pid, &view) else {
                        continue;
                    };
                    let root = Payload {
                        process_tree_root_pid: Some(tree_root_pid),
                        restart_resync: resync,
                        ..Payload::default()
                    };
                    let created = self.signal(
                        SignalKind::AudioSessionCreated,
                        subject.clone(),
                        root.clone(),
                    );
                    self.queue.push_back(created);
                    if event.flow == SessionFlow::Capture && event.state == SessionState::Active {
                        self.mic_started(&subject, tree_root_pid, &event, resync);
                    }
                    self.tracked.insert(
                        event.session_key.clone(),
                        TrackedSession {
                            pid: event.pid,
                            flow: event.flow,
                            endpoint_id: event.endpoint_id.clone(),
                            state: event.state,
                            subject,
                            tree_root_pid,
                        },
                    );
                }
                Some(prev) => {
                    if prev.state == event.state {
                        continue;
                    }
                    self.transition(&event.session_key, &prev, event.state, &event);
                }
            }
        }
        // Sessions that vanished from the view are expired.
        let gone: Vec<(String, TrackedSession)> = self
            .tracked
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, prev) in gone {
            let event = SessionEvent {
                pid: prev.pid,
                session_key: key.clone(),
                endpoint_id: prev.endpoint_id.clone(),
                flow: prev.flow,
                state: SessionState::Expired,
            };
            self.transition(&key, &prev, SessionState::Expired, &event);
        }
    }

    fn mic_started(
        &mut self,
        subject: &Subject,
        tree_root_pid: u32,
        event: &SessionEvent,
        resync: bool,
    ) {
        let payload = Payload {
            restart_resync: resync,
            process_tree_root_pid: Some(tree_root_pid),
            ..Payload::default()
        };
        let started = self.signal(SignalKind::MicCaptureStarted, subject.clone(), payload);
        self.queue.push_back(started);
        self.diagnostics.determinate += 1;
        let since = self.clock.now().monotonic_ns;
        self.endpoints.record(EndpointObservation {
            pid: event.pid,
            endpoint_id: event.endpoint_id.clone(),
            session_key: event.session_key.clone(),
            since_monotonic_ns: since,
        });
    }

    fn transition(
        &mut self,
        key: &str,
        prev: &TrackedSession,
        next: SessionState,
        event: &SessionEvent,
    ) {
        let root = Payload {
            process_tree_root_pid: Some(prev.tree_root_pid),
            ..Payload::default()
        };
        if prev.flow == SessionFlow::Capture {
            if prev.state == SessionState::Active && next != SessionState::Active {
                let stopped = self.signal(
                    SignalKind::MicCaptureStopped,
                    prev.subject.clone(),
                    root.clone(),
                );
                self.queue.push_back(stopped);
                self.diagnostics.determinate += 1;
                self.endpoints.clear(prev.pid, key);
                self.stopped_now.insert(app_key_of(&prev.subject));
            } else if prev.state != SessionState::Active && next == SessionState::Active {
                self.mic_started(&prev.subject.clone(), prev.tree_root_pid, event, false);
            }
        }
        if next == SessionState::Expired {
            let destroyed = self.signal(
                SignalKind::AudioSessionDestroyed,
                prev.subject.clone(),
                root,
            );
            self.queue.push_back(destroyed);
            self.tracked.remove(key);
        } else if let Some(t) = self.tracked.get_mut(key) {
            t.state = next;
            t.endpoint_id = event.endpoint_id.clone();
        }
    }

    /// Corroboration only: the consent store never emits.
    fn corroborate(&mut self) {
        let uses = self.consent.poll();
        let active_apps: BTreeSet<String> = self
            .tracked
            .values()
            .filter(|t| t.flow == SessionFlow::Capture && t.state == SessionState::Active)
            .map(|t| app_key_of(&t.subject))
            .collect();
        for u in uses.into_iter().filter(|u| u.in_use) {
            let Some(app) = self.targets_app_key(&u.app_key) else {
                continue;
            };
            if active_apps.contains(&app) {
                continue; // agrees with the session manager
            }
            if self.stopped_now.contains(&app) {
                self.diagnostics.conflicts += 1;
            } else {
                self.diagnostics.inconclusive_consent_only += 1;
            }
        }
    }

    /// Maps a consent-store key to the target application it names, as `image:<name>` or
    /// `package:<family>`; `None` for applications this collector does not observe.
    fn targets_app_key(&self, consent_key: &str) -> Option<String> {
        let leaf = consent_key
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(consent_key);
        if let Some(img) = self
            .targets
            .image_names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(leaf))
        {
            return Some(format!("image:{}", img.to_ascii_lowercase()));
        }
        self.targets
            .package_family_names
            .iter()
            .find(|n| consent_key.starts_with(n.as_str()))
            .map(|n| format!("package:{n}"))
    }

    /// One observation cycle: primary view, then corroboration.
    pub fn poll(&mut self) {
        self.diagnostics.polls += 1;
        if !self.subscribed {
            return;
        }
        let events = self.sessions.poll();
        self.apply_view(events, false);
        self.corroborate();
    }
}

fn app_key_of(subject: &Subject) -> String {
    match subject {
        Subject::Process {
            image_name,
            package_family_name: Some(pkg),
            ..
        } if !pkg.is_empty() => {
            let _ = image_name;
            format!("package:{pkg}")
        }
        Subject::Process { image_name, .. } => format!("image:{}", image_name.to_ascii_lowercase()),
        _ => String::new(),
    }
}

fn tree_root(view: &BTreeMap<u32, ProcessRecord>, record: &ProcessRecord) -> u32 {
    let mut current = record;
    let mut hops = 0;
    while let Some(parent) = view.get(&current.parent_pid) {
        if parent.pid == current.pid
            || !parent.image_name.eq_ignore_ascii_case(&record.image_name)
            || hops > 64
        {
            break;
        }
        current = parent;
        hops += 1;
    }
    current.pid
}

impl<M, S, E, P, C> SignalSource for AudioSessionMicCollector<M, S, E, P, C>
where
    M: SessionManager,
    S: ConsentStore,
    E: ProcessEnumerator,
    P: PackageIdentityProbe,
    C: Clock,
{
    fn source_id(&self) -> &str {
        SOURCE_ID
    }

    fn next_signal(&mut self) -> Option<Signal> {
        if !self.started {
            self.started = true;
            match self.sessions.subscribe() {
                Ok(initial) => {
                    self.subscribed = true;
                    self.apply_view(initial, true);
                }
                Err(e) => {
                    self.diagnostics.startup_failure =
                        Some(StartupFailure::MicUseUnavailable { code: e.code });
                }
            }
            let started = self.signal(
                SignalKind::CollectorStarted,
                Subject::System,
                Payload::default(),
            );
            return Some(started);
        }
        if self.queue.is_empty() {
            self.poll();
        }
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_session::FakeSessionManager;
    use crate::package_identity::FakePackageIdentityProbe;
    use crate::process::FakeProcessEnumerator;
    use crate::SteppingClock;

    fn record(pid: u32, parent_pid: u32, image: &str) -> ProcessRecord {
        ProcessRecord {
            pid,
            parent_pid,
            image_name: image.to_string(),
        }
    }
    fn targets() -> TargetApplications {
        TargetApplications {
            image_names: vec!["example-meetings.exe".into(), "example-browser.exe".into()],
            package_family_names: vec![],
        }
    }
    fn processes() -> FakeProcessEnumerator {
        FakeProcessEnumerator::new().then_view(vec![
            record(100, 4, "example-meetings.exe"),
            record(200, 4, "example-browser.exe"),
            record(201, 200, "example-browser.exe"),
            record(300, 4, "unrelated.exe"),
        ])
    }
    fn ev(pid: u32, key: &str, flow: SessionFlow, state: SessionState) -> SessionEvent {
        SessionEvent {
            pid,
            session_key: key.into(),
            endpoint_id: "{ep-default}".into(),
            flow,
            state,
        }
    }
    fn clock() -> SteppingClock {
        SteppingClock::new(1_000_000, 1_000_000, 1_756_857_600_000)
    }
    fn kinds(signals: &[Signal]) -> Vec<(SignalKind, u32)> {
        signals
            .iter()
            .map(|s| match &s.subject {
                Subject::Process { pid, .. } => (s.kind, *pid),
                Subject::System => (s.kind, 0),
                other => panic!("unexpected subject {other:?}"),
            })
            .collect()
    }
    fn drain<S: SignalSource>(s: &mut S, max: usize) -> Vec<Signal> {
        let mut out = Vec::new();
        while out.len() < max {
            match s.next_signal() {
                Some(x) => out.push(x),
                None => break,
            }
        }
        out
    }

    #[test]
    fn mic_use_from_fake_session_manager() {
        let sessions = FakeSessionManager::new()
            // start: meetings app has an inactive capture session; browser helper has a render session
            .then_view(vec![
                ev(
                    100,
                    "s-meet-cap",
                    SessionFlow::Capture,
                    SessionState::Inactive,
                ),
                ev(
                    201,
                    "s-browser-render",
                    SessionFlow::Render,
                    SessionState::Active,
                ),
                ev(
                    300,
                    "s-unrelated",
                    SessionFlow::Capture,
                    SessionState::Active,
                ),
            ])
            // meetings app starts capturing; browser helper opens a capture session
            .then_view(vec![
                ev(
                    100,
                    "s-meet-cap",
                    SessionFlow::Capture,
                    SessionState::Active,
                ),
                ev(
                    201,
                    "s-browser-render",
                    SessionFlow::Render,
                    SessionState::Active,
                ),
                ev(
                    201,
                    "s-browser-cap",
                    SessionFlow::Capture,
                    SessionState::Active,
                ),
            ])
            // meetings app stops and its session expires; browser capture continues
            .then_view(vec![
                ev(
                    100,
                    "s-meet-cap",
                    SessionFlow::Capture,
                    SessionState::Expired,
                ),
                ev(
                    201,
                    "s-browser-render",
                    SessionFlow::Render,
                    SessionState::Active,
                ),
                ev(
                    201,
                    "s-browser-cap",
                    SessionFlow::Capture,
                    SessionState::Active,
                ),
            ]);
        let mut c = AudioSessionMicCollector::new(
            sessions,
            FakeConsentStore::new(),
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let first = c.next_signal().unwrap();
        assert_eq!(first.kind, SignalKind::CollectorStarted);
        assert_eq!(c.diagnostics().startup_failure, None);
        // initial view: two sessions created (unrelated.exe ignored), no mic use yet
        let initial = drain(&mut c, 2);
        assert_eq!(
            kinds(&initial),
            vec![
                (SignalKind::AudioSessionCreated, 100),
                (SignalKind::AudioSessionCreated, 201)
            ]
        );
        // second view
        let second = drain(&mut c, 3);
        assert_eq!(
            kinds(&second),
            vec![
                (SignalKind::MicCaptureStarted, 100),
                (SignalKind::AudioSessionCreated, 201),
                (SignalKind::MicCaptureStarted, 201),
            ]
        );
        assert_eq!(
            second[0].subject,
            Subject::Process {
                pid: 100,
                image_name: "example-meetings.exe".into(),
                package_family_name: None
            }
        );
        assert_eq!(
            second[2].payload.process_tree_root_pid,
            Some(200),
            "browser helper joins on its tree root"
        );
        assert!(!second[0].payload.restart_resync);
        assert_eq!(
            c.endpoint_observations().endpoint_for(100),
            Some("{ep-default}")
        );
        // third view: stop + destroy for the meetings app
        let third = drain(&mut c, 2);
        assert_eq!(
            kinds(&third),
            vec![
                (SignalKind::MicCaptureStopped, 100),
                (SignalKind::AudioSessionDestroyed, 100)
            ]
        );
        assert_eq!(c.endpoint_observations().endpoint_for(100), None);
        assert_eq!(
            c.endpoint_observations().endpoint_for(201),
            Some("{ep-default}")
        );
        assert!(c.next_signal().is_none(), "steady state emits nothing");
        assert_eq!(c.diagnostics().determinate, 3);
        assert!(drain(&mut c, 8).iter().all(|s| s.source_id == SOURCE_ID));
    }

    #[test]
    fn one_observation_batch_emits_created_and_started_without_a_second_poll() {
        let sessions = FakeSessionManager::new().then_view(vec![ev(
            100,
            "s-meet-cap",
            SessionFlow::Capture,
            SessionState::Active,
        )]);
        let mut collector = AudioSessionMicCollector::new(
            sessions,
            FakeConsentStore::new(),
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let batch = collector.observe_batch();
        assert_eq!(
            kinds(&batch),
            vec![
                (SignalKind::CollectorStarted, 0),
                (SignalKind::AudioSessionCreated, 100),
                (SignalKind::MicCaptureStarted, 100),
            ]
        );
        assert_eq!(
            collector.diagnostics().polls,
            0,
            "no second OS poll was needed"
        );
    }

    #[test]
    fn active_then_inactive_notifications_in_one_poll_emit_both_transitions() {
        let sessions = FakeSessionManager::new()
            .then_view(vec![ev(
                100,
                "s",
                SessionFlow::Capture,
                SessionState::Inactive,
            )])
            .then_view(vec![
                ev(100, "s", SessionFlow::Capture, SessionState::Active),
                ev(100, "s", SessionFlow::Capture, SessionState::Inactive),
            ]);
        let mut collector = AudioSessionMicCollector::new(
            sessions,
            FakeConsentStore::new(),
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let _ = collector.observe_batch();
        let batch = collector.observe_batch();
        assert_eq!(
            kinds(&batch),
            vec![
                (SignalKind::MicCaptureStarted, 100),
                (SignalKind::MicCaptureStopped, 100),
            ]
        );
    }

    #[test]
    fn consent_store_never_emits_a_signal_alone() {
        // The session manager reports nothing for the meetings app; the consent store says it is
        // using the microphone: inconclusive, no signal.
        let sessions = FakeSessionManager::new()
            .then_view(vec![])
            .then_view(vec![]);
        let consent = FakeConsentStore::new().then_view(vec![ConsentUse {
            app_key: "C:\\Program Files\\Example\\example-meetings.exe".into(),
            in_use: true,
        }]);
        let mut c = AudioSessionMicCollector::new(
            sessions,
            consent,
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        assert_eq!(c.next_signal().unwrap().kind, SignalKind::CollectorStarted);
        assert!(c.next_signal().is_none());
        assert!(c.next_signal().is_none());
        assert_eq!(c.diagnostics().inconclusive_consent_only, 2);
        assert_eq!(c.diagnostics().conflicts, 0);
        assert_eq!(c.diagnostics().determinate, 0);

        // Conflict: the consent window is still open while the session manager reports Expired.
        let sessions = FakeSessionManager::new()
            .then_view(vec![ev(
                100,
                "s1",
                SessionFlow::Capture,
                SessionState::Active,
            )])
            .then_view(vec![ev(
                100,
                "s1",
                SessionFlow::Capture,
                SessionState::Expired,
            )]);
        let consent = FakeConsentStore::new().then_view(vec![ConsentUse {
            app_key: "C:\\Program Files\\Example\\example-meetings.exe".into(),
            in_use: true,
        }]);
        let mut c = AudioSessionMicCollector::new(
            sessions,
            consent,
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let _ = c.next_signal();
        let initial = drain(&mut c, 2);
        assert_eq!(
            kinds(&initial),
            vec![
                (SignalKind::AudioSessionCreated, 100),
                (SignalKind::MicCaptureStarted, 100)
            ]
        );
        assert!(
            initial[1].payload.restart_resync,
            "already active at start is a resync"
        );
        let after = drain(&mut c, 2);
        assert_eq!(
            kinds(&after),
            vec![
                (SignalKind::MicCaptureStopped, 100),
                (SignalKind::AudioSessionDestroyed, 100)
            ],
            "the session manager wins the conflict"
        );
        assert_eq!(c.diagnostics().conflicts, 1);
        assert_eq!(c.diagnostics().inconclusive_consent_only, 0);
    }

    #[test]
    fn registration_failure_is_the_typed_startup_failure() {
        let consent = FakeConsentStore::new().then_view(vec![ConsentUse {
            app_key: "example-meetings.exe".into(),
            in_use: true,
        }]);
        let mut c = AudioSessionMicCollector::new(
            FakeSessionManager::failing(-2004287483),
            consent,
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let first = c.next_signal().unwrap();
        assert_eq!(first.kind, SignalKind::CollectorStarted);
        assert_eq!(
            c.diagnostics().startup_failure,
            Some(StartupFailure::MicUseUnavailable { code: -2004287483 })
        );
        for _ in 0..3 {
            assert!(
                c.next_signal().is_none(),
                "never degrades to consent-store-only signals"
            );
        }
        assert_eq!(
            c.diagnostics().inconclusive_consent_only,
            0,
            "the consent store is not consulted"
        );
    }

    #[test]
    fn non_default_endpoint_is_observed_but_never_a_device_signal() {
        let mut e = ev(100, "s1", SessionFlow::Capture, SessionState::Active);
        e.endpoint_id = "{0.0.1.00000000}.{headset-1234}".into();
        let sessions = FakeSessionManager::new().then_view(vec![e]);
        let mut c = AudioSessionMicCollector::new(
            sessions,
            FakeConsentStore::new(),
            processes(),
            FakePackageIdentityProbe::default(),
            clock(),
            targets(),
        );
        let all = drain(&mut c, 8);
        assert!(all
            .iter()
            .all(|s| !matches!(s.subject, Subject::Device { .. })));
        assert_eq!(
            c.endpoint_observations().endpoint_for(100),
            Some("{0.0.1.00000000}.{headset-1234}")
        );
        let obs: Vec<&EndpointObservation> = c.endpoint_observations().iter().collect();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].session_key, "s1");
    }
}
