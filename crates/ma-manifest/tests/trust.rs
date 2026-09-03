use ed25519_dalek::SigningKey;
use ma_manifest::*;
use std::path::Path;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn keys_with(id: &str, sk: &SigningKey) -> KeySet {
    let mut set = KeySet::empty();
    set.insert(id, &sk.verifying_key());
    set
}
fn update_payload(manifest_version: u64, rollover: Option<(&str, &SigningKey)>) -> Vec<u8> {
    let mut value = serde_json::json!({
        "manifest_version": manifest_version,
        "version": format!("0.{manifest_version}.0"),
        "channel": "release",
        "artifacts": [{ "name": "MeetingAssistant-setup.exe", "url": "https://objects.githubusercontent.com/example/MeetingAssistant-setup.exe", "sha256": "ab".repeat(32), "size": 12345 }],
        "engine_replacement": true
    });
    if let Some((next_id, next)) = rollover {
        value["key_rollover"] = serde_json::json!({ "next_key_id": next_id, "next_public_key": hex::encode(next.verifying_key().to_bytes()) });
    }
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn tampered_manifest_rejected() {
    let sk = key(1);
    let keys = keys_with("release-2026", &sk);
    let good = sign(&update_payload(8, None), "release-2026", &sk);
    assert!(verify(&good, &keys).is_ok());
    // flip one payload byte after the header
    let mut tampered = good.clone();
    let header_end = tampered.iter().position(|b| *b == b'\n').unwrap();
    let idx = header_end + 20;
    tampered[idx] ^= 0x01;
    let err = verify(&tampered, &keys).unwrap_err();
    assert_eq!(err, RejectCode::Tampered);
    // a marker in a tampered payload never reaches the error or a debug line
    let mut marked = update_payload(8, None);
    marked.extend_from_slice(b" ZZ-UNVERIFIED-URL-ZZ");
    let forged = sign(&marked, "release-2026", &key(2));
    let err = verify(&forged, &keys).unwrap_err();
    assert_eq!(err, RejectCode::Tampered);
    assert!(!format!("{err:?}").contains("ZZ-UNVERIFIED"));
    // captive portal, HTML, truncated
    for junk in [
        &b"<html><body>Sign in to the network</body></html>"[..],
        b"",
        b"ma-manifest-v1 release-2026",
        b"ma-manifest-v1 release-2026 zz\n{}",
    ] {
        assert_eq!(
            verify(junk, &keys).unwrap_err(),
            RejectCode::NotAManifest,
            "{junk:?}"
        );
    }
    // the four codes are distinct
    let codes = [
        RejectCode::Tampered,
        RejectCode::Downgrade,
        RejectCode::UnknownKey,
        RejectCode::DigestMismatch,
        RejectCode::NotAManifest,
        RejectCode::Malformed,
    ];
    for (i, a) in codes.iter().enumerate() {
        for (j, b) in codes.iter().enumerate() {
            assert_eq!(i == j, a == b);
        }
    }
}

#[test]
fn replayed_older_manifest_rejected() {
    let sk = key(1);
    let mut updater = Updater::new(keys_with("release-2026", &sk), 9);
    let replayed = sign(&update_payload(7, None), "release-2026", &sk);
    assert_eq!(
        updater.consider(&replayed, false, false).unwrap_err(),
        RejectCode::Downgrade,
        "signature validity alone is not sufficient"
    );
    let same = sign(&update_payload(9, None), "release-2026", &sk);
    assert_eq!(
        updater.consider(&same, false, false).unwrap_err(),
        RejectCode::Downgrade
    );
    let newer = sign(&update_payload(10, None), "release-2026", &sk);
    assert!(
        matches!(updater.consider(&newer, false, false), Ok(UpdateDecision::Apply(m)) if m.manifest_version == 10)
    );
    // explicit confirmation is the only way down
    assert!(matches!(
        updater.consider(&replayed, false, true),
        Ok(UpdateDecision::Apply(_))
    ));
    assert_eq!(
        check_version(1, 0, false),
        Ok(()),
        "unknown installed version is 0 and still verifies"
    );
    // an engine replacement waits for a terminal session
    let mut updater = Updater::new(keys_with("release-2026", &sk), 9);
    assert!(matches!(
        updater.consider(&newer, true, false),
        Ok(UpdateDecision::Deferred(_))
    ));
    assert_eq!(updater.installed_manifest_version, 9, "nothing was applied");
}

#[test]
fn unknown_key_rejected_rollover_accepted() {
    let current = key(1);
    let next = key(3);
    let mut updater = Updater::new(keys_with("release-2026", &current), 1);
    let only_next = sign(&update_payload(2, None), "release-2027", &next);
    assert_eq!(
        updater.consider(&only_next, false, false).unwrap_err(),
        RejectCode::UnknownKey
    );
    // a known key id but the wrong private key is tampering, not an unknown key
    let wrong = sign(&update_payload(2, None), "release-2026", &next);
    assert_eq!(
        updater.consider(&wrong, false, false).unwrap_err(),
        RejectCode::Tampered
    );
    // rollover signed by the current key introduces the next key
    let rollover = sign(
        &update_payload(2, Some(("release-2027", &next))),
        "release-2026",
        &current,
    );
    assert!(matches!(
        updater.consider(&rollover, false, false),
        Ok(UpdateDecision::Apply(_))
    ));
    assert!(updater.keys.contains("release-2027"));
    updater.installed_manifest_version = 2;
    let signed_by_next = sign(&update_payload(3, None), "release-2027", &next);
    assert!(matches!(
        updater.consider(&signed_by_next, false, false),
        Ok(UpdateDecision::Apply(_))
    ));
    // the embedded key set exists and names a key
    assert_eq!(KeySet::embedded().ids(), ["release-2026"]);
}

#[test]
fn digest_mismatch_blocks_adapter_activation() {
    let sk = key(1);
    let keys = keys_with("release-2026", &sk);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("adapter-meet.json"),
        b"{\"adapter\":\"meet\"}",
    )
    .unwrap();
    let real = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
        b"{\"adapter\":\"meet\"}",
    ));
    let payload = |sha: &str| {
        serde_json::to_vec(&serde_json::json!({
        "manifest_version": 4, "adapter_id": "meet", "version": "1.2.0",
        "artifacts": [{ "name": "adapter-meet.json", "url": "https://objects.githubusercontent.com/example/adapter-meet.json", "sha256": sha, "size": 18 }],
        "pinned_extension_id": "abcdefghijklmnopabcdefghijklmnop"
    })).unwrap()
    };
    let bad = sign(&payload(&"cd".repeat(32)), "release-2026", &sk);
    let manifest = verify(&bad, &keys).unwrap().parse_adapter().unwrap();
    assert_eq!(
        rollback::activate_adapter(&manifest, dir.path()).unwrap_err(),
        RejectCode::DigestMismatch
    );
    let good = sign(&payload(&real), "release-2026", &sk);
    let manifest = verify(&good, &keys).unwrap().parse_adapter().unwrap();
    assert_eq!(
        rollback::activate_adapter(&manifest, dir.path()).unwrap(),
        ["adapter-meet.json"]
    );
    // a manifest pointing outside the distribution hosts, or with a path-like name, is malformed
    let elsewhere = serde_json::to_vec(&serde_json::json!({ "manifest_version": 4, "adapter_id": "meet", "version": "1.2.0", "artifacts": [{ "name": "../adapter.json", "url": "https://cdn.example-vendor.test/adapter.json", "sha256": real, "size": 18 }] })).unwrap();
    assert_eq!(
        verify(&sign(&elsewhere, "release-2026", &sk), &keys)
            .unwrap()
            .parse_adapter()
            .unwrap_err(),
        RejectCode::Malformed
    );
    // schemas describe the payloads
    for (schema, payload) in [
        ("update-manifest.schema.json", update_payload(1, None)),
        ("adapter-manifest.schema.json", payload(&real)),
    ] {
        let schema_text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/manifest")
                .join(schema),
        )
        .unwrap();
        let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for field in required {
            assert!(value.get(field).is_some(), "{schema:?} requires {field}");
        }
        assert_eq!(schema["additionalProperties"], false);
    }
}
