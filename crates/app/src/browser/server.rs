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
use meeting_assistant_core::{BrowserLink, BrowserMeeting, BrowserTab, naming};

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
        let mut count = pending.load(Ordering::Acquire);
        while count < MAX_UNAUTHENTICATED {
            match pending.compare_exchange_weak(count, count + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(Self(pending)),
                Err(current) => count = current,
            }
        }
        None
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

    /// Writes incoming data to `path`, renamed to the extension of the container the browser records in.
    fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<PathBuf> {
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

        state = self.0.wait_while(state, START_TIMEOUT, |s| {
            s.hub.capture_phase() == Some(&Phase::Starting)
        });
        if state.hub.capture_phase() == Some(&Phase::Recording) {
            // The container is known now. Name the file after it before anyone is told where the recording is.
            let extension = state.hub.capture_extension().unwrap_or(naming::RECORDING_EXTENSION);
            return Ok(match name_for_container(path, extension) {
                Ok(written) => written,
                Err(error) => {
                    log::warn!("Cannot give the recording the .{extension} extension; it keeps its name: {error}");
                    path.to_owned()
                }
            });
        }

        // Could not start. Tell the extension to stop in case it is partway into starting, and leave no empty file.
        let _ = state.send(tab.link, &Outgoing::Stop { tab: tab.tab });
        let finished = state.hub.end_capture();
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
        drop(state);
        close(finished)
    }

    fn take_failure(&self) -> Option<String> {
        let finished = self.0.lock().hub.take_failed()?;
        // Keep the partial data. The result of finalizing is the failure cause itself.
        close(finished).err().map(|error| format!("{error:#}"))
    }
}

/// Gives the file being written the extension of `extension`'s container, without replacing a file that already
/// has that name. The file stays open while it is renamed: Rust opens files so that this is allowed on Windows too.
fn name_for_container(path: &Path, extension: &str) -> io::Result<PathBuf> {
    if path.extension().is_some_and(|current| current == extension) {
        return Ok(path.to_owned());
    }
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let renamed = (1..)
        .map(|attempt| path.with_file_name(naming::candidate_file_name_with_extension(&stem, attempt, extension)))
        .find(|candidate| !candidate.exists())
        .expect("candidates never run out");
    std::fs::rename(path, &renamed)?;
    log::info!(
        "Recording as .{extension}, the format the browser records: {}",
        renamed.display()
    );
    Ok(renamed)
}

/// Finishes writing.
fn close(finished: Finished) -> anyhow::Result<()> {
    let outcome = finished.outcome();
    log::info!("Wrote {} bytes of tab recording ({})", finished.bytes, finished.mime);
    // Drop the sink to close the file.
    drop(finished);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_is_renamed_to_its_container_while_it_is_written_and_replaces_no_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("2026-11-18_10-00-00_GoogleMeet.mp4");
        std::fs::write(folder.path().join("2026-11-18_10-00-00_GoogleMeet.webm"), b"earlier").unwrap();
        let mut file = File::create(&path).unwrap();
        file.write_all(b"first").unwrap();

        let renamed = name_for_container(&path, "webm").unwrap();
        file.write_all(b" second").unwrap();
        drop(file);

        assert_eq!(renamed, folder.path().join("2026-11-18_10-00-00_GoogleMeet_2.webm"));
        assert_eq!(std::fs::read(&renamed).unwrap(), b"first second");
        assert!(!path.exists());
        let earlier = folder.path().join("2026-11-18_10-00-00_GoogleMeet.webm");
        assert_eq!(std::fs::read(earlier).unwrap(), b"earlier");
    }

    #[test]
    fn a_recording_already_named_after_its_container_keeps_its_name() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.mp4");
        File::create(&path).unwrap();

        assert_eq!(name_for_container(&path, "mp4").unwrap(), path);
        assert!(path.exists());
    }

    #[test]
    fn unauthenticated_connections_are_limited_and_dropping_one_frees_its_place() {
        let pending = AtomicUsize::new(0);
        let mut slots: Vec<_> = (0..MAX_UNAUTHENTICATED)
            .map(|_| PendingSlot::take(&pending).expect("a place below the limit"))
            .collect();

        assert!(PendingSlot::take(&pending).is_none());

        slots.pop();
        assert!(PendingSlot::take(&pending).is_some());
        drop(slots);
        assert_eq!(pending.load(Ordering::Acquire), 0);
    }
}
