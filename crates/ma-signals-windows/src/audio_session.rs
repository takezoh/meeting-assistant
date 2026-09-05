//! The audio-session observation seam (contract-audio-session-mic-use).
//!
//! `IAudioSessionManager2` session enumeration plus `IAudioSessionNotification` and
//! `IAudioSessionEvents` registration is the primary and only emitting source for microphone use.
//! The seam reports sessions as plain events; the live implementation is compiled only on Windows
//! and [`FakeSessionManager`] scripts the same events for the portable tests.

#[cfg(any(windows, test))]
use std::collections::BTreeSet;
use std::collections::VecDeque;

#[cfg(any(windows, test))]
fn endpoint_refresh_plan(
    known: &BTreeSet<String>,
    active: &BTreeSet<String>,
    unhealthy: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let remove: BTreeSet<String> = known
        .difference(active)
        .chain(known.intersection(unhealthy))
        .cloned()
        .collect();
    let add: BTreeSet<String> = active
        .difference(known)
        .chain(active.intersection(unhealthy))
        .cloned()
        .collect();
    (remove.into_iter().collect(), add.into_iter().collect())
}

#[cfg(any(windows, test))]
struct SessionRegistration<C, S> {
    endpoint_id: String,
    session_key: String,
    control: C,
    sink: S,
}

#[cfg(any(windows, test))]
impl<C, S> SessionRegistration<C, S> {
    fn new(endpoint_id: &str, session_key: &str, control: C, sink: S) -> Self {
        Self {
            endpoint_id: endpoint_id.to_string(),
            session_key: session_key.to_string(),
            control,
            sink,
        }
    }
}

#[cfg(any(windows, test))]
fn take_endpoint_registrations<C, S>(
    registered: &mut Vec<SessionRegistration<C, S>>,
    known_sessions: &mut BTreeSet<String>,
    endpoint_id: &str,
) -> Vec<SessionRegistration<C, S>> {
    let mut removed = Vec::new();
    let mut index = 0;
    while index < registered.len() {
        if registered[index].endpoint_id == endpoint_id {
            removed.push(registered.swap_remove(index));
        } else {
            index += 1;
        }
    }
    known_sessions.retain(|key| registered.iter().any(|entry| entry.session_key == *key));
    removed
}

#[cfg(any(windows, test))]
fn merge_notifications(
    queued: &mut Vec<SessionEvent>,
    view: Vec<SessionEvent>,
) -> Vec<SessionEvent> {
    let mut merged = std::mem::take(queued);
    merged.extend(view);
    merged
}

/// Which direction a session moves audio in. Only capture sessions are microphone use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionFlow {
    Capture,
    Render,
}

/// `AudioSessionState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Inactive,
    Active,
    Expired,
}

/// The current state of one audio session, as enumerated or as notified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEvent {
    /// The owning process.
    pub pid: u32,
    /// The session instance identifier; stable for the life of the session.
    pub session_key: String,
    /// The endpoint the session is bound to (MMDevice identifier).
    pub endpoint_id: String,
    pub flow: SessionFlow,
    pub state: SessionState,
}

/// Why notification registration failed. This is the typed startup failure `MicUseUnavailable`
/// carries; the consent store never stands in for the primary source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionManagerError {
    pub code: i32,
}

/// The primary source. `subscribe` registers for notifications and returns the sessions that
/// already exist (their states feed `restart_resync`); `poll` returns the current view of every
/// session, from which the collector derives transitions.
pub trait SessionManager {
    fn subscribe(&mut self) -> Result<Vec<SessionEvent>, SessionManagerError>;
    fn poll(&mut self) -> Vec<SessionEvent>;
}

/// Scripted session views. `subscribe` returns the first view (or the scripted failure); each
/// `poll` returns the next view, repeating the last once the script is exhausted.
#[derive(Debug, Default, Clone)]
pub struct FakeSessionManager {
    subscribe_failure: Option<SessionManagerError>,
    views: VecDeque<Vec<SessionEvent>>,
    last: Vec<SessionEvent>,
}

impl FakeSessionManager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn failing(code: i32) -> Self {
        Self {
            subscribe_failure: Some(SessionManagerError { code }),
            ..Self::default()
        }
    }
    pub fn then_view(mut self, view: Vec<SessionEvent>) -> Self {
        self.views.push_back(view);
        self
    }
}

