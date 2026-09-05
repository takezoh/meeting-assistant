//! The Phase 1 composition root and diagnostic harness (contract-diagnostic-session-harness).
//!
//! `ma-engine` is L5 and may depend on every lower layer, so this is where the collectors, the
//! capture sources, the extension channel and the detector are wired together. Service identifiers
//! come from the four adapter tables, whose crates are renamed in `Cargo.toml` so that no such token
//! appears in this crate's source. Nothing here starts a collector or opens a capture source unless
//! the `record` command asks for it explicitly (NFR-105).

pub mod session;

use ma_detect::AdapterTable;
use ma_ext_channel::auth::AclApplier;
use ma_ext_channel::{Request, Response, Server};
use ma_secure::acl::SecurityDescriptor;
use ma_signal::adapter::TableAdapter;
use ma_signal::{FixtureSource, SignalTimeline, TimelineHeader};
use ma_signals_windows::{ProcessRecord, TargetApplications};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use session::{label_timeline, DiagnosticSession};

/// Version stamped on timelines and passed to the adapter table.
pub const ADAPTER_TABLE_VERSION: u32 = 1;

/// How long the listener waits for a slow or silent client before giving up on one request.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(50);
/// Bounds on one request so a local client cannot make the harness allocate freely.
const MAX_BODY: usize = 4096;
const MAX_HEADER_LINE: usize = 1024;
const MAX_HEADERS: usize = 32;

/// The four service adapter tables, loaded through their renamed crates.
pub struct AdapterTables {
    adapters: Vec<TableAdapter>,
}

impl AdapterTables {
    pub fn load() -> Self {
        Self {
            adapters: vec![
                adapter_a::adapter(),
                adapter_b::adapter(),
                adapter_c::adapter(),
                adapter_d::adapter(),
            ],
        }
    }

    /// A table built from an explicit list (tests).
    pub fn from_adapters(adapters: Vec<TableAdapter>) -> Self {
        Self { adapters }
    }

    /// The synthetic adapters the committed fixtures are redacted against
    /// (contract-replayable-timeline-fixtures): the same ids, weights and corroboration as the
    /// product tables' classes, with the fixture identifiers documented in
    /// `crates/ma-signal/tests/phase1_fixture_shape.rs`. Replaying a committed fixture against the
    /// product tables cannot detect anything by design (the real identifiers are redacted out);
    /// replaying against these tables reproduces the committed decisions sidecars.
    pub fn synthetic_fixture_tables() -> Self {
        fn desktop(id: &str, image: &str) -> TableAdapter {
            TableAdapter::from_toml(&format!(
                "id = \"{id}\"\nclass = \"desktop\"\nevidence_weight = 2\ncorroboration = {{ microphone = true, tab = false }}\nprocess_images = [\"{image}\"]\n"
            ))
            .expect("synthetic desktop table parses")
        }
        let browser = TableAdapter::from_toml(
            "id = \"browser-x\"\nclass = \"browser\"\nevidence_weight = 1\ncorroboration = { microphone = true, tab = true }\nbrowser_images = [\"example-browser.exe\"]\ntab_hosts = [\"meet.example.test\"]\n",
        )
        .expect("synthetic browser table parses");
        Self {
            adapters: vec![
                desktop("desk-a", "example-desk.exe"),
                desktop("desk-b", "example-other.exe"),
                desktop("desk-c", "example-desk-c.exe"),
                browser,
            ],
        }
    }

    pub fn ids(&self) -> Vec<String> {
        self.adapters.iter().map(|a| a.spec().id.clone()).collect()
    }

