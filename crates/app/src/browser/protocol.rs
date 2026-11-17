//! Messages exchanged with the extension and how they are framed. Independent of where I/O goes.
//!
//! Like the browser's native messaging, one message is a 4-byte length followed by JSON.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Name of the relay process, as registered with the browser.
pub const HOST_NAME: &str = "com.meeting_assistant.host";
/// Extension ID. Derived from the key embedded in the extension's manifest.
pub const EXTENSION_ID: &str = "lnplioaidiniokndppmckdaflgefgale";

/// Maximum size of one message (same as the limit the browser accepts from an extension).
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

/// Browser launch option that allows this extension alone to record a tab without a user gesture.
pub fn browser_launch_option() -> String {
    format!("--allowlisted-extension-id={EXTENSION_ID}")
}

/// String identifying the extension's origin. The browser passes it as an argument when launching the relay process.
pub fn extension_origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}/")
}

/// Messages arriving from the extension (and the relay process).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Incoming {
    /// Sent first by the relay process.
    Hello {
        token: String,
        host_pid: u32,
    },
    /// A meeting started or ended in a tab.
    Meeting {
        tab: u64,
        active: bool,
        #[serde(default)]
        service: String,
    },
    Started {
        tab: u64,
        #[serde(default)]
        mime: String,
    },
    /// Next piece of recorded data (base64).
    Chunk {
        tab: u64,
        data: String,
    },
    Stopped {
        tab: u64,
    },
    Error {
        #[serde(default)]
        tab: Option<u64>,
        message: String,
    },
    Warning {
        message: String,
    },
    /// A type this app does not know. Can happen with a newer extension and an older app, so it is ignored.
    #[serde(other)]
    Unknown,
}

impl Incoming {
    pub fn parse(body: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(body)
    }
}

/// Commands sent to the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outgoing {
    Start { tab: u64 },
    Stop { tab: u64 },
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

pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_ne_bytes(length) as usize;
    if length > MAX_FRAME {
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
    fn parses_messages_from_the_extension() {
        let parse = |json: &str| Incoming::parse(json.as_bytes()).unwrap();

        assert_eq!(
            parse(r#"{"type":"meeting","tab":7,"active":true,"service":"GoogleMeet"}"#),
            Incoming::Meeting {
                tab: 7,
                active: true,
                service: "GoogleMeet".into()
            }
        );
        assert_eq!(
            parse(r#"{"type":"meeting","tab":7,"active":false}"#),
            Incoming::Meeting {
                tab: 7,
                active: false,
                service: String::new()
            }
        );
        assert_eq!(
            parse(r#"{"type":"started","tab":7,"mime":"video/mp4"}"#),
            Incoming::Started {
                tab: 7,
                mime: "video/mp4".into()
            }
        );
        assert_eq!(
            parse(r#"{"type":"chunk","tab":7,"data":"AAEC"}"#),
            Incoming::Chunk {
                tab: 7,
                data: "AAEC".into()
            }
        );
        assert_eq!(parse(r#"{"type":"stopped","tab":7}"#), Incoming::Stopped { tab: 7 });
        assert_eq!(
            parse(r#"{"type":"error","message":"x"}"#),
            Incoming::Error {
                tab: None,
                message: "x".into()
            }
        );
        assert_eq!(
            parse(r#"{"type":"warning","message":"w"}"#),
            Incoming::Warning { message: "w".into() }
        );
    }

    #[test]
    fn unknown_message_type_is_received_as_ignorable() {
        assert_eq!(
            Incoming::parse(br#"{"type":"future-feature","x":1}"#).unwrap(),
            Incoming::Unknown
        );
    }

    #[test]
    fn malformed_message_is_an_error() {
        assert!(Incoming::parse(b"not json").is_err());
        assert!(Incoming::parse(br#"{"type":"chunk","tab":"seven"}"#).is_err());
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

        assert_eq!(json(Outgoing::Start { tab: 7 }), r#"{"type":"start","tab":7}"#);
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
    }
}
