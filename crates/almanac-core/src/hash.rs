//! Hashing over canonical bytes.
//!
//! Every hash in the baseline and every digest in a catalog entry
//! is a SHA-256 over canonical bytes, so the same content produces
//! the same hash on every machine. Canonical JSON here means
//! serde_json's map ordering, which sorts object keys, rendered
//! without insignificant whitespace for hashing purposes. The
//! catalog files themselves are written pretty printed by the CLI,
//! over the same sorted key order.

use sha2::{Digest, Sha256};

/// SHA-256 of raw bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// SHA-256 of a JSON value's canonical bytes, lowercase hex.
pub fn sha256_json(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).expect("a JSON value always serializes");
    sha256_hex(&bytes)
}

/// SHA-256 of a string's UTF-8 bytes, lowercase hex.
pub fn sha256_str(text: &str) -> String {
    sha256_hex(text.as_bytes())
}

/// The digest field form the ARD draft defines, `sha256:` prefixed.
pub fn digest_field(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hash_matches_the_known_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn json_key_order_does_not_change_the_hash() {
        let a: serde_json::Value = serde_json::json!({"b": 1, "a": 2});
        let b: serde_json::Value = serde_json::json!({"a": 2, "b": 1});
        assert_eq!(sha256_json(&a), sha256_json(&b));
    }
}
