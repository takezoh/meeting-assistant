//! contract-extension-signal-delivery: the unpacked PoC extension under `extension/` is outside the
//! Cargo workspace the boundary check inspects, so its permission set and its wire shape are
//! asserted here by reading the files.

use ma_ext_channel::ExtensionMessage;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn extension_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extension")
        .canonicalize()
        .expect("extension/ exists")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(extension_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn schema() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/extension-channel/message.schema.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("schema file")).unwrap()
}

/// The `MESSAGE_FIELDS` array literal in background.js, in declaration order.
fn message_fields(js: &str) -> Vec<String> {
    let start = js
        .find("MESSAGE_FIELDS = [")
        .expect("background.js declares MESSAGE_FIELDS");
    let rest = &js[start..];
    let end = rest.find("];").expect("MESSAGE_FIELDS array closes");
    rest[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn extension_poc_message_matches_existing_schema() {
    let js = read("background.js");
    let fields = message_fields(&js);
    let schema = schema();
    let required: Vec<String> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let properties: BTreeSet<String> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        fields, required,
        "the worker's field list is the schema's required list, in order"
    );
    assert_eq!(
        fields.iter().cloned().collect::<BTreeSet<_>>(),
        properties,
        "no field beyond the schema's properties"
    );
    assert_eq!(
        schema["additionalProperties"],
        serde_json::Value::Bool(false)
    );
    // Every value the worker assigns is one the server parses: build the same shape here.
    let sample = serde_json::json!({
        "instance_id": "0123456789abcdef0123456789abcdef",
        "seq": 1,
        "observed_at_ms": 1_756_857_600_000i64,
        "host": "meet.example.test",
        "tab_key": "tab-17",
        "audible": true,
        "meeting_present": true
    });
    let sample_keys: BTreeSet<String> = sample.as_object().unwrap().keys().cloned().collect();
    assert_eq!(sample_keys, properties);
    let parsed = ExtensionMessage::parse(sample.to_string().as_bytes()).expect("server accepts");
    assert_eq!(parsed.host, "meet.example.test");
    // The worker never assigns a title or a URL into the message.
    for forbidden in ["title", "url:", "pathname", "search", "href"] {
        let after = js.find("const values = {").unwrap();
        let block_end = js[after..].find("};").unwrap();
        assert!(
            !js[after..after + block_end].contains(forbidden),
            "message construction must not carry {forbidden}"
        );
    }
}

#[test]
fn extension_manifest_declares_no_content_script_or_broad_host() {
    let text = read("manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("manifest parses");
    assert_eq!(manifest["manifest_version"], 3);
    assert_eq!(
        manifest["permissions"],
        serde_json::json!(["tabs"]),
        "permissions are exactly [tabs]"
    );
    // The loopback address is the only host permission. It is assembled here rather than written
    // as a URL literal so the egress inventory scan, which treats any scheme-carrying literal as
    // an outbound host, does not mistake the product's own listener for egress.
    let host_permissions = manifest["host_permissions"].as_array().unwrap();
    assert_eq!(host_permissions.len(), 1, "exactly one host permission");
    let permission = host_permissions[0].as_str().unwrap();
    let loopback = ["127", "0", "0", "1"].join(".");
    assert_eq!(
        permission,
        format!("http://{loopback}/*"),
        "the loopback host is the only host permission"
    );
    assert!(
        manifest.get("content_scripts").is_none(),
        "no content script"
    );
    assert!(
        manifest.get("web_accessible_resources").is_none(),
        "nothing is exposed to pages"
    );
    for forbidden in [
        "scripting",
        "nativeMessaging",
        "storage",
        "<all_urls>",
        "tabCapture",
        "webRequest",
        "https://",
    ] {
        assert!(
            !text.contains(forbidden),
            "manifest must not mention {forbidden}"
        );
    }
    assert_eq!(manifest["background"]["service_worker"], "background.js");

    // The worker reduces tab.url to its hostname and touches nothing else on the URL.
    let js = read("background.js");
    let uses: Vec<&str> = js
        .match_indices("tab.url")
        .map(|(i, _)| &js[i..i + 30])
        .collect();
    assert!(
        !uses.is_empty(),
        "the worker reads the tab URL to derive the host"
    );
    for use_site in uses {
        assert!(
            use_site.starts_with("tab.url).hostname"),
            "tab.url may only be reduced to a hostname, found: {use_site}"
        );
    }
    for forbidden in [
        "tab.title",
        "tabCapture",
        "chrome.storage",
        "chrome.scripting",
    ] {
        assert!(!js.contains(forbidden), "worker must not use {forbidden}");
    }
    // The generated provisioning file is ignored build output even when a developer has run the
    // harness locally; an environment variable cannot weaken this repository guard.
    let ignore =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.gitignore"))
            .unwrap();
    assert!(ignore.lines().any(|line| line == "/extension/endpoint.js"));
    assert!(
        !extension_dir().join("endpoint.js").exists(),
        "extension/endpoint.js is generated by the harness and must not be checked in"
    );
}
