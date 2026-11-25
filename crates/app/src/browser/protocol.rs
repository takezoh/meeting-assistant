//! Messages exchanged with the extension and how they are framed. Independent of where I/O goes.
//!
//! Control messages travel through the browser's native messaging, where one message is a 4-byte length followed by
//! JSON. The recording itself travels as binary WebSocket messages on a separate upload connection (see
//! [`UploadHello`]), so it is neither base64-encoded nor wrapped in JSON.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Name of the relay process, as registered with the browser.
pub const HOST_NAME: &str = "com.meeting_assistant.host";
/// Extension ID. Derived from the key embedded in the extension's manifest.
pub const EXTENSION_ID: &str = "lnplioaidiniokndppmckdaflgefgale";

/// Maximum size of one message (same as the limit the browser accepts from an extension).
pub const MAX_FRAME: usize = 64 * 1024 * 1024;
/// Maximum size of the first message of a connection. Nothing has proved who the peer is yet,
/// so it is not allowed to make the app reserve more than a `hello` needs.
pub const MAX_HELLO_FRAME: usize = 4 * 1024;

/// Browser launch option that allows this extension alone to record a tab without a user gesture.
pub fn browser_launch_option() -> String {
    format!("--allowlisted-extension-id={EXTENSION_ID}")
}

/// String identifying the extension's origin. The browser passes it as an argument when launching the relay process.
pub fn extension_origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}

/// The `Origin` header the extension's pages send when they open a WebSocket.
pub fn extension_web_origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}")
}

/// Messages arriving from the extension (and the relay process).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Incoming {
    /// Sent first by the relay process.
    Hello { token: String, host_pid: u32 },
    /// A meeting started or ended in a tab. `service` and `metadata` come with a start.
    Meeting {
        tab: u64,
        active: bool,
        #[serde(default)]
        service: String,
        /// What the extension tells of the meeting, which differs per service, as `[key, value]` pairs in order; the
        /// first tells it apart from others of the same service (a Google Meet code, or the tab's title).
        #[serde(default)]
        metadata: Vec<(String, String)>,
    },
    /// Recording a tab could not start.
    Error {
        #[serde(default)]
        tab: Option<u64>,
        message: String,
    },
}

impl Incoming {
    pub fn parse(body: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(body)
    }
}

/// A size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PictureSize {
    pub width: u32,
    pub height: u32,
}

/// Commands sent to the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outgoing {
    /// Record `tab` and send the recording to the WebSocket at `upload`, presenting `token` first. The picture is the
    /// tab's size in device pixels, scaled down to fit in `max_size` with its aspect ratio kept.
    Start {
        tab: u64,
        upload: String,
        token: String,
        max_size: PictureSize,
    },
    Stop {
        tab: u64,
    },
}

/// The first message on an upload connection, as text: proves the peer was told about this recording, and says
/// which container it records in. The extension sends it as it starts recording, so the app takes its arrival as the
/// recording's time zero. Binary messages with the recording follow, in order; nothing else is sent. A normal close
/// ends the recording; a close with another code fails it, with the reason given.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadHello {
    pub token: String,
    pub mime: String,
}

/// WebSocket close code for a recording that ended normally.
pub const CLOSE_NORMAL: u16 = 1000;

/// Whether an upload's WebSocket handshake comes from the extension's pages, judged by its `Origin` header.
pub fn is_extension_origin(origin: Option<&[u8]>) -> bool {
    origin == Some(extension_web_origin().as_bytes())
}

/// Checks the first message of an upload. Returns the container it announces when it carries `token`.
pub fn accept_upload_hello(first: Option<&str>, token: &str) -> Result<String, String> {
    let text = first.ok_or("the first message is not text")?;
    let hello: UploadHello = serde_json::from_str(text).map_err(|_| "the first message is not a hello")?;
    if !tokens_match(&hello.token, token) {
        return Err("token mismatch".to_owned());
    }
    Ok(hello.mime)
}

/// What the close of an upload means: the recording ended normally, or it failed for the reason given.
/// `close` is the close code and reason, `None` for a close without a code.
pub fn upload_close_outcome(close: Option<(u16, &str)>) -> Result<(), String> {
    match close {
        None | Some((CLOSE_NORMAL, _)) => Ok(()),
        Some((code, "")) => Err(format!("The recording ended abnormally (close code {code})")),
        Some((_, reason)) => Err(reason.to_owned()),
    }
}