    /// The process identifiers the Windows collectors match on: the services' own images and
    /// package family names, plus the browser images whose microphone use corroborates a tab.
    pub fn target_applications(&self) -> TargetApplications {
        let mut targets = TargetApplications::default();
        for a in &self.adapters {
            let spec = a.spec();
            for image in spec.process_images.iter().chain(spec.browser_images.iter()) {
                if !targets
                    .image_names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(image))
                {
                    targets.image_names.push(image.clone());
                }
            }
            for pkg in &spec.package_family_names {
                if !targets.package_family_names.contains(pkg) {
                    targets.package_family_names.push(pkg.clone());
                }
            }
        }
        targets
    }

    /// The meeting hostnames the extension is provisioned with.
    pub fn meeting_hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = self
            .adapters
            .iter()
            .flat_map(|a| a.spec().tab_hosts.iter().cloned())
            .collect();
        hosts.sort();
        hosts.dedup();
        hosts
    }

    /// The detector's adapter table with every service registered.
    pub fn detector_table(&self) -> AdapterTable {
        let mut table = AdapterTable::new(ADAPTER_TABLE_VERSION);
        for a in &self.adapters {
            table.register(Box::new(a.clone()));
        }
        table
    }
}

/// The header every diagnostic session and replayed timeline carries.
pub fn timeline_header(created: &str) -> TimelineHeader {
    TimelineHeader {
        schema_version: ma_signal::SCHEMA_VERSION,
        adapter_table_version: ADAPTER_TABLE_VERSION,
        machine_profile: "redacted".to_string(),
        created: created.to_string(),
    }
}

/// The offline replay path: the only caller of `SignalTimeline::merge`, which drains a source to
/// exhaustion and therefore fits a recorded fixture and never a live collector.
pub fn replay(
    timeline_path: &Path,
    tables: &AdapterTables,
) -> std::io::Result<ma_detect::DetectorOutput> {
    let text = std::fs::read_to_string(timeline_path)?;
    let recorded = SignalTimeline::from_jsonl(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let header = timeline_header(&recorded_created(&text));
    let mut fixture = FixtureSource::new("replay", recorded.signals().to_vec());
    let merged = SignalTimeline::merge(header, &mut [&mut fixture]);
    let mut table = tables.detector_table();
    Ok(ma_detect::decide(
        &merged,
        &ma_detect::DetectorConfig::default(),
        &mut table,
    ))
}

fn recorded_created(text: &str) -> String {
    text.lines()
        .next()
        .and_then(|l| serde_json::from_str::<TimelineHeader>(l).ok())
        .map(|h| h.created)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Writes the generated `endpoint.js` the unpacked extension imports
/// (adr-20260904-extension-endpoint-provisioning-poc) and applies the same owner-only descriptor
/// `endpoint.json` carries, through the injected applier, before the path is returned: the token
/// lives in this file too, so it gets the same protection (NFR-103).
pub fn provision_extension(
    extension_dir: &Path,
    port: u16,
    token_hex: &str,
    meeting_hosts: &[String],
    owner_sid: &str,
    applier: &mut dyn AclApplier,
) -> std::io::Result<PathBuf> {
    let path = extension_dir.join("endpoint.js");
    let hosts = serde_json::to_string(meeting_hosts).expect("hosts serialize");
    let body = format!(
        "// Generated by ma-diag at engine start. Untracked build output; do not commit.\nexport const ENDPOINT = {{\n  port: {port},\n  token: {},\n  meeting_hosts: {hosts},\n}};\n",
        serde_json::to_string(token_hex).expect("token serializes")
    );
    std::fs::write(&path, body)?;
    let security = SecurityDescriptor::owner_only(owner_sid);
    applier.apply(&path, &security)?;
    Ok(path)
}

/// The topmost ancestor with the same image name inside a process snapshot: the process-tree
/// root the detector joins tab and microphone facts on.
pub fn process_tree_root(snapshot: &[ProcessRecord], pid: u32) -> Option<u32> {
    let record = snapshot.iter().find(|r| r.pid == pid)?;
    let mut current = record;
    let mut hops = 0;
    while let Some(parent) = snapshot.iter().find(|r| r.pid == current.parent_pid) {
        if parent.pid == current.pid
            || !parent.image_name.eq_ignore_ascii_case(&record.image_name)
            || hops > 64
        {
            break;
        }
        current = parent;
        hops += 1;
    }
    Some(current.pid)
}

/// Resolves the process that owns the peer end of a loopback connection. The live implementation
/// is Windows-only; a resolver that cannot attribute the peer returns `None` and the tab signals
/// carry no process-tree root, which the detector treats as inconclusive rather than as a match.
pub trait PeerResolver {
    fn peer_pid(&mut self, peer_port: u16) -> Option<u32>;
}

/// Never attributes a peer.
#[derive(Debug, Default)]
pub struct NoPeerResolver;

impl PeerResolver for NoPeerResolver {
    fn peer_pid(&mut self, _peer_port: u16) -> Option<u32> {
        None
    }
}

/// `GetExtendedTcpTable` over the IPv4 table: the owning pid of the connection whose local port is
/// the peer's ephemeral port.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsPeerResolver;

#[cfg(windows)]
impl PeerResolver for WindowsPeerResolver {
    fn peer_pid(&mut self, peer_port: u16) -> Option<u32> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS,
        };
        use windows::Win32::Networking::WinSock::AF_INET;
        let mut size: u32 = 0;
        // SAFETY: size query with a null buffer.
        unsafe {
            GetExtendedTcpTable(
                None,
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            );
        }
        let mut buf = vec![0u8; size as usize];
        // SAFETY: buffer of the size the first call reported.
        let rc = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            )
        };
        if rc != 0 || buf.len() < std::mem::size_of::<MIB_TCPTABLE_OWNER_PID>() {
            return None;
        }
        // SAFETY: the buffer holds a MIB_TCPTABLE_OWNER_PID followed by `dwNumEntries` rows.
        let table = unsafe { &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
        let rows = unsafe {
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        rows.iter()
            .find(|row| u16::from_be((row.dwLocalPort & 0xffff) as u16) == peer_port)
            .map(|row| row.dwOwningPid)
    }
}

