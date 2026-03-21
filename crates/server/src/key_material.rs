//! API key hashing (SHA-256 hex, lowercase) for storage and comparison.

use sha2::{Digest, Sha256};

/// Deterministic hash of a presented API secret (matches stored `api_keys.key_hash`).
pub fn hash_api_key_secret(plaintext: &str) -> String {
    let mut h = Sha256::new();
    h.update(plaintext.as_bytes());
    hex::encode(h.finalize())
}

/// Random secret prefix `rh_` + 64 hex chars (256-bit).
pub fn generate_api_key_plaintext() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    format!("rh_{}", hex::encode(bytes))
}
