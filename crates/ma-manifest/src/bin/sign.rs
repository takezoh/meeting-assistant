//! `ma-manifest-sign <key-id> <payload.json> <out>`: signs a manifest payload with the Ed25519
//! private key in `MA_MANIFEST_SIGNING_KEY` (32 bytes, hex). Used only by the release workflow.

use ed25519_dalek::SigningKey;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: ma-manifest-sign <key-id> <payload.json> <out>");
        std::process::exit(2);
    }
    let key_hex = std::env::var("MA_MANIFEST_SIGNING_KEY").unwrap_or_else(|_| {
        eprintln!("MA_MANIFEST_SIGNING_KEY is not set");
        std::process::exit(2);
    });
    let key_bytes: [u8; 32] = hex::decode(key_hex.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .unwrap_or_else(|| {
            eprintln!("MA_MANIFEST_SIGNING_KEY must be 32 bytes of hex");
            std::process::exit(2);
        });
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let payload = std::fs::read(&args[2]).expect("payload readable");
    // the payload must be a well-formed manifest before it is signed
    let probe = ma_manifest::sign(&payload, &args[1], &signing_key);
    let mut keys = ma_manifest::KeySet::empty();
    keys.insert(&args[1], &signing_key.verifying_key());
    let verified = ma_manifest::verify(&probe, &keys).expect("self-verification");
    if verified.parse_update().is_err() && verified.parse_adapter().is_err() {
        eprintln!("payload is not a well-formed update or adapter manifest");
        std::process::exit(1);
    }
    std::fs::write(&args[3], probe).expect("output writable");
}