/// A minimal loopback HTTP listener for the extension channel: status-only responses, the
/// contract's headers (`Origin`, `X-MA-Token`), and the peer resolved to a process-tree root.
pub struct LoopbackListener {
    listener: TcpListener,
    port: u16,
    connections: u32,
}

impl LoopbackListener {
    /// Binds an ephemeral port on 127.0.0.1.
    pub fn bind() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            port,
            connections: 0,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.listener.set_nonblocking(nonblocking)
    }

    /// Accepts one connection if one is pending and feeds it to the server. Returns whether a
    /// request was handled. The accepted stream is switched to blocking reads with a timeout, so
    /// a non-blocking listener never yields spurious 400s and a silent client cannot hang the
    /// harness.
    pub fn poll_once<C: ma_ext_channel::Clock>(
        &mut self,
        server: &mut Server<C>,
        resolver: &mut dyn PeerResolver,
        snapshot: &[ProcessRecord],
    ) -> std::io::Result<bool> {
        let (stream, peer) = match self.listener.accept() {
            Ok(x) => x,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) => return Err(e),
        };
        self.connections += 1;
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
        stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
        let peer_root = resolver
            .peer_pid(peer.port())
            .and_then(|pid| process_tree_root(snapshot, pid).or(Some(pid)));
        let response = match parse_request(&stream, self.connections, peer_root) {
            Some(request) => server.handle(request),
            None => Response { status: 400 },
        };
        write_response(stream, response)
    }
}

/// Reads one newline-terminated line with a length bound; a longer line is malformed.
fn read_bounded_line(reader: &mut BufReader<&TcpStream>) -> Option<String> {
    let mut buf = Vec::new();
    let read = reader
        .by_ref()
        .take((MAX_HEADER_LINE + 2) as u64)
        .read_until(b'\n', &mut buf)
        .ok()?;
    if read == 0 || !buf.ends_with(b"\n") {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Parses `POST /report` with `Origin` and `X-MA-Token` headers and a JSON body. Anything else is
/// malformed. Header lines, header count and body are all bounded.
fn parse_request(
    stream: &TcpStream,
    connection_id: u32,
    peer_root: Option<u32>,
) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let line = read_bounded_line(&mut reader)?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "POST" || parts.next()? != "/report" {
        return None;
    }
    let mut origin = None;
    let mut token = None;
    let mut length = 0usize;
    for _ in 0..MAX_HEADERS {
        let header = read_bounded_line(&mut reader)?;
        let header = header.trim_end();
        if header.is_empty() {
            if length > MAX_BODY {
                return None;
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).ok()?;
            return Some(Request {
                connection_id,
                origin,
                token,
                body,
                peer_process_tree_root_pid: peer_root,
            });
        }
        let (name, value) = header.split_once(':')?;
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "origin" => origin = Some(value.to_string()),
            "x-ma-token" => token = Some(value.to_string()),
            "content-length" => length = value.parse().ok()?,
            _ => {}
        }
    }
    None
}

