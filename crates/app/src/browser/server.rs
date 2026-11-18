//! The side that listens for connections from the extension. Handles sockets and files and leaves
//! state decisions to [`Hub`].
//!
//! At the extension's request the browser launches a relay process ([`super::relay`]), which connects
//! to this app over loopback TCP and carries the control messages. For each recording the app opens a second,
//! short-lived loopback listener; the extension's recording page connects to it with a WebSocket and sends the
//! recording as binary messages, which are appended to the file as they are.
//!
//! Both listeners first ask the operating system who owns the other end of a new connection and close it unread
//! unless it is one of the current user's processes ([`peer`](super::peer)); the token checks come after that.

use std::collections::{HashMap, VecDeque};
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
use tungstenite::handshake::server::{ErrorResponse, Request, Response, ServerHandshake};
use tungstenite::handshake::{HandshakeError, MidHandshake};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

use super::endpoint::Endpoint;
use super::hub::{CaptureId, ChunkSink, Finished, Hub, Phase, Upload};
use super::peer::{self, LoopbackPeers, UserId};
use super::protocol::{
    Incoming, MAX_FRAME, MAX_HELLO_FRAME, Outgoing, UploadWarning, accept_upload_hello, is_extension_origin,
    read_frame, read_frame_within, tokens_match, upload_close_outcome, write_frame,
};
use crate::linked::BrowserPort;

const START_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a new connection may take to present the token. The relay sends it immediately after connecting.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// The recording page sends data every second; this long without any means it is gone.
const UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the upload listener checks for a connection while it waits for the recording page.
const UPLOAD_ACCEPT_POLL: Duration = Duration::from_millis(20);
/// How many connections may be waiting to present the token at once. Any local process can connect to
/// the loopback port, so those that have not proved themselves are kept few and short-lived.
const MAX_UNAUTHENTICATED: usize = 8;

/// The check a recording upload's WebSocket handshake makes on its request.
type OriginCheck = fn(&Request, Response) -> Result<Response, ErrorResponse>;

/// A connection to a recording's upload listener that has not presented the token yet.
enum Unproven {
    Handshaking(MidHandshake<ServerHandshake<TcpStream, OriginCheck>>),
    AwaitingHello(WebSocket<TcpStream>),
}

/// How far an unproven connection got.
enum Progress {
    Proven(WebSocket<TcpStream>, String),
    Waiting(Unproven),
}

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
    peers: Arc<dyn LoopbackPeers>,
    /// The user this app runs as: only this user's processes may connect.
    user: UserId,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether a just-accepted connection may be read: only if one of the current user's processes made it.
    fn admit(&self, stream: &TcpStream) -> Result<(), String> {
        let (Ok(local), Ok(peer)) = (stream.local_addr(), stream.peer_addr()) else {
            return Err("its addresses cannot be read".to_owned());
        };
        peer::admit(self.peers.owner(local.port(), peer.port()), &self.user)
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
    /// Starts listening and publishes the endpoint for the relay process. `peers` tells who made a connection.
    pub fn start(peers: Arc<dyn LoopbackPeers>) -> anyhow::Result<Self> {
        let user = peers
            .current_user()
            .context("Cannot tell which user this app runs as")?;
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("Cannot listen for connections from the extension")?;
        let token = random_token()?;
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
            peers,
            user,
        });
        std::thread::Builder::new().name("browser-link".into()).spawn({
            let shared = shared.clone();
            move || accept_links(&shared, &listener)
        })?;
        Ok(Self(shared))
    }
}

/// Serves each connection the current user's processes make to the control listener, on a thread of its own.
fn accept_links(shared: &Arc<Shared>, listener: &TcpListener) {
    for stream in listener.incoming().flatten() {
        if let Err(reason) = shared.admit(&stream) {
            log::warn!("Refused a connection from the extension: {reason}.");
            continue;
        }
        let shared = shared.clone();
        let _ = std::thread::Builder::new()
            .name("browser-link-conn".into())
            .spawn(move || serve(&shared, stream));
    }
}

impl BrowserPort for BrowserLinkServer {
    fn snapshot(&self) -> (Vec<BrowserLink>, Vec<BrowserMeeting>) {
        self.0.lock().hub.snapshot()
    }

