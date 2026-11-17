//! The side that listens for connections from the extension. Handles sockets and files and leaves
//! state decisions to [`Hub`].
//!
//! At the extension's request the browser launches a relay process ([`super::relay`]), which connects
//! to this app over loopback TCP.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::Context;
use base64::Engine as _;
use meeting_assistant_core::{BrowserLink, BrowserMeeting, BrowserTab};

use super::endpoint::Endpoint;
use super::hub::{ChunkSink, Finished, Hub, Phase};
use super::protocol::{Incoming, MAX_HELLO_FRAME, Outgoing, read_frame, read_frame_within, tokens_match, write_frame};
use crate::linked::BrowserPort;

const START_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a new connection may take to present the token. The relay sends it immediately after connecting.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// How many connections may be waiting to present the token at once. Any local process can connect to
/// the loopback port, so those that have not proved themselves are kept few and short-lived.
const MAX_UNAUTHENTICATED: usize = 8;

/// A place among the connections that have not presented the token yet. Dropping it frees the place.
struct PendingSlot<'a>(&'a AtomicUsize);

impl<'a> PendingSlot<'a> {
    fn take(pending: &'a AtomicUsize) -> Option<Self> {
        pending
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_UNAUTHENTICATED).then_some(count + 1)
            })
            .ok()
            .map(|_| Self(pending))
    }
}

impl Drop for PendingSlot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Writes recording data to a file as is.
struct FileSink(File);

impl ChunkSink for FileSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }
}

#[derive(Default)]
struct State {
    hub: Hub,
    /// Per-connection destination for commands.
    streams: HashMap<u64, TcpStream>,
    /// Path of the file being recorded to.
    capture_path: Option<PathBuf>,
}

impl State {
    fn send(&self, link: u64, message: &Outgoing) -> anyhow::Result<()> {
        let stream = self
            .streams
            .get(&link)
            .context("Not connected to the browser extension")?;
        write_frame(&mut &*stream, &message.to_body()).context("Cannot send a command to the extension")
    }
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    token: String,
    /// Number of connections that have not presented the token yet.
    unauthenticated: AtomicUsize,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Waits until `still_waiting` becomes false or the timeout expires.
    fn wait_while<'a>(
        &self,
        mut state: MutexGuard<'a, State>,
        timeout: Duration,
        still_waiting: impl Fn(&State) -> bool,
    ) -> MutexGuard<'a, State> {
        let deadline = Instant::now() + timeout;
        while still_waiting(&state) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        state
    }
}

/// Listens for connections from the extension. Clones share the same state.
#[derive(Clone)]
pub struct BrowserLinkServer(Arc<Shared>);

impl BrowserLinkServer {
    /// Starts listening and publishes the endpoint for the relay process.
    pub fn start() -> anyhow::Result<Self> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("Cannot listen for connections from the extension")?;
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("Cannot obtain random bytes: {e}"))?;
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random);
        Endpoint {
            port: listener.local_addr()?.port(),
            token: token.clone(),
        }
        .publish()?;

        let shared = Arc::new(Shared {
            state: Mutex::default(),
            changed: Condvar::new(),
            token,
            unauthenticated: AtomicUsize::new(0),
        });
        std::thread::Builder::new().name("browser-link".into()).spawn({
            let shared = shared.clone();
            move || {
                for stream in listener.incoming().flatten() {
                    let shared = shared.clone();
                    let _ = std::thread::Builder::new()
                        .name("browser-link-conn".into())
                        .spawn(move || serve(&shared, stream));
                }
            }
        })?;
        Ok(Self(shared))
    }
}