fn write_response(mut stream: TcpStream, response: Response) -> std::io::Result<bool> {
    let reason = match response.status {
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        409 => "Conflict",
        429 => "Too Many Requests",
        _ => "Status",
    };
    stream.write_all(
        format!(
            "HTTP/1.1 {} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            response.status
        )
        .as_bytes(),
    )?;
    stream.flush()?;
    Ok(true)
}

/// The commands `ma-diag` accepts. Everything but `Record` is read-only with respect to
/// collectors and capture sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// No subcommand: print usage, touch nothing.
    Usage,
    /// List the adapter tables' ids, target identifiers and meeting hosts.
    List,
    /// Start a live diagnostic session under `artifact_root`, provisioning `extension_dir` when given.
    Record {
        artifact_root: PathBuf,
        extension_dir: Option<PathBuf>,
        max_rounds: Option<u64>,
    },
    /// Attach a `was_meeting` label to a timeline's sidecar.
    Label {
        timeline: PathBuf,
        from_ns: u64,
        to_ns: u64,
        was_meeting: bool,
        note: String,
    },
    /// Replay a recorded timeline through the detector and print the decisions.
    Replay {
        timeline: PathBuf,
        synthetic_tables: bool,
    },
    /// Compute the echo return loss between a loopback and a microphone track directory.
    MeasureLeak {
        loopback_track: PathBuf,
        mic_track: PathBuf,
        application: String,
        alignment_uncertainty_ms: u32,
        out: Option<PathBuf>,
    },
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Command, String> {
        let Some(sub) = args.first() else {
            return Ok(Command::Usage);
        };
        let value = |flag: &str| -> Option<String> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let flag = |name: &str| args.iter().any(|a| a == name);
        let int = |flag: &str| -> Result<Option<u64>, String> {
            value(flag)
                .map(|v| v.parse::<u64>().map_err(|e| format!("{flag}: {e}")))
                .transpose()
        };
        match sub.as_str() {
            "list" => Ok(Command::List),
            "record" => Ok(Command::Record {
                artifact_root: PathBuf::from(
                    value("--artifact-root").ok_or("record needs --artifact-root DIR")?,
                ),
                extension_dir: value("--extension-dir").map(PathBuf::from),
                max_rounds: int("--max-rounds")?,
            }),
            "label" => Ok(Command::Label {
                timeline: PathBuf::from(value("--timeline").ok_or("label needs --timeline FILE")?),
                from_ns: int("--from-ns")?.ok_or("label needs --from-ns")?,
                to_ns: int("--to-ns")?.ok_or("label needs --to-ns")?,
                was_meeting: match value("--was-meeting").as_deref() {
                    Some("true") => true,
                    Some("false") => false,
                    _ => return Err("label needs --was-meeting true|false".into()),
                },
                note: value("--note").unwrap_or_default(),
            }),
            "replay" => Ok(Command::Replay {
                timeline: PathBuf::from(value("--timeline").ok_or("replay needs --timeline FILE")?),
                synthetic_tables: flag("--synthetic-tables"),
            }),
            "measure-leak" => Ok(Command::MeasureLeak {
                loopback_track: PathBuf::from(
                    value("--loopback-track").ok_or("measure-leak needs --loopback-track DIR")?,
                ),
                mic_track: PathBuf::from(
                    value("--mic-track").ok_or("measure-leak needs --mic-track DIR")?,
                ),
                application: value("--application").ok_or("measure-leak needs --application ID")?,
                alignment_uncertainty_ms: int("--alignment-uncertainty-ms")?
                    .ok_or("measure-leak needs --alignment-uncertainty-ms N")?
                    .try_into()
                    .map_err(|_| "--alignment-uncertainty-ms is too large")?,
                out: value("--out").map(PathBuf::from),
            }),
            other => Err(format!("unknown subcommand {other}")),
        }
    }
}