impl Outgoing {
    pub fn to_body(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("commands can always be serialized to JSON")
    }
}

/// The message the relay process sends first.
pub fn hello_body(token: &str, host_pid: u32) -> Vec<u8> {
    serde_json::json!({ "type": "hello", "token": token, "host_pid": host_pid })
        .to_string()
        .into_bytes()
}

/// Whether the token a peer presented is the expected one.
///
/// Every byte is compared whatever the outcome, so the time taken does not reveal how much of a guess was right.
pub fn tokens_match(presented: &str, expected: &str) -> bool {
    let (presented, expected) = (presented.as_bytes(), expected.as_bytes());
    let difference = presented
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b));
    presented.len() == expected.len() && difference == 0
}

pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    read_frame_within(reader, MAX_FRAME)
}

/// Reads one message, rejecting a declared length over `limit` before reserving memory for it.
pub fn read_frame_within(reader: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_ne_bytes(length) as usize;
    if length > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message is too large"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(body)
}

pub fn write_frame(writer: &mut impl Write, body: &[u8]) -> io::Result<()> {
    writer.write_all(&(body.len() as u32).to_ne_bytes())?;
    writer.write_all(body)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn written_messages_read_back_identically() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"first").unwrap();
        write_frame(&mut wire, "second".as_bytes()).unwrap();

        let mut reader = Cursor::new(wire);
        assert_eq!(read_frame(&mut reader).unwrap(), b"first");
        assert_eq!(read_frame(&mut reader).unwrap(), "second".as_bytes());
        assert!(read_frame(&mut reader).is_err(), "reading at end of stream is an error");
    }

    #[test]
    fn truncated_message_is_an_error() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"truncated").unwrap();
        wire.truncate(wire.len() - 3);

        assert!(read_frame(&mut Cursor::new(wire)).is_err());
    }

    #[test]
    fn length_over_limit_is_rejected_without_reading_body() {
        let wire = ((MAX_FRAME + 1) as u32).to_ne_bytes().to_vec();

        let error = read_frame(&mut Cursor::new(wire)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn first_message_is_held_to_a_smaller_limit() {
        let mut wire = Vec::new();
        write_frame(&mut wire, &hello_body("token", 42)).unwrap();
        assert!(read_frame_within(&mut Cursor::new(wire), MAX_HELLO_FRAME).is_ok());

        // Only the length is sent: a peer must not be able to make the app reserve the declared size.
        let oversized = ((MAX_HELLO_FRAME + 1) as u32).to_ne_bytes().to_vec();
        let error = read_frame_within(&mut Cursor::new(oversized), MAX_HELLO_FRAME).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn only_the_exact_token_matches() {
        assert!(tokens_match("abc123", "abc123"));
        assert!(!tokens_match("abc124", "abc123"));
        assert!(!tokens_match("abc12", "abc123"), "a prefix is not a match");
        assert!(!tokens_match("abc1234", "abc123"));
        assert!(!tokens_match("", "abc123"));
    }

    #[test]
    fn parses_messages_from_the_extension() {
        let parse = |json: &str| Incoming::parse(json.as_bytes()).unwrap();

        assert_eq!(
            parse(
                r#"{"type":"meeting","tab":7,"active":true,"service":"GoogleMeet","metadata":[["meeting_code","abc-defg-hij"],["page_title","Meet - abc-defg-hij"]]}"#
            ),
            Incoming::Meeting {
                tab: 7,
                active: true,
                service: "GoogleMeet".into(),
                metadata: vec![
                    ("meeting_code".into(), "abc-defg-hij".into()),
                    ("page_title".into(), "Meet - abc-defg-hij".into())
                ]
            }
        );
        assert_eq!(
            parse(r#"{"type":"meeting","tab":7,"active":false}"#),
            Incoming::Meeting {
                tab: 7,
                active: false,
                service: String::new(),
                metadata: Vec::new()
            }
        );
        assert_eq!(
            parse(r#"{"type":"error","tab":7,"message":"x"}"#),
            Incoming::Error {
                tab: Some(7),
                message: "x".into()
            }
        );
    }

    #[test]
    fn recording_data_does_not_travel_through_native_messaging() {
        for removed in [
            r#"{"type":"chunk","tab":7,"data":"AAEC"}"#,
            r#"{"type":"started","tab":7,"mime":"video/mp4"}"#,
            r#"{"type":"stopped","tab":7}"#,
        ] {
            assert!(Incoming::parse(removed.as_bytes()).is_err(), "{removed}");
        }
    }

    #[test]
    fn malformed_message_is_an_error() {
        assert!(Incoming::parse(b"not json").is_err());
        assert!(Incoming::parse(br#"{"type":"meeting","tab":"seven"}"#).is_err());
    }

    #[test]
    fn only_the_extensions_origin_may_upload() {
        assert!(is_extension_origin(Some(
            b"chrome-extension://lnplioaidiniokndppmckdaflgefgale"
        )));
        assert!(!is_extension_origin(Some(
            b"chrome-extension://lnplioaidiniokndppmckdaflgefgale/"
        )));
        assert!(!is_extension_origin(Some(b"https://meet.google.com")));
        assert!(!is_extension_origin(None));
    }

    #[test]
    fn the_first_upload_message_must_carry_this_recordings_token() {
        let hello = r#"{"token":"secret","mime":"video/mp4"}"#;
        assert_eq!(accept_upload_hello(Some(hello), "secret"), Ok("video/mp4".into()));
        assert!(accept_upload_hello(Some(hello), "other").is_err());
        assert!(accept_upload_hello(Some(r#"{"mime":"video/mp4"}"#), "secret").is_err());
        assert!(accept_upload_hello(None, "secret").is_err(), "binary first is refused");
    }

    #[test]
    fn a_normal_close_ends_the_recording_and_any_other_fails_it_with_its_reason() {
        assert_eq!(upload_close_outcome(None), Ok(()));
        assert_eq!(upload_close_outcome(Some((1000, ""))), Ok(()));
        assert_eq!(
            upload_close_outcome(Some((1011, "Recording failed (NotReadableError)"))),
            Err("Recording failed (NotReadableError)".into())
        );
        assert_eq!(
            upload_close_outcome(Some((1006, ""))),
            Err("The recording ended abnormally (close code 1006)".into())
        );
    }

    #[test]
    fn the_upload_hello_parses() {
        let hello: UploadHello = serde_json::from_str(r#"{"token":"t","mime":"video/webm;codecs=vp9,opus"}"#).unwrap();
        assert_eq!(hello.mime, "video/webm;codecs=vp9,opus");
        assert!(serde_json::from_str::<UploadHello>(r#"{"mime":"video/mp4"}"#).is_err());
    }

    #[test]
    fn relay_first_message_parses_as_hello() {
        assert_eq!(
            Incoming::parse(&hello_body("secret", 42)).unwrap(),
            Incoming::Hello {
                token: "secret".into(),
                host_pid: 42
            }
        );
    }

    #[test]
    fn commands_serialize_in_a_form_the_extension_reads() {
        let json = |message: Outgoing| String::from_utf8(message.to_body()).unwrap();

        assert_eq!(
            json(Outgoing::Start {
                tab: 7,
                upload: "ws://127.0.0.1:5000/".into(),
                token: "t".into(),
                max_size: PictureSize {
                    width: 1280,
                    height: 720
                }
            }),
            r#"{"type":"start","tab":7,"upload":"ws://127.0.0.1:5000/","token":"t","max_size":{"width":1280,"height":720}}"#
        );
        assert_eq!(json(Outgoing::Stop { tab: 7 }), r#"{"type":"stop","tab":7}"#);
    }

    #[test]
    fn launch_option_and_origin_derive_from_extension_id() {
        assert_eq!(
            browser_launch_option(),
            "--allowlisted-extension-id=lnplioaidiniokndppmckdaflgefgale"
        );
        assert_eq!(
            extension_origin(),
            "chrome-extension://lnplioaidiniokndppmckdaflgefgale/"
        );
        assert_eq!(
            extension_web_origin(),
            "chrome-extension://lnplioaidiniokndppmckdaflgefgale"
        );
    }
}