impl SessionManager for FakeSessionManager {
    fn subscribe(&mut self) -> Result<Vec<SessionEvent>, SessionManagerError> {
        if let Some(e) = self.subscribe_failure.clone() {
            return Err(e);
        }
        Ok(self.poll())
    }
    fn poll(&mut self) -> Vec<SessionEvent> {
        if let Some(view) = self.views.pop_front() {
            self.last = view;
        }
        self.last.clone()
    }
}

#[cfg(windows)]
mod live {
    //! `IMMDeviceEnumerator` → per-endpoint `IAudioSessionManager2` → session enumeration, with an
    //! `IAudioSessionNotification` registered on every active endpoint and an
    //! `IAudioSessionEvents` sink on every session. Notifications arrive on COM threads and are
    //! queued; `poll` re-enumerates endpoints (so a device plugged in mid-session is observed) and
    //! sessions, and every notified state change is applied on top of the enumerated view, so a
    //! transition is visible at the next poll without waiting for the enumeration to catch up.

    use super::{SessionEvent, SessionFlow, SessionManager, SessionManagerError, SessionState};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, Mutex};
    use windows::core::{implement, Interface, Ref};
    use windows::Win32::Media::Audio::{
        eCapture, eRender, AudioSessionDisconnectReason, AudioSessionState,
        AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl,
        IAudioSessionControl2, IAudioSessionEvents, IAudioSessionEvents_Impl,
        IAudioSessionManager2, IAudioSessionNotification, IAudioSessionNotification_Impl,
        IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    type Queue = Arc<Mutex<Vec<SessionEvent>>>;
    type KnownSessions = Arc<Mutex<BTreeSet<String>>>;
    type LiveSessionRegistration =
        super::SessionRegistration<IAudioSessionControl, IAudioSessionEvents>;
    type RegisteredSessions = Arc<Mutex<Vec<LiveSessionRegistration>>>;

    struct ComInit(bool);

    impl ComInit {
        fn new() -> Self {
            // SAFETY: plain COM initialisation on this thread.
            ComInit(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
        }
    }

    impl Drop for ComInit {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balanced with the successful initialisation.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Sink for new sessions on one endpoint: the session is registered for state events and its
    /// current state queued, so a session created between two polls is not missed.
    #[implement(IAudioSessionNotification)]
    struct SessionCreatedSink {
        endpoint_id: String,
        flow: SessionFlow,
        queue: Queue,
        known_sessions: KnownSessions,
        registered: RegisteredSessions,
    }

    impl IAudioSessionNotification_Impl for SessionCreatedSink_Impl {
        fn OnSessionCreated(
            &self,
            session: Ref<'_, IAudioSessionControl>,
        ) -> windows::core::Result<()> {
            if let Some(control) = session.as_ref() {
                register_session_events(
                    control,
                    &self.endpoint_id,
                    self.flow,
                    &self.queue,
                    &self.known_sessions,
                    &self.registered,
                );
            }
            Ok(())
        }
    }

    /// Sink for one session's state changes.
    #[implement(IAudioSessionEvents)]
    struct SessionEventsSink {
        event: SessionEvent,
        queue: Queue,
    }

    impl IAudioSessionEvents_Impl for SessionEventsSink_Impl {
        fn OnDisplayNameChanged(
            &self,
            _: &windows::core::PCWSTR,
            _: *const windows::core::GUID,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnIconPathChanged(
            &self,
            _: &windows::core::PCWSTR,
            _: *const windows::core::GUID,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnSimpleVolumeChanged(
            &self,
            _: f32,
            _: windows::core::BOOL,
            _: *const windows::core::GUID,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnChannelVolumeChanged(
            &self,
            _: u32,
            _: *const f32,
            _: u32,
            _: *const windows::core::GUID,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnGroupingParamChanged(
            &self,
            _: *const windows::core::GUID,
            _: *const windows::core::GUID,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn OnStateChanged(&self, newstate: AudioSessionState) -> windows::core::Result<()> {
            if let Ok(mut q) = self.queue.lock() {
                let mut event = self.event.clone();
                event.state = map_state(newstate);
                q.push(event);
            }
            Ok(())
        }
        fn OnSessionDisconnected(
            &self,
            _: AudioSessionDisconnectReason,
        ) -> windows::core::Result<()> {
            if let Ok(mut q) = self.queue.lock() {
                let mut event = self.event.clone();
                event.state = SessionState::Expired;
                q.push(event);
            }
            Ok(())
        }
    }

    fn map_state(state: AudioSessionState) -> SessionState {
        if state == AudioSessionStateActive {
            SessionState::Active
        } else if state == AudioSessionStateExpired {
            SessionState::Expired
        } else {
            SessionState::Inactive
        }
    }

    fn take_string(p: windows::core::PWSTR) -> String {
        // SAFETY: CoTaskMem string returned by WASAPI, freed after copying.
        let text = unsafe { p.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(p.0.cast())) };
        text
    }

    fn endpoint_id(device: &IMMDevice) -> Option<String> {
        // SAFETY: GetId returns a CoTaskMem string.
        unsafe { device.GetId() }.ok().map(take_string)
    }

    fn session_key(control2: &IAudioSessionControl2, endpoint_id: &str, pid: u32) -> String {
        // SAFETY: returns a CoTaskMem string.
        match unsafe { control2.GetSessionInstanceIdentifier() } {
            Ok(p) => take_string(p),
            Err(_) => format!("{endpoint_id}|{pid}"),
        }
    }

    /// Registers an events sink on a session and queues its current state.
    fn register_session_events(
        control: &IAudioSessionControl,
        endpoint_id: &str,
        flow: SessionFlow,
        queue: &Queue,
        known_sessions: &KnownSessions,
        registered: &RegisteredSessions,
    ) {
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            return;
        };
        // SAFETY: live session control.
        let pid = unsafe { control2.GetProcessId() }.unwrap_or(0);
        if pid == 0 {
            return;
        }
        let key = session_key(&control2, endpoint_id, pid);
        let Ok(mut known) = known_sessions.lock() else {
            return;
        };
        if !known.insert(key.clone()) {
            return;
        }
        drop(known);
        let state = unsafe { control.GetState() }
            .map(map_state)
            .unwrap_or(SessionState::Inactive);
        let event = SessionEvent {
            pid,
            session_key: key.clone(),
            endpoint_id: endpoint_id.to_string(),
            flow,
            state,
        };
        let sink: IAudioSessionEvents = SessionEventsSink {
            event: event.clone(),
            queue: queue.clone(),
        }
        .into();
        // SAFETY: registers a live COM sink kept alive in `registered`.
        if unsafe { control.RegisterAudioSessionNotification(&sink) }.is_ok() {
            if let Ok(mut r) = registered.lock() {
                r.push(super::SessionRegistration::new(
                    endpoint_id,
                    &key,
                    control.clone(),
                    sink,
                ));
            }
        }
        if let Ok(mut q) = queue.lock() {
            q.push(event);
        }
    }

    pub struct WindowsSessionManager {
        managers: BTreeMap<
            String,
            (
                IAudioSessionManager2,
                SessionFlow,
                IAudioSessionNotification,
            ),
        >,
        registered: RegisteredSessions,
        known_sessions: KnownSessions,
        queue: Queue,
        _com: ComInit,
    }

    impl Default for WindowsSessionManager {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WindowsSessionManager {
        pub fn new() -> Self {
            Self {
                managers: BTreeMap::new(),
                registered: Arc::new(Mutex::new(Vec::new())),
                known_sessions: Arc::new(Mutex::new(BTreeSet::new())),
                queue: Arc::new(Mutex::new(Vec::new())),
                _com: ComInit::new(),
            }
        }

        /// Enumerates the active endpoints, activating a session manager and registering the
        /// session-created sink for every endpoint not seen before. Returns how many endpoints
        /// are known and whether any registration failed.
        fn refresh_endpoints(&mut self) -> Result<usize, SessionManagerError> {
            let err = |e: windows::core::Error| SessionManagerError { code: e.code().0 };
            // SAFETY: standard enumerator creation and endpoint enumeration.
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(err)?;
            let mut active = Vec::new();
            for (flow, kind) in [
                (eCapture, SessionFlow::Capture),
                (eRender, SessionFlow::Render),
            ] {
                let devices = unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }
                    .map_err(err)?;
                let count = unsafe { devices.GetCount() }.map_err(err)?;
                for i in 0..count {
                    let Ok(device) = (unsafe { devices.Item(i) }) else {
                        continue;
                    };
                    let Some(id) = endpoint_id(&device) else {
                        continue;
                    };
                    active.push((id, kind, device));
                }
            }
            let active_ids: BTreeSet<String> = active.iter().map(|(id, _, _)| id.clone()).collect();
            let known_ids: BTreeSet<String> = self.managers.keys().cloned().collect();
            let unhealthy_ids: BTreeSet<String> = self
                .managers
                .iter()
                .filter_map(|(id, (manager, _, _))| {
                    // SAFETY: a failed enumeration means this registration no longer represents
                    // the active device currently carrying the same id.
                    unsafe { manager.GetSessionEnumerator() }
                        .is_err()
                        .then(|| id.clone())
                })
                .collect();
            let (removed, added) =
                super::endpoint_refresh_plan(&known_ids, &active_ids, &unhealthy_ids);
            for id in removed {
                if let Some((manager, _, sink)) = self.managers.remove(&id) {
                    // SAFETY: unregisters the exact sink previously registered on this manager.
                    let _ = unsafe { manager.UnregisterSessionNotification(&sink) };
                }
                let removed_sessions = match (self.registered.lock(), self.known_sessions.lock()) {
                    (Ok(mut registered), Ok(mut known)) => {
                        super::take_endpoint_registrations(&mut registered, &mut known, &id)
                    }
                    _ => Vec::new(),
                };
                for registration in removed_sessions {
                    // SAFETY: unregisters the exact live session sink before dropping it.
                    let _ = unsafe {
                        registration
                            .control
                            .UnregisterAudioSessionNotification(&registration.sink)
                    };
                }
            }
            let added: BTreeSet<String> = added.into_iter().collect();
            for (id, kind, device) in active {
                if !added.contains(&id) {
                    continue;
                }
                let manager: IAudioSessionManager2 =
                    match unsafe { device.Activate(CLSCTX_ALL, None) } {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                let sink: IAudioSessionNotification = SessionCreatedSink {
                    endpoint_id: id.clone(),
                    flow: kind,
                    queue: self.queue.clone(),
                    known_sessions: self.known_sessions.clone(),
                    registered: self.registered.clone(),
                }
                .into();
                // SAFETY: registers a live COM sink kept alive in `notifications`.
                unsafe { manager.RegisterSessionNotification(&sink) }.map_err(err)?;
                self.managers.insert(id, (manager, kind, sink));
            }
            Ok(self.managers.len())
        }

        /// The enumerated view of every session on every known endpoint, registering events on
        /// sessions seen for the first time.
        fn enumerate_sessions(&mut self) -> Vec<SessionEvent> {
            let mut out = Vec::new();
            let managers: Vec<(String, IAudioSessionManager2, SessionFlow)> = self
                .managers
                .iter()
                .map(|(id, (m, f, _))| (id.clone(), m.clone(), *f))
                .collect();
            for (endpoint_id, manager, flow) in managers {
                // SAFETY: session enumeration on a live manager.
                let Ok(list) = (unsafe { manager.GetSessionEnumerator() }) else {
                    continue;
                };
                let count = unsafe { list.GetCount() }.unwrap_or(0);
                for i in 0..count {
                    let Ok(control) = (unsafe { list.GetSession(i) }) else {
                        continue;
                    };
                    let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                        continue;
                    };
                    let pid = unsafe { control2.GetProcessId() }.unwrap_or(0);
                    if pid == 0 {
                        continue;
                    }
                    let key = session_key(&control2, &endpoint_id, pid);
                    register_session_events(
                        &control,
                        &endpoint_id,
                        flow,
                        &self.queue,
                        &self.known_sessions,
                        &self.registered,
                    );
                    let state = match unsafe { control.GetState() } {
                        Ok(s) => map_state(s),
                        Err(_) => continue,
                    };
                    out.push(SessionEvent {
                        pid,
                        session_key: key,
                        endpoint_id: endpoint_id.clone(),
                        flow,
                        state,
                    });
                }
            }
            out
        }

        /// Applies the notified state changes on top of the enumerated view, so a transition that
        /// happened since the enumeration is reported now rather than at the next poll.
        fn apply_notifications(&self, view: Vec<SessionEvent>) -> Vec<SessionEvent> {
            match self.queue.lock() {
                // Preserve callback order. The final enumerated view follows as reconciliation,
                // so an Active -> Inactive transition wholly between polls remains visible.
                Ok(mut queue) => super::merge_notifications(&mut queue, view),
                Err(_) => view,
            }
        }
    }

    impl SessionManager for WindowsSessionManager {
        fn subscribe(&mut self) -> Result<Vec<SessionEvent>, SessionManagerError> {
            if self.refresh_endpoints()? == 0 {
                return Err(SessionManagerError { code: 0 });
            }
            Ok(self.poll())
        }

        fn poll(&mut self) -> Vec<SessionEvent> {
            // Endpoints connected mid-session are picked up here; a refresh failure keeps the
            // known endpoints and is reported through the enumeration staying stale.
            let _ = self.refresh_endpoints();
            let view = self.enumerate_sessions();
            self.apply_notifications(view)
        }
    }
}

#[cfg(windows)]
pub use live::WindowsSessionManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_manager_repeats_its_last_view_and_can_fail_to_subscribe() {
        let ev = SessionEvent {
            pid: 1,
            session_key: "s".into(),
            endpoint_id: "{e}".into(),
            flow: SessionFlow::Capture,
            state: SessionState::Active,
        };
        let mut m = FakeSessionManager::new().then_view(vec![ev.clone()]);
        assert_eq!(m.subscribe().unwrap(), vec![ev.clone()]);
        assert_eq!(m.poll(), vec![ev]);
        let mut f = FakeSessionManager::failing(-1);
        assert_eq!(f.subscribe(), Err(SessionManagerError { code: -1 }));
    }

    #[test]
    fn unhealthy_same_id_endpoint_is_removed_and_freshly_registered() {
        let id = "{bluetooth-headset}".to_string();
        let known = BTreeSet::from([id.clone()]);
        let active_again = BTreeSet::from([id.clone()]);
        let unhealthy = BTreeSet::from([id.clone()]);
        assert_eq!(
            endpoint_refresh_plan(&known, &active_again, &unhealthy),
            (vec![id.clone()], vec![id])
        );
    }

    #[test]
    fn endpoint_teardown_removes_its_registrations_but_not_queued_callbacks() {
        let mut registered = vec![
            SessionRegistration::new("a", "a-1", 1, 11),
            SessionRegistration::new("b", "b-1", 2, 12),
            SessionRegistration::new("a", "a-2", 3, 13),
        ];
        let mut known = BTreeSet::from(["a-1".into(), "a-2".into(), "b-1".into()]);
        let queued = [SessionEvent {
            pid: 7,
            session_key: "a-1".into(),
            endpoint_id: "a".into(),
            flow: SessionFlow::Capture,
            state: SessionState::Active,
        }];

        let removed = take_endpoint_registrations(&mut registered, &mut known, "a");

        assert_eq!(removed.len(), 2);
        assert_eq!(
            removed
                .iter()
                .map(|entry| (entry.control, entry.sink))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([(1, 11), (3, 13)])
        );
        assert_eq!(registered.len(), 1);
        assert_eq!(known, BTreeSet::from(["b-1".into()]));
        assert_eq!(queued.len(), 1, "teardown must not erase callback evidence");
    }

    #[test]
    fn notification_merge_preserves_callback_order_before_reconciliation() {
        let event = |state| SessionEvent {
            pid: 7,
            session_key: "s".into(),
            endpoint_id: "a".into(),
            flow: SessionFlow::Capture,
            state,
        };
        let mut queued = vec![event(SessionState::Active), event(SessionState::Inactive)];
        let merged = merge_notifications(&mut queued, vec![event(SessionState::Expired)]);
        assert_eq!(
            merged.iter().map(|item| item.state).collect::<Vec<_>>(),
            [
                SessionState::Active,
                SessionState::Inactive,
                SessionState::Expired
            ]
        );
        assert!(queued.is_empty());
    }
}