    /// Writes incoming data to `path`, renamed to the extension of the container the browser records in.
    fn start_capture(&self, tab: BrowserTab, path: &Path) -> anyhow::Result<PathBuf> {
        // The recording page connects here and nowhere else; the listener lives only until it has.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("Cannot listen for the recording from the extension")?;
        let upload = Upload {
            url: format!("ws://127.0.0.1:{}/", listener.local_addr()?.port()),
            token: random_token()?,
        };
        let token = upload.token.clone();

        let mut state = self.0.lock();
        let file = File::create(path).with_context(|| format!("Cannot create the output file: {}", path.display()))?;
        let started = state
            .hub
            .begin_capture(tab, Box::new(FileSink(file)), upload)
            .and_then(|(id, instruction)| {
                let shared = self.0.clone();
                std::thread::Builder::new()
                    .name("browser-upload".into())
                    .spawn(move || receive_upload(&shared, &listener, id, &token))
                    .context("Cannot start receiving the recording")?;
                state.send(tab.link, &instruction)
            });
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
    let name =
        naming::first_unused_file_name_with_extension(&stem, extension, |name| path.with_file_name(name).exists());
    let renamed = path.with_file_name(name);
    std::fs::rename(path, &renamed)?;
    log::info!(
        "Recording as .{extension}, the format the browser records: {}",
        renamed.display()
    );
    Ok(renamed)
}

/// A random secret, for the relay and for each recording's upload.
fn random_token() -> anyhow::Result<String> {
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("Cannot obtain random bytes: {e}"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random))
}

/// Takes the recording page's upload for recording `id` and appends what it sends, until it closes.
fn receive_upload(shared: &Shared, listener: &TcpListener, id: CaptureId, token: &str) {
    let Some(mut socket) = accept_upload(shared, listener, id, token) else {
        return;
    };
    let outcome = loop {
        let message = socket.read();
        let mut state = shared.lock();
        if !state.hub.is_capturing(id) {
            // The recording was ended or failed on this side; the page notices the close and stops.
            break None;
        }
        match message {
            Ok(Message::Binary(bytes)) => state.hub.upload_data(id, &bytes),
            Ok(Message::Text(text)) => match serde_json::from_str::<UploadWarning>(&text) {
                Ok(warning) => log::warn!("Warning from the extension: {}", warning.warning),
                Err(_) => log::warn!("Ignored an unexpected text message in the recording upload."),
            },
            Ok(Message::Close(frame)) => {
                break Some(upload_close_outcome(
                    frame
                        .as_ref()
                        .map(|frame| (u16::from(frame.code), frame.reason.as_str())),
                ));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) =>
            {
                break Some(Err(format!(
                    "No recording data arrived for {} seconds",
                    UPLOAD_IDLE_TIMEOUT.as_secs()
                )));
            }
            Err(error) => break Some(Err(format!("The recording upload broke off: {error}"))),
        }
    };
    if let Some(outcome) = outcome {
        shared.lock().hub.upload_closed(id, outcome);
        shared.changed.notify_all();
    }
    let _ = socket.close(None);
    let _ = socket.flush();
}

/// Waits, while recording `id` is starting, for the recording page to connect and prove itself. Other connections
/// are turned away and do not end the wait.
///
/// Connections are advanced side by side without blocking, each with a fixed time to present the token, so a peer
/// that connects and then sends slowly or nothing cannot hold up the recording page. When too many are waiting, the
/// one that has waited longest is dropped to make room.
fn accept_upload(shared: &Shared, listener: &TcpListener, id: CaptureId, token: &str) -> Option<WebSocket<TcpStream>> {
    listener.set_nonblocking(true).ok()?;
    let deadline = Instant::now() + START_TIMEOUT;
    let mut waiting: VecDeque<(Unproven, Instant)> = VecDeque::new();
    while Instant::now() < deadline && shared.lock().hub.is_capturing(id) {
        loop {
            match listener.accept() {
                Ok((stream, _)) if let Err(reason) = shared.admit(&stream) => {
                    log::warn!("Refused a connection for the recording: {reason}.");
                }
                Ok((stream, _)) => match start_upload_handshake(stream) {
                    Ok(connection) => {
                        if waiting.len() == MAX_UNAUTHENTICATED {
                            waiting.pop_front();
                            log::warn!("Dropped a connection for the recording that had not presented the token.");
                        }
                        waiting.push_back((connection, Instant::now() + HELLO_TIMEOUT));
                    }
                    Err(error) => log::warn!("Rejected a connection for the recording: {error:#}"),
                },
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    log::warn!("Cannot accept the recording from the extension: {error}");
                    return None;
                }
            }
        }

        let now = Instant::now();
        for _ in 0..waiting.len() {
            let Some((connection, expires)) = waiting.pop_front() else {
                break;
            };
            if now >= expires {
                log::warn!("Rejected a connection for the recording: it did not present the token in time");
                continue;
            }
            match advance_upload_handshake(connection, token) {
                Ok(Progress::Proven(socket, mime)) => {
                    shared.lock().hub.upload_opened(id, mime);
                    shared.changed.notify_all();
                    return Some(socket);
                }
                Ok(Progress::Waiting(connection)) => waiting.push_back((connection, expires)),
                Err(error) => log::warn!("Rejected a connection for the recording: {error:#}"),
            }
        }
        std::thread::sleep(UPLOAD_ACCEPT_POLL);
    }
    None
}

