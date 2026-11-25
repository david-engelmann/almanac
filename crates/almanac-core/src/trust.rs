//! Trust manifest signature verification primitives.
//!
//! The ARD draft seals a trust manifest with a
//! detached JWS over its JCS serialization. This
//! module holds the pure machinery: base64url, JWK
//! reading, and ES256 verification. Fetching keys (a
//! publisher's did:web document) is the CLI's work;
//! everything here takes values and returns answers,
//! so the core stays free of the network.

use serde_json::Value;

use crate::catalog::Entry;
use crate::hash;
use crate::jcs;

/// Whether a manifest presents itself as the draft's
/// signed envelope: it carries any of the envelope's
/// defining members. A manifest carrying none of them
/// is the earlier expiry-only form, judged on expiry
/// alone wherever it surfaces.
pub fn is_envelope(manifest: &Value) -> bool {
    manifest.get("identity").is_some()
        || manifest.get("subject").is_some()
        || manifest.get("signature").is_some()
}

/// The hashes a baseline records for one entry's
/// trust envelopes: the sha256 of each envelope's
/// JCS canonical bytes, the singular manifest first
/// and then the plural array's, sorted so the record
/// is stable under reordering of the array. Only
/// envelopes are recorded; an expiry-only manifest
/// is not an envelope and earns no record, so the
/// removal class it feeds measures envelopes, as it
/// promises. An envelope that cannot be
/// canonicalized is not recorded; the envelope
/// layers report it on their own terms wherever it
/// is judged.
pub fn trust_manifest_hashes(entry: &Entry) -> Vec<String> {
    let mut hashes: Vec<String> = entry
        .trust_manifest
        .iter()
        .chain(entry.trust_manifests.iter())
        .filter(|manifest| is_envelope(manifest))
        .filter_map(|manifest| jcs::canonicalize(manifest).ok())
        .map(|bytes| hash::sha256_hex(&bytes))
        .collect();
    hashes.sort();
    hashes
}

/// One P-256 verification key, with the identifier a
/// JWS header names it by when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustKey {
    pub kid: Option<String>,
    /// The uncompressed point, 0x04 || x || y.
    pub point: Vec<u8>,
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The decode table, built once at compile time.
const B64URL_DECODE: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut index = 0;
    while index < 64 {
        table[B64URL[index] as usize] = index as u8;
        index += 1;
    }
    table
};

/// Decode base64url, padded or unpadded. A length of
/// 1 mod 4 cannot be a base64 encoding of anything
/// and is refused rather than silently truncated.
pub fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4 + 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = B64URL_DECODE[byte as usize];
        if value == 255 {
            return None;
        }
        acc = (acc << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Encode base64url without padding, the JWS form.
pub fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0] as u32,
            *chunk.get(1).unwrap_or(&0) as u32,
            *chunk.get(2).unwrap_or(&0) as u32,
        ];
        let n = (b[0] << 16) | (b[1] << 8) | b[2];
        out.push(B64URL[(n >> 18) as usize & 63] as char);
        out.push(B64URL[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64URL[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[n as usize & 63] as char);
        }
    }
    out
}

