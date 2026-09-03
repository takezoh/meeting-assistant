//! The public key set embedded in the code-signed binary, and rollover.

use ed25519_dalek::VerifyingKey;
use std::collections::BTreeMap;

pub type KeyId = String;

/// Release public keys. The signing keys never exist in this repository.
const EMBEDDED: [(&str, &str); 1] = [("release-2026", include_str!("../keys/release-2026.pub"))];

#[derive(Debug, Clone)]
pub struct KeySet {
    keys: BTreeMap<KeyId, VerifyingKey>,
}

impl KeySet {
    pub fn embedded() -> KeySet {
        let mut set = KeySet::empty();
        for (id, hex_key) in EMBEDDED {
            set.insert(
                id,
                &parse_hex_key(hex_key.trim()).expect("embedded key is valid"),
            );
        }
        set
    }
    pub fn empty() -> KeySet {
        KeySet {
            keys: BTreeMap::new(),
        }
    }
    pub fn insert(&mut self, id: &str, key: &VerifyingKey) {
        self.keys.insert(id.to_string(), *key);
    }
    pub fn get(&self, id: &str) -> Option<&VerifyingKey> {
        self.keys.get(id)
    }
    pub fn contains(&self, id: &str) -> bool {
        self.keys.contains_key(id)
    }
    pub fn ids(&self) -> Vec<&str> {
        self.keys.keys().map(String::as_str).collect()
    }
}

pub fn parse_hex_key(hex_key: &str) -> Option<VerifyingKey> {
    let bytes = hex::decode(hex_key).ok()?;
    let array: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&array).ok()
}
