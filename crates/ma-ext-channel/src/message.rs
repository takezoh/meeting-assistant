//! The message an extension may post. Unknown fields are rejected, so a URL or a page title cannot
//! ride along; `contracts/extension-channel/message.schema.json` is the same shape as JSON Schema.

use serde::{Deserialize, Serialize};

pub const MAX_HOST_LEN: usize = 253;
pub const MAX_TAB_KEY_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionMessage {
    /// The extension instance (a browser session); `seq` is strictly increasing per instance.
    pub instance_id: String,
    /// Per-instance strictly increasing sequence.
    pub seq: u64,
    /// The extension's observation time, Unix milliseconds; older than the freshness window is dropped.
    pub observed_at_ms: i64,
    /// Hostname only. Never a full URL.
    pub host: String,
    /// Opaque per-tab key chosen by the extension; never a title.
    pub tab_key: String,
    pub audible: bool,
    pub meeting_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    NotJson,
    HostNotAHostname,
    TabKeyTooLong,
    InstanceIdInvalid,
}

impl ExtensionMessage {
    /// Parse and validate a request body. The parse error payload is never retained.
    pub fn parse(body: &[u8]) -> Result<ExtensionMessage, MessageError> {
        let message: ExtensionMessage =
            serde_json::from_slice(body).map_err(|_| MessageError::NotJson)?;
        if !is_hostname(&message.host) {
            return Err(MessageError::HostNotAHostname);
        }
        if message.tab_key.is_empty() || message.tab_key.len() > MAX_TAB_KEY_LEN {
            return Err(MessageError::TabKeyTooLong);
        }
        if message.instance_id.is_empty()
            || message.instance_id.len() > MAX_TAB_KEY_LEN
            || !message
                .instance_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(MessageError::InstanceIdInvalid);
        }
        Ok(message)
    }
}

fn is_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && !host.contains("://")
        && !host.contains('/')
        && !host.contains('?')
        && !host.contains('#')
        && host.split('.').all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_or_a_title_is_not_a_message() {
        let with_url = br#"{"instance_id":"i1","seq":1,"observed_at_ms":1,"host":"https://meet.example.test/abc-defg","tab_key":"t1","audible":true,"meeting_present":true}"#;
        assert_eq!(
            ExtensionMessage::parse(with_url),
            Err(MessageError::HostNotAHostname)
        );
        let with_title = br#"{"instance_id":"i1","seq":1,"observed_at_ms":1,"host":"meet.example.test","tab_key":"t1","audible":true,"meeting_present":true,"title":"Budget review"}"#;
        assert_eq!(
            ExtensionMessage::parse(with_title),
            Err(MessageError::NotJson)
        );
    }

    #[test]
    fn schema_matches_struct() {
        let schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/extension-channel/message.schema.json"
        ))
        .unwrap();
        let props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            props,
            [
                "audible",
                "host",
                "instance_id",
                "meeting_present",
                "observed_at_ms",
                "seq",
                "tab_key"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);
    }
}