/// Read one JWK into a trust key. Only EC P-256 keys
/// are read, the draft's mandatory algorithm's curve;
/// anything else is not a key this build can use.
pub fn trust_key_from_jwk(jwk: &Value) -> Option<TrustKey> {
    if jwk.get("kty").and_then(|v| v.as_str()) != Some("EC")
        || jwk.get("crv").and_then(|v| v.as_str()) != Some("P-256")
    {
        return None;
    }
    let x = b64url_decode(jwk.get("x")?.as_str()?)?;
    let y = b64url_decode(jwk.get("y")?.as_str()?)?;
    if x.len() != 32 || y.len() != 32 {
        return None;
    }
    let mut point = vec![0x04];
    point.extend_from_slice(&x);
    point.extend_from_slice(&y);
    Some(TrustKey {
        kid: jwk
            .get("kid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        point,
    })
}

/// Read a JWKS document (an object with a keys array)
/// or a single bare JWK into trust keys.
pub fn trust_keys_from_jwks(document: &Value) -> Vec<TrustKey> {
    if let Some(keys) = document.get("keys").and_then(|v| v.as_array()) {
        return keys.iter().filter_map(trust_key_from_jwk).collect();
    }
    trust_key_from_jwk(document).into_iter().collect()
}

/// Read the verification keys a DID document carries:
/// its verificationMethod entries holding a
/// publicKeyJwk, each keyed by the method's id, which
/// is the identifier a JWS kid names. A method counts
/// only when it belongs to the document's own subject:
/// its id's DID part must be the document's id, so a
/// document can never register a key under another
/// identity's kid and shadow it in a pooled key list.
/// A relative method id (one starting with #) is read
/// as the document's id with that fragment, the form
/// DID documents commonly carry.
pub fn trust_keys_from_did_document(document: &Value) -> Vec<TrustKey> {
    let Some(subject) = document.get("id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let Some(methods) = document
        .get("verificationMethod")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    methods
        .iter()
        .filter_map(|method| {
            let id = method.get("id").and_then(|v| v.as_str())?;
            let kid = if let Some(fragment) = id.strip_prefix('#') {
                if fragment.is_empty() {
                    return None;
                }
                format!("{subject}#{fragment}")
            } else if id.split('#').next() == Some(subject) {
                id.to_string()
            } else {
                return None;
            };
            // The controller, when the method names one,
            // must be the subject too.
            if let Some(controller) = method.get("controller").and_then(|v| v.as_str())
                && controller != subject
            {
                return None;
            }
            let jwk = method.get("publicKeyJwk")?;
            let mut key = trust_key_from_jwk(jwk)?;
            key.kid = Some(kid);
            Some(key)
        })
        .collect()
}

/// The outcome of judging one signature.
#[derive(Debug, PartialEq, Eq)]
pub enum SignatureVerdict {
    /// The signature verifies under the named key.
    Valid,
    /// The signature is present and was attempted, and
    /// does not verify: malformed, or wrong.
    Invalid(String),
    /// The signature could not be attempted in this
    /// run; the value says why, in words for a report.
    Unverified(String),
}

/// Judge a detached JWS signature over canonical
/// bytes. The signature is the compact form with an
/// empty payload segment; the signing input rebuilds
/// the payload segment from the canonical bytes, as
/// RFC 7515 defines for a detached payload.
pub fn verify_detached_jws(
    signature: &str,
    canonical: &[u8],
    keys: &[TrustKey],
) -> SignatureVerdict {
    let segments: Vec<&str> = signature.split('.').collect();
    if segments.len() != 3 || !segments[1].is_empty() {
        return SignatureVerdict::Invalid(
            "the signature is not a detached JWS compact serialization".into(),
        );
    }
    let Some(header_bytes) = b64url_decode(segments[0]) else {
        return SignatureVerdict::Invalid("the JWS header is not base64url".into());
    };
    let Ok(header) = serde_json::from_slice::<Value>(&header_bytes) else {
        return SignatureVerdict::Invalid("the JWS header is not JSON".into());
    };
    let alg = header.get("alg").and_then(|v| v.as_str()).unwrap_or("");
    if alg != "ES256" {
        return SignatureVerdict::Unverified(format!(
            "the signature algorithm is {alg}, and this build verifies ES256 only"
        ));
    }
    let Some(kid) = header.get("kid").and_then(|v| v.as_str()) else {
        return SignatureVerdict::Unverified("the JWS header names no key".into());
    };
    let Some(key) = keys.iter().find(|k| k.kid.as_deref() == Some(kid)) else {
        return SignatureVerdict::Unverified(format!("no key in this run matches the kid {kid}"));
    };
    let Some(signature_bytes) = b64url_decode(segments[2]) else {
        return SignatureVerdict::Invalid("the JWS signature is not base64url".into());
    };
    if signature_bytes.len() != 64 {
        return SignatureVerdict::Invalid(
            "the JWS signature is not the 64 bytes an ES256 signature is".into(),
        );
    }
    let input = format!("{}.{}", segments[0], b64url_encode(canonical));
    let public = ring::signature::UnparsedPublicKey::new(
        &ring::signature::ECDSA_P256_SHA256_FIXED,
        &key.point,
    );
    match public.verify(input.as_bytes(), &signature_bytes) {
        Ok(()) => SignatureVerdict::Valid,
        Err(_) => SignatureVerdict::Invalid("the signature does not verify".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn b64url_matches_the_rfc_vectors_and_rejects_other_alphabets() {
        // The RFC 4648 vectors, pinned independently of
        // this module's own encode, plus the two bytes
        // whose encodings separate the URL safe alphabet
        // from the standard one.
        let vectors: [(&[u8], &str); 8] = [
            (b"", ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
            (&[0xfb, 0xff], "-_8"),
        ];
        for (bytes, text) in vectors {
            assert_eq!(b64url_encode(bytes), text);
            assert_eq!(b64url_decode(text).unwrap(), bytes);
        }
        assert!(b64url_decode("a+b").is_none());
        assert_eq!(b64url_decode("Zg==").unwrap(), b"f");
        // A lone character is 1 mod 4, no encoding at all.
        assert!(b64url_decode("Z").is_none());
    }

    #[test]
    fn manifest_hashes_record_envelopes_only_over_canonical_bytes() {
        // The record pins the sha256 of the envelope's
        // canonical form, the canonical text written by
        // hand here, so the pin does not ride on the
        // canonicalizer it also exercises. An expiry-only
        // manifest is not an envelope and earns no hash.
        let mut entry = Entry {
            identifier: "urn:air:example.com:mcp:demo".into(),
            display_name: "Demo".into(),
            media_type: "application/mcp-server-card+json".into(),
            url: None,
            data: None,
            description: "A demo server.".into(),
            capabilities: vec![],
            representative_queries: vec![],
            tags: vec![],
            version: None,
            digest: None,
            updated_at: None,
            trust_manifest: Some(json!({"identity": "did:web:example.com"})),
            trust_manifests: vec![json!({"expiresAt": "2999-01-01"})],
        };
        let expected = hash::sha256_hex(b"{\"identity\":\"did:web:example.com\"}");
        assert_eq!(trust_manifest_hashes(&entry), vec![expected]);
        entry.trust_manifest = None;
        assert!(trust_manifest_hashes(&entry).is_empty());
    }

    #[test]
    fn a_did_document_registers_keys_only_under_its_own_subject() {
        let jwk = json!({
            "kty": "EC", "crv": "P-256",
            "x": b64url_encode(&[7u8; 32]), "y": b64url_encode(&[9u8; 32]),
        });
        let document = json!({
            "id": "did:web:auditor.example.org",
            "verificationMethod": [
                {"id": "did:web:auditor.example.org#key-1", "publicKeyJwk": jwk},
                {"id": "#key-2", "publicKeyJwk": jwk},
                {"id": "did:web:example.com#key-1", "publicKeyJwk": jwk},
                {"id": "did:web:auditor.example.org#key-3",
                 "controller": "did:web:example.com", "publicKeyJwk": jwk},
            ],
        });
        let keys = trust_keys_from_did_document(&document);
        let kids: Vec<&str> = keys.iter().filter_map(|k| k.kid.as_deref()).collect();
        // The auditor's own two methods read, the
        // relative one absolutized; the method claiming
        // the publisher's kid and the one controlled by
        // another DID are both refused, so a fetched
        // document cannot shadow a publisher's key.
        assert_eq!(
            kids,
            [
                "did:web:auditor.example.org#key-1",
                "did:web:auditor.example.org#key-2"
            ]
        );
        // A document with no subject id registers none.
        assert!(trust_keys_from_did_document(&json!({"verificationMethod": []})).is_empty());
    }

    #[test]
    fn jwk_reading_takes_only_p256() {
        let key = trust_key_from_jwk(&json!({
            "kty": "EC", "crv": "P-256", "kid": "k1",
            "x": b64url_encode(&[7u8; 32]), "y": b64url_encode(&[9u8; 32]),
        }))
        .expect("a P-256 JWK reads");
        assert_eq!(key.kid.as_deref(), Some("k1"));
        assert_eq!(key.point.len(), 65);
        assert!(trust_key_from_jwk(&json!({"kty": "RSA"})).is_none());
        assert!(trust_key_from_jwk(&json!({"kty": "EC", "crv": "P-384"})).is_none());
    }

    #[test]
    fn a_non_detached_or_wrong_alg_signature_is_judged_without_a_key() {
        assert!(matches!(
            verify_detached_jws("a.b.c", b"{}", &[]),
            SignatureVerdict::Invalid(_)
        ));
        let header = b64url_encode(br#"{"alg":"RS256","kid":"k1"}"#);
        assert!(matches!(
            verify_detached_jws(&format!("{header}..AAAA"), b"{}", &[]),
            SignatureVerdict::Unverified(_)
        ));
    }
}