/// The echo-return-loss measurement over two recorded track directories: the chunks are decoded
/// in sequence order, the origins come from the manifests, and the result is the per-application
/// record `contract-echo-leak-measurement` defines.
pub fn measure_leak(
    loopback_track: &Path,
    mic_track: &Path,
    application: &str,
    alignment_uncertainty_ms: u32,
) -> std::io::Result<ma_capture::wasapi::LeakMeasurementRecord> {
    fn load(track: &Path) -> std::io::Result<(u64, Vec<i16>, bool)> {
        let manifest = ma_capture::ChunkManifest::load(track)
            .map_err(|e| std::io::Error::other(format!("{}: {e:?}", track.display())))?
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{}: no chunk manifest", track.display()),
                )
            })?;
        let mut samples = Vec::new();
        let mut chunks = manifest.chunks.clone();
        chunks.sort_by_key(|c| c.seq);
        for chunk in chunks {
            let bytes = std::fs::read(track.join(format!("{:06}.wav", chunk.seq)))?;
            let decoded = ma_capture::wav::decode(&bytes)
                .map_err(|e| std::io::Error::other(format!("chunk {}: {e:?}", chunk.seq)))?;
            let expected = chunk.start_sample as usize;
            if samples.len() != expected {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: non-contiguous chunks", track.display()),
                ));
            }
            samples.extend(decoded.samples);
        }
        Ok((
            manifest.origin.start_monotonic_ns,
            samples,
            !manifest.gaps.is_empty(),
        ))
    }
    let (loop_origin, loop_samples, loop_has_gaps) = load(loopback_track)?;
    let (mic_origin, mic_samples, mic_has_gaps) = load(mic_track)?;
    if loop_has_gaps || mic_has_gaps {
        return Ok(ma_capture::wasapi::LeakMeasurementRecord {
            schema_version: ma_capture::wasapi::RECORD_SCHEMA_VERSION,
            application_id: application.to_string(),
            window_seconds: ma_capture::wasapi::WINDOW_SECONDS,
            outcome: ma_capture::wasapi::LeakOutcome::NoQualifyingWindow,
        });
    }
    Ok(ma_capture::wasapi::measure_echo_return_loss(
        application,
        &ma_capture::wasapi::TrackSamples {
            start_monotonic_ns: loop_origin,
            samples: &loop_samples,
        },
        &ma_capture::wasapi::TrackSamples {
            start_monotonic_ns: mic_origin,
            samples: &mic_samples,
        },
        alignment_uncertainty_ms,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_ext_channel::auth::RecordingApplier;
    use ma_signal::adapter::TableAdapter;

    fn table(id: &str, image: &str, host: &str) -> TableAdapter {
        TableAdapter::from_toml(&format!(
            r#"
id = "{id}"
class = "browser"
evidence_weight = 1
corroboration = {{ microphone = true, tab = true }}
browser_images = ["{image}"]
tab_hosts = ["{host}"]
"#
        ))
        .unwrap()
    }

    #[test]
    fn tables_feed_targets_hosts_and_the_detector_table() {
        let tables = AdapterTables::from_adapters(vec![
            table("svc-a", "example-browser.exe", "meet.example.test"),
            table("svc-b", "Example-Browser.exe", "call.example.test"),
        ]);
        let targets = tables.target_applications();
        assert_eq!(targets.image_names, vec!["example-browser.exe".to_string()]);
        assert_eq!(
            tables.meeting_hosts(),
            vec![
                "call.example.test".to_string(),
                "meet.example.test".to_string()
            ]
        );
        assert_eq!(tables.ids(), vec!["svc-a", "svc-b"]);
        let table = tables.detector_table();
        assert_eq!(table.version(), ADAPTER_TABLE_VERSION);
        assert!(table.adapter("svc-a").is_some() && table.adapter("svc-b").is_some());
    }

    #[test]
    fn the_real_tables_load_without_naming_a_service_here() {
        let tables = AdapterTables::load();
        assert_eq!(tables.ids().len(), 4);
        assert!(!tables.target_applications().is_empty());
        assert!(!tables.meeting_hosts().is_empty());
    }

    /// The committed decisions sidecars are what the harness replay path produces over the
    /// committed timelines against the synthetic fixture tables, byte for byte.
    #[test]
    fn replay_reproduces_the_committed_sidecars_with_the_synthetic_tables() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/signal-timelines");
        let tables = AdapterTables::synthetic_fixture_tables();
        for name in [
            "teams-desktop-session",
            "slack-huddle-session",
            "zoom-desktop-session",
            "meet-chrome-with-extension",
            "meet-chrome-without-extension",
        ] {
            let output = replay(&dir.join(format!("{name}.jsonl")), &tables).unwrap();
            let committed =
                std::fs::read_to_string(dir.join(format!("{name}.decisions.json"))).unwrap();
            assert_eq!(
                output.to_canonical_json().trim(),
                committed.trim(),
                "{name}: the harness replay reproduces the committed sidecar"
            );
            assert!(!output.decisions.is_empty(), "{name}");
        }
        // The product tables cannot detect a redacted fixture: replaying with them is the wrong
        // oracle, which is why the binary requires --synthetic-tables to be explicit about it.
        let product = replay(
            &dir.join("teams-desktop-session.jsonl"),
            &AdapterTables::load(),
        )
        .unwrap();
        assert!(product
            .decisions
            .iter()
            .all(|d| !d.outcome.is_determinate_start()));
    }

    #[test]
    fn tree_root_follows_same_image_parents() {
        let snap = vec![
            ProcessRecord {
                pid: 4,
                parent_pid: 0,
                image_name: "system".into(),
            },
            ProcessRecord {
                pid: 200,
                parent_pid: 4,
                image_name: "example-browser.exe".into(),
            },
            ProcessRecord {
                pid: 201,
                parent_pid: 200,
                image_name: "example-browser.exe".into(),
            },
        ];
        assert_eq!(process_tree_root(&snap, 201), Some(200));
        assert_eq!(process_tree_root(&snap, 200), Some(200));
        assert_eq!(process_tree_root(&snap, 999), None);
    }

    #[test]
    fn provisioning_writes_port_token_and_hosts_and_applies_the_acl() {
        let dir = tempfile::tempdir().unwrap();
        let mut applier = RecordingApplier::default();
        let path = provision_extension(
            dir.path(),
            49_152,
            &"ab".repeat(32),
            &["meet.example.test".to_string()],
            "S-1-5-21-1-2-3-1001",
            &mut applier,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("port: 49152"));
        assert!(text.contains(&"ab".repeat(32)));
        assert!(text.contains("meet.example.test"));
        assert!(text.starts_with("// Generated"));
        assert_eq!(
            applier.applied.len(),
            1,
            "the token file gets the owner-only descriptor"
        );
        assert_eq!(applier.applied[0].0, path);
    }

    #[test]
    fn loopback_listener_feeds_the_channel_server_with_the_peer_root() {
        use ma_ext_channel::{ServerConfig, SystemClock};
        use std::io::{Read as _, Write as _};
        let ext = "abcdefghijklmnopabcdefghijklmnop";
        let mut server = Server::start(
            &ServerConfig {
                pinned_extension_id: ext.into(),
            },
            SystemClock::default(),
        );
        let token = server.authenticator().token().to_hex();
        let mut listener = LoopbackListener::bind().unwrap();
        listener.set_nonblocking(true).unwrap();
        assert!(!listener
            .poll_once(&mut server, &mut NoPeerResolver, &[])
            .unwrap());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let body = format!(
            r#"{{"instance_id":"inst-a","seq":1,"observed_at_ms":{now_ms},"host":"meet.example.test","tab_key":"tab-17","audible":true,"meeting_present":true}}"#
        );
        let request = format!(
            "POST /report HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: chrome-extension://{ext}\r\nX-MA-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        struct Fixed;
        impl PeerResolver for Fixed {
            fn peer_pid(&mut self, _: u16) -> Option<u32> {
                Some(201)
            }
        }
        let snapshot = vec![
            ProcessRecord {
                pid: 200,
                parent_pid: 4,
                image_name: "example-browser.exe".into(),
            },
            ProcessRecord {
                pid: 201,
                parent_pid: 200,
                image_name: "example-browser.exe".into(),
            },
        ];
        let serve = |listener: &mut LoopbackListener, server: &mut Server<SystemClock>| {
            for _ in 0..400 {
                if listener.poll_once(server, &mut Fixed, &snapshot).unwrap() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        };
        let mut client = TcpStream::connect(("127.0.0.1", listener.port())).unwrap();
        client.write_all(request.as_bytes()).unwrap();
        assert!(serve(&mut listener, &mut server));
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 204"), "{reply}");
        let signals = server.drain();
        assert_eq!(signals.len(), 2);
        assert!(signals
            .iter()
            .all(|s| s.payload.process_tree_root_pid == Some(200)));

        // A wrong token is a 401 and yields no signal.
        let mut client = TcpStream::connect(("127.0.0.1", listener.port())).unwrap();
        client
            .write_all(request.replace(&token, &"00".repeat(32)).as_bytes())
            .unwrap();
        assert!(serve(&mut listener, &mut server));
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
        assert!(server.drain().is_empty());

        // A client that sends nothing is cut off by the read timeout (400), never waited on forever.
        let started = std::time::Instant::now();
        let mut silent = TcpStream::connect(("127.0.0.1", listener.port())).unwrap();
        assert!(serve(&mut listener, &mut server));
        let mut reply = String::new();
        let _ = silent.read_to_string(&mut reply);
        assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
        assert!(started.elapsed() < Duration::from_secs(10));

        // An oversized header line is malformed.
        let mut client = TcpStream::connect(("127.0.0.1", listener.port())).unwrap();
        let huge = format!(
            "POST /report HTTP/1.1\r\nX-Junk: {}\r\n\r\n",
            "a".repeat(5000)
        );
        client.write_all(huge.as_bytes()).unwrap();
        assert!(serve(&mut listener, &mut server));
        let mut reply = String::new();
        let _ = client.read_to_string(&mut reply);
        assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
    }

    #[test]
    fn command_parsing_defaults_to_usage() {
        assert_eq!(Command::parse(&[]).unwrap(), Command::Usage);
        assert_eq!(Command::parse(&["list".into()]).unwrap(), Command::List);
        assert!(Command::parse(&["record".into()]).is_err());
        assert!(Command::parse(&["explode".into()]).is_err());
        let replay = Command::parse(&[
            "replay".into(),
            "--timeline".into(),
            "t.jsonl".into(),
            "--synthetic-tables".into(),
        ])
        .unwrap();
        assert!(matches!(
            replay,
            Command::Replay {
                synthetic_tables: true,
                ..
            }
        ));
        assert!(Command::parse(&["measure-leak".into()]).is_err());
    }

    #[test]
    fn measure_leak_reads_two_track_directories() {
        use ma_capture::{
            CaptureSource, ChunkWriter, RealFs, SourceEvent, SyntheticSource, SAMPLE_RATE,
        };
        use ma_core_types::id::TypedId;
        use ma_core_types::TrackId;
        let dir = tempfile::tempdir().unwrap();
        let mut fs = RealFs;
        for role in ["loopback", "mic"] {
            let mut source = SyntheticSource::new(SAMPLE_RATE, 65 * SAMPLE_RATE as u64, 16_000);
            let mut writer = ChunkWriter::open(
                &dir.path().join(role),
                TrackId::new(),
                role,
                source.origin(),
            )
            .unwrap();
            loop {
                match source.next() {
                    SourceEvent::Samples(s) => {
                        writer.push(&s);
                        writer.drain(&mut fs).unwrap();
                    }
                    SourceEvent::Ended => break,
                    other => panic!("{other:?}"),
                }
            }
            if role == "mic" {
                writer.record_capture_gap(60 * SAMPLE_RATE as u64);
            }
            writer.finish(&mut fs).unwrap();
        }
        let record = measure_leak(
            &dir.path().join("loopback"),
            &dir.path().join("mic"),
            "example",
            10,
        )
        .unwrap();
        assert_eq!(record.application_id, "example");
        // A manifest gap is missing observation, never synthesized microphone silence.
        assert_eq!(
            record.outcome,
            ma_capture::wasapi::LeakOutcome::NoQualifyingWindow
        );
        assert!(
            measure_leak(&dir.path().join("missing"), &dir.path().join("mic"), "x", 0).is_err()
        );
    }
}