impl BrowserPort for BrowserLinkServer {
    fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        self.0.lock().hub.snapshot()
    }

    /// Writes incoming data to `path`.
    fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<()> {
        let mut state = self.0.lock();
        let file = File::create(path).with_context(|| format!("Cannot create the output file: {}", path.display()))?;
        let started = state
            .hub
            .begin_capture(tab, Box::new(FileSink(file)))
            .and_then(|instruction| state.send(tab.link, &instruction));
        if let Err(error) = started {
            state.hub.end_capture();
            drop(state);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        state.capture_path = Some(path.to_owned());

        state = self.0.wait_while(state, START_TIMEOUT, |s| {
            s.hub.capture_phase() == Some(&Phase::Starting)
        });
        if state.hub.capture_phase() == Some(&Phase::Recording) {
            return Ok(());
        }

        // Could not start. Tell the extension to stop in case it is partway into starting, and leave no empty file.
        let _ = state.send(tab.link, &Outgoing::Stop { tab: tab.tab });
        let finished = state.hub.end_capture();
        state.capture_path = None;
        drop(state);
        let reason = finished.as_ref().and_then(|finished| finished.outcome().err());
        drop(finished);
        let _ = std::fs::remove_file(path);
        match reason {
            Some(reason) => anyhow::bail!("The extension cannot record the tab: {reason}"),
            None => anyhow::bail!("The extension exited before it started recording the tab"),
        }
    }

    fn stop_capture(&self) -> anyhow::Result<()> {
        let mut state = self.0.lock();
        if let Some((link, instruction)) = state.hub.stop_request() {
            // Even if the connection is gone and the command cannot be sent, finalize the file with what has arrived.
            let _ = state.send(link, &instruction);
            state = self.0.wait_while(state, STOP_TIMEOUT, |s| {
                s.hub.capture_phase().is_some_and(Phase::is_in_progress)
            });
        }
        let Some(finished) = state.hub.end_capture() else {
            return Ok(());
        };
        let path = state.capture_path.take();
        drop(state);
        close(finished, path)
    }

    fn take_failure(&self) -> Option<String> {
        let mut state = self.0.lock();
        let finished = state.hub.take_failed()?;
        let path = state.capture_path.take();
        drop(state);
        // Keep the partial data. The result of finalizing is the failure cause itself.
        close(finished, path).err().map(|error| format!("{error:#}"))
    }
}

/// Finishes writing. If the extension could not record as MP4, fixes the file extension to match the contents.
fn close(finished: Finished, path: Option<PathBuf>) -> anyhow::Result<()> {
    let outcome = finished.outcome();
    let extension = finished.container_extension();
    log::info!("Wrote {} bytes of tab recording ({})", finished.bytes, finished.mime);
    // Drop the sink to close the file.
    drop(finished);

    if let Some(path) = path
        && path.extension().is_some_and(|current| current != extension)
    {
        let renamed = path.with_extension(extension);
        std::fs::rename(&path, &renamed)?;
        log::warn!(
            "Saved with a different extension to match the format the browser recorded: {}",
            renamed.display()
        );
    }
    outcome.map_err(|reason| anyhow::anyhow!("The tab recording failed partway: {reason}"))
}

/// Serves one connection until it drops.
fn serve(shared: &Shared, stream: TcpStream) {
    let Ok(mut reader) = stream.try_clone() else { return };

    // The first message confirms that this is a relay process launched by this app. Until it has,
    // the peer gets a small message, a short time and one of a few places, so that a process which
    // does not know the token cannot tie up memory or threads.
    let host_pid = {
        let Some(_slot) = PendingSlot::take(&shared.unauthenticated) else {
            log::warn!("Rejected a connection from the extension (too many unauthenticated connections).");
            return;
        };
        if stream.set_read_timeout(Some(HELLO_TIMEOUT)).is_err() {
            return;
        }
        let hello = read_frame_within(&mut reader, MAX_HELLO_FRAME)
            .ok()
            .and_then(|body| Incoming::parse(&body).ok());
        let Some(Incoming::Hello { token, host_pid }) = hello else {
            return;
        };
        if !tokens_match(&token, &shared.token) {
            log::warn!("Rejected a connection from the extension (token mismatch).");
            return;
        }
        // An authenticated connection stays open while idle, for as long as the browser runs.
        if stream.set_read_timeout(None).is_err() {
            return;
        }
        host_pid
    };

    let link = {
        let mut state = shared.lock();
        let link = state.hub.connect(host_pid);
        state.streams.insert(link, stream);
        link
    };
    log::info!("Connected to the browser extension.");

    while let Ok(body) = read_frame(&mut reader) {
        match Incoming::parse(&body) {
            Ok(message) => shared.lock().hub.receive(link, message),
            Err(error) => log::warn!("Cannot parse a message from the extension: {error}"),
        }
        shared.changed.notify_all();
    }

    {
        let mut state = shared.lock();
        state.hub.disconnect(link);
        state.streams.remove(&link);
    }
    shared.changed.notify_all();
    log::info!("Connection to the browser extension closed.");
}