/// Accepts a WebSocket only from the extension's pages. Until the token is presented the peer may send only a small
/// message.
#[expect(clippy::result_large_err, reason = "tungstenite fixes the callback's error type")]
fn only_the_extension(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    if is_extension_origin(request.headers().get("origin").map(|value| value.as_bytes())) {
        return Ok(response);
    }
    let mut refusal = ErrorResponse::new(Some("Forbidden".to_owned()));
    *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
    Err(refusal)
}

fn start_upload_handshake(stream: TcpStream) -> anyhow::Result<Unproven> {
    // Set explicitly: whether an accepted socket inherits the listener's mode differs between systems.
    stream.set_nonblocking(true)?;
    let small = WebSocketConfig::default()
        .max_message_size(Some(MAX_HELLO_FRAME))
        .max_frame_size(Some(MAX_HELLO_FRAME));
    match tungstenite::accept_hdr_with_config(stream, only_the_extension as OriginCheck, Some(small)) {
        Ok(socket) => Ok(Unproven::AwaitingHello(socket)),
        Err(HandshakeError::Interrupted(handshake)) => Ok(Unproven::Handshaking(handshake)),
        Err(HandshakeError::Failure(error)) => anyhow::bail!("not a WebSocket from the extension: {error}"),
    }
}

/// Takes the handshake as far as the data that has arrived allows. The first message must carry this recording's
/// token; then the socket becomes a blocking upload with the full message size.
fn advance_upload_handshake(connection: Unproven, token: &str) -> anyhow::Result<Progress> {
    let mut socket = match connection {
        Unproven::Handshaking(handshake) => match handshake.handshake() {
            Ok(socket) => socket,
            Err(HandshakeError::Interrupted(handshake)) => {
                return Ok(Progress::Waiting(Unproven::Handshaking(handshake)));
            }
            Err(HandshakeError::Failure(error)) => anyhow::bail!("not a WebSocket from the extension: {error}"),
        },
        Unproven::AwaitingHello(socket) => socket,
    };
    let first = match socket.read() {
        Ok(first) => first,
        Err(tungstenite::Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
            return Ok(Progress::Waiting(Unproven::AwaitingHello(socket)));
        }
        Err(error) => return Err(error.into()),
    };
    let mime = accept_upload_hello(first.to_text().ok(), token).map_err(|reason| anyhow::anyhow!(reason))?;

    socket.set_config(|config| {
        config.max_message_size = Some(MAX_FRAME);
        config.max_frame_size = Some(MAX_FRAME);
    });
    let stream = socket.get_ref();
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(UPLOAD_IDLE_TIMEOUT))?;
    stream.set_write_timeout(Some(HELLO_TIMEOUT))?;
    Ok(Progress::Proven(socket, mime))
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
    use tungstenite::client::IntoClientRequest;
    use tungstenite::protocol::CloseFrame;
    use tungstenite::protocol::frame::coding::CloseCode;

    use std::io::Read;

    use super::super::peer::fakes::{CURRENT_USER, FakeLoopbackPeers};
    use super::super::protocol::{extension_web_origin, hello_body};

    use super::*;

    /// A sink that only remembers the data written to it.
    #[derive(Clone, Default)]
    struct MemorySink(Arc<Mutex<Vec<u8>>>);

    impl ChunkSink for MemorySink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
    }

    /// A recording of tab 7 that is starting, with its upload listener served on another thread.
    struct Upload {
        shared: Arc<Shared>,
        peers: Arc<FakeLoopbackPeers>,
        id: CaptureId,
        port: u16,
        sink: MemorySink,
        thread: std::thread::JoinHandle<()>,
    }

    const TOKEN: &str = "recording-token";
    const RELAY_TOKEN: &str = "relay-token";

    fn shared(peers: &Arc<FakeLoopbackPeers>) -> Arc<Shared> {
        Arc::new(Shared {
            state: Mutex::default(),
            changed: Condvar::new(),
            token: RELAY_TOKEN.into(),
            unauthenticated: AtomicUsize::new(0),
            peers: peers.clone(),
            user: UserId(CURRENT_USER.into()),
        })
    }

    /// Whether the app closed this connection, without having been sent anything, within `within`.
    fn closed_unread(stream: &mut TcpStream, within: Duration) -> bool {
        stream.set_read_timeout(Some(within)).unwrap();
        match stream.read(&mut [0u8; 1]) {
            Ok(0) => true,
            Ok(_) => false,
            Err(error) => !matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut),
        }
    }

    fn upload() -> Upload {
        let peers = Arc::new(FakeLoopbackPeers::default());
        let shared = shared(&peers);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let sink = MemorySink::default();
        let id = {
            let mut state = shared.lock();
            let link = state.hub.connect(301);
            let upload = super::Upload {
                url: format!("ws://127.0.0.1:{port}/"),
                token: TOKEN.into(),
            };
            state
                .hub
                .begin_capture(BrowserTab { link, tab: 7 }, Box::new(sink.clone()), upload)
                .unwrap()
                .0
        };
        let thread = std::thread::spawn({
            let shared = shared.clone();
            move || receive_upload(&shared, &listener, id, TOKEN)
        });
        Upload {
            shared,
            peers,
            id,
            port,
            sink,
            thread,
        }
    }

    fn connect(port: u16, origin: &str) -> tungstenite::Result<WebSocket<TcpStream>> {
        let mut request = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
        request.headers_mut().insert("Origin", origin.parse().unwrap());
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        tungstenite::client(request, stream)
            .map(|(socket, _)| socket)
            .map_err(|error| match error {
                tungstenite::HandshakeError::Failure(error) => error,
                tungstenite::HandshakeError::Interrupted(_) => unreachable!("blocking stream"),
            })
    }

    fn hello(token: &str) -> Message {
        Message::text(format!(r#"{{"token":"{token}","mime":"video/webm;codecs=vp9,opus"}}"#))
    }

    fn phase(shared: &Shared) -> Option<Phase> {
        shared.lock().hub.capture_phase().cloned()
    }

    #[test]
    fn the_recording_page_streams_binary_data_into_the_file_and_closes_it() {
        let upload = upload();
        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        page.send(Message::text(r#"{"warning":"no microphone"}"#)).unwrap();
        page.send(Message::binary(b"\x1a\x45\xdf\xa3".to_vec())).unwrap();
        page.send(Message::binary(vec![7u8; 300_000])).unwrap();
        page.close(None).unwrap();
        // Wait for the app to acknowledge the close, as the page does before it lets itself be closed.
        while page.read().is_ok() {}
        upload.thread.join().unwrap();

        assert_eq!(phase(&upload.shared), Some(Phase::Stopped));
        assert_eq!(upload.shared.lock().hub.capture_extension(), Some("webm"));
        let written = upload.sink.0.lock().unwrap();
        assert_eq!(written.len(), 4 + 300_000);
        assert_eq!(&written[..4], b"\x1a\x45\xdf\xa3");
    }

    #[test]
    fn only_the_extension_with_this_recordings_token_is_let_in() {
        let upload = upload();
        assert!(
            connect(upload.port, "https://example.com").is_err(),
            "a page that is not the extension is refused at the handshake"
        );

        let mut impostor = connect(upload.port, &extension_web_origin()).unwrap();
        impostor.send(hello("guessed")).unwrap();
        assert!(impostor.read().is_err(), "a wrong token is turned away");
        assert_eq!(
            phase(&upload.shared),
            Some(Phase::Starting),
            "and does not end the wait"
        );

        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        page.send(Message::binary(b"data".to_vec())).unwrap();
        page.close(None).unwrap();
        while page.read().is_ok() {}
        upload.thread.join().unwrap();

        assert_eq!(phase(&upload.shared), Some(Phase::Stopped));
        assert_eq!(*upload.sink.0.lock().unwrap(), b"data");
    }

    #[test]
    fn peers_that_connect_and_say_nothing_do_not_keep_the_recording_page_out() {
        let upload = upload();
        // More silent peers than may wait at once; they stay connected for the whole test.
        let silent: Vec<TcpStream> = (0..=MAX_UNAUTHENTICATED)
            .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, upload.port)).unwrap())
            .collect();
        let mut slow = TcpStream::connect((Ipv4Addr::LOCALHOST, upload.port)).unwrap();
        slow.write_all(b"GET / HTTP/1.1\r\n").unwrap();

        let started = Instant::now();
        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        while phase(&upload.shared) == Some(Phase::Starting) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(phase(&upload.shared), Some(Phase::Recording));
        assert!(
            started.elapsed() < HELLO_TIMEOUT,
            "the page got in without waiting for the other peers to time out"
        );

        page.send(Message::binary(b"data".to_vec())).unwrap();
        page.close(None).unwrap();
        while page.read().is_ok() {}
        upload.thread.join().unwrap();
        assert_eq!(*upload.sink.0.lock().unwrap(), b"data");
        drop((silent, slow));
    }

    #[test]
    fn another_users_connections_to_the_upload_are_closed_unread_and_do_not_keep_the_page_out() {
        let upload = upload();
        upload.peers.set_foreign(true);
        let mut others: Vec<TcpStream> = (0..3 * MAX_UNAUTHENTICATED)
            .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, upload.port)).unwrap())
            .collect();
        for other in &mut others {
            assert!(
                closed_unread(other, HELLO_TIMEOUT / 2),
                "closed at once, not after the time for the token"
            );
        }
        assert_eq!(phase(&upload.shared), Some(Phase::Starting));

        upload.peers.set_foreign(false);
        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        page.send(Message::binary(b"data".to_vec())).unwrap();
        page.close(None).unwrap();
        while page.read().is_ok() {}
        upload.thread.join().unwrap();
        assert_eq!(*upload.sink.0.lock().unwrap(), b"data");
    }

    #[test]
    fn another_users_connections_to_the_control_listener_are_closed_unread() {
        let peers = Arc::new(FakeLoopbackPeers::default());
        let shared = shared(&peers);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn({
            let shared = shared.clone();
            move || accept_links(&shared, &listener)
        });

        peers.set_foreign(true);
        let mut other = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        assert!(closed_unread(&mut other, HELLO_TIMEOUT / 2));
        assert_eq!(
            shared.unauthenticated.load(Ordering::Acquire),
            0,
            "it never took a place"
        );

        peers.set_foreign(false);
        let relay = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write_frame(&mut &relay, &hello_body(RELAY_TOKEN, 4242)).unwrap();
        let deadline = Instant::now() + HELLO_TIMEOUT;
        while shared.lock().hub.snapshot().0.is_empty() {
            assert!(Instant::now() < deadline, "the relay of this user was not connected");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(relay);
    }

    #[test]
    fn an_upload_closed_with_an_error_fails_the_recording_with_its_reason() {
        let upload = upload();
        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        page.close(Some(CloseFrame {
            code: CloseCode::Error,
            reason: "Recording failed (NotReadableError)".into(),
        }))
        .unwrap();
        while page.read().is_ok() {}
        upload.thread.join().unwrap();

        assert_eq!(
            phase(&upload.shared),
            Some(Phase::Failed("Recording failed (NotReadableError)".into()))
        );
    }

    #[test]
    fn the_upload_is_closed_when_the_recording_ends_on_this_side() {
        let upload = upload();
        let mut page = connect(upload.port, &extension_web_origin()).unwrap();
        page.send(hello(TOKEN)).unwrap();
        while phase(&upload.shared) != Some(Phase::Recording) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(upload.shared.lock().hub.is_capturing(upload.id));
        upload.shared.lock().hub.end_capture();
        page.send(Message::binary(b"late".to_vec())).unwrap();

        upload.thread.join().unwrap();
        assert!(upload.sink.0.lock().unwrap().is_empty());
    }

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
