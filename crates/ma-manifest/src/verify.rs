//! Verify first, parse second. The wire form is a one-line header followed by the payload bytes:
//! `ma-manifest-v1 <key_id> <signature-hex>\n<payload>`. The header is not JSON, so a captive-portal
//! HTML page fails as `NotAManifest` before anything else happens, and the key id is used for
//! nothing but the key-set lookup.

use crate::keys::KeySet;
use crate::manifest::{artifacts_well_formed, AdapterManifest, UpdateManifest};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};

pub const HEADER_PREFIX: &str = "ma-manifest-v1";

/// Typed, distinct rejection codes. None of them carries a manifest-declared value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectCode {
    /// Not our header at all (captive portal, HTML, truncated).
    NotAManifest,
    /// The key id is not in the embedded set: the manifest is signed only by an unknown key.
    UnknownKey,
    /// The signature does not verify: tampered or corrupted.
    Tampered,
    /// Verified but the payload is not a well-formed manifest (schema, hosts, digests).
    Malformed,
    /// The manifest version is not strictly greater than the installed one.
    Downgrade,
    /// A declared artifact digest does not match the file on disk.
    DigestMismatch,
}

/// A payload whose signature verified under a known key. The only way to reach the parsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified<'a> {
    key_id: &'a str,
    payload: &'a [u8],
}

impl<'a> Verified<'a> {
    pub fn key_id(&self) -> &'a str {
        self.key_id
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
    pub fn parse_update(&self) -> Result<UpdateManifest, RejectCode> {
        let manifest: UpdateManifest =
            serde_json::from_slice(self.payload).map_err(|_| RejectCode::Malformed)?;
        if !artifacts_well_formed(&manifest.artifacts) || manifest.artifacts.is_empty() {
            return Err(RejectCode::Malformed);
        }
        if let Some(rollover) = &manifest.key_rollover {
            if crate::keys::parse_hex_key(&rollover.next_public_key).is_none()
                || rollover.next_key_id.is_empty()
            {
                return Err(RejectCode::Malformed);
            }
        }
        Ok(manifest)
    }
    pub fn parse_adapter(&self) -> Result<AdapterManifest, RejectCode> {
        let manifest: AdapterManifest =
            serde_json::from_slice(self.payload).map_err(|_| RejectCode::Malformed)?;
        if !artifacts_well_formed(&manifest.artifacts) || manifest.artifacts.is_empty() {
            return Err(RejectCode::Malformed);
        }
        Ok(manifest)
    }
}

/// Split the header. Returns (key_id, signature, payload) without interpreting the payload.
fn split(bytes: &[u8]) -> Result<(&str, Signature, &[u8]), RejectCode> {
    let newline = bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or(RejectCode::NotAManifest)?;
    let header = std::str::from_utf8(&bytes[..newline]).map_err(|_| RejectCode::NotAManifest)?;
    let mut parts = header.split(' ');
    if parts.next() != Some(HEADER_PREFIX) {
        return Err(RejectCode::NotAManifest);
    }
    let key_id = parts
        .next()
        .filter(|k| !k.is_empty())
        .ok_or(RejectCode::NotAManifest)?;
    let signature_hex = parts.next().ok_or(RejectCode::NotAManifest)?;
    if parts.next().is_some() {
        return Err(RejectCode::NotAManifest);
    }
    let signature_bytes = hex::decode(signature_hex).map_err(|_| RejectCode::NotAManifest)?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| RejectCode::NotAManifest)?;
    Ok((key_id, signature, &bytes[newline + 1..]))
}

/// Verify `bytes` against `keys`. Nothing in the payload is read before the signature verifies.
pub fn verify<'a>(bytes: &'a [u8], keys: &KeySet) -> Result<Verified<'a>, RejectCode> {
    let (key_id, signature, payload) = split(bytes)?;
    let key = keys.get(key_id).ok_or(RejectCode::UnknownKey)?;
    key.verify(payload, &signature)
        .map_err(|_| RejectCode::Tampered)?;
    Ok(Verified { key_id, payload })
}

/// Produce the wire form. Used by the release workflow's signing step and by tests.
pub fn sign(payload: &[u8], key_id: &str, signing_key: &SigningKey) -> Vec<u8> {
    let signature = signing_key.sign(payload);
    let mut out = format!(
        "{HEADER_PREFIX} {key_id} {}\n",
        hex::encode(signature.to_bytes())
    )
    .into_bytes();
    out.extend_from_slice(payload);
    out
}
