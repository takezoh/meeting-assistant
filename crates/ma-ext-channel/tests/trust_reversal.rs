//! contract-extension-trust-reversal-check: the owner-only descriptor is applied, not only built,
//! and tab signals carry the peer's process-tree root so the detector can join them with
//! microphone facts. The two live observations adr-20260903-extension-localhost-channel-trust
//! assigns to Phase 1 (same-user readability of endpoint.json, browser policy reaching the
//! loopback listener) are Windows-tier records; the portable tier asserts the mechanism exists.

use ma_ext_channel::auth::{AclApplier, RecordingApplier};
use ma_ext_channel::{Clock, EndpointDescriptor, Request, Response, Server, ServerConfig};
use ma_signal::SignalKind;

struct FixedClock;
impl Clock for FixedClock {
    fn monotonic_ns(&self) -> u64 {
        1_000_000_000
    }
    fn wall_utc_ms(&self) -> i64 {
        1_756_857_600_000
    }
}

const EXT: &str = "abcdefghijklmnopabcdefghijklmnop";

fn request(server: &Server<FixedClock>, seq: u64, peer: Option<u32>) -> Request {
    Request {
        connection_id: 1,
        origin: Some(format!("chrome-extension://{EXT}")),
        token: Some(server.authenticator().token().to_hex()),
        body: format!(
            r#"{{"instance_id":"inst-a","seq":{seq},"observed_at_ms":1756857600000,"host":"meet.example.test","tab_key":"tab-17","audible":true,"meeting_present":true}}"#
        )
        .into_bytes(),
        peer_process_tree_root_pid: peer,
    }
}

#[test]
fn endpoint_write_applies_the_owner_only_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    let descriptor = EndpointDescriptor {
        port: 49_152,
        token: "ab".repeat(32),
    };
    let mut applier = RecordingApplier::default();
    let (path, security) = descriptor
        .write(dir.path(), "S-1-5-21-1-2-3-1001", &mut applier)
        .unwrap();
    assert!(security.grants_owner_only());
    assert_eq!(
        applier.applied,
        vec![(path.clone(), security.to_sddl())],
        "the descriptor is applied to the written file before the path is returned"
    );
    assert!(path.is_file());
    // A failing applier fails the write: an unprotected token file is never reported as written.
    struct Refusing;
    impl AclApplier for Refusing {
        fn apply(
            &mut self,
            _: &std::path::Path,
            _: &ma_secure::acl::SecurityDescriptor,
        ) -> std::io::Result<()> {
            Err(std::io::Error::other("acl refused"))
        }
    }
    assert!(descriptor
        .write(dir.path(), "S-1-5-21-1-2-3-1001", &mut Refusing)
        .is_err());
    assert!(!EndpointDescriptor::path_under(dir.path()).exists());
}

/// NFR-103(a), Windows tier (`v-win1-endpoint-dacl-readability-observed`, run with `--ignored`):
/// apply the real DACL and observe whether *another* same-user process can read the token file.
/// The observation is made from a separate process (`cmd.exe /c type`), not from the writer, and
/// the test records the observed fact without pre-judging it. A readable result is the ADR's
/// consultation/reversal input, not a reason to falsify the observation test. The SID is derived
/// from the current token, so the declared CI runner needs no undeclared secret.
#[cfg(windows)]
#[test]
#[ignore = "Windows-tier observation; run explicitly with -- --ignored"]
fn endpoint_json_not_readable_by_other_same_user_process() {
    use ma_ext_channel::auth::WindowsAclApplier;
    let dir = tempfile::tempdir().unwrap();
    let descriptor = EndpointDescriptor {
        port: 49_152,
        token: "ab".repeat(32),
    };
    let whoami = std::process::Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .expect("whoami.exe runs");
    assert!(whoami.status.success(), "whoami /user succeeds");
    let row = String::from_utf8_lossy(&whoami.stdout);
    let sid = row
        .trim()
        .split(',')
        .next_back()
        .map(|field| field.trim().trim_matches('"').to_string())
        .filter(|field| field.starts_with("S-1-"))
        .expect("whoami /user returns the current token SID");
    let mut applier = WindowsAclApplier;
    let (path, _) = descriptor
        .write(dir.path(), &sid, &mut applier)
        .expect("the owner-only DACL is applied; an apply failure is a test failure, not a skip");
    // Another process of the same user tries to read the file.
    let status = std::process::Command::new("cmd.exe")
        .args(["/c", "type"])
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("cmd.exe runs");
    let observation_dir = std::env::var_os("MA_WINDOWS_OBSERVATION_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target/windows-observations"));
    std::fs::create_dir_all(&observation_dir).expect("observation directory is writable");
    let observation_path = observation_dir.join("endpoint-json-same-user-readability.json");
    let observation = serde_json::json!({
        "schema_version": 1,
        "endpoint_json_same_user_readable": status.success(),
        "reader": "separate-cmd-process",
    });
    std::fs::write(
        &observation_path,
        serde_json::to_vec_pretty(&observation).unwrap(),
    )
    .expect("readability observation is durable");
    eprintln!(
        "observation written to {}: {observation}",
        observation_path.display()
    );
}

#[test]
fn tab_signals_carry_the_peer_process_tree_root() {
    let mut server = Server::start(
        &ServerConfig {
            pinned_extension_id: EXT.into(),
        },
        FixedClock,
    );
    assert_eq!(
        server.handle(request(&server, 1, Some(4242))),
        Response::ACCEPTED
    );
    let signals = server.drain();
    assert_eq!(
        signals.iter().map(|s| s.kind).collect::<Vec<_>>(),
        [SignalKind::TabMeetingPresent, SignalKind::TabAudible]
    );
    assert!(signals
        .iter()
        .all(|s| s.payload.process_tree_root_pid == Some(4242)));
    // The wire message itself is unchanged: the pid comes from the transport, never from the body.
    assert_eq!(server.handle(request(&server, 2, None)), Response::ACCEPTED);
    assert!(server
        .drain()
        .iter()
        .all(|s| s.payload.process_tree_root_pid.is_none()));
}
