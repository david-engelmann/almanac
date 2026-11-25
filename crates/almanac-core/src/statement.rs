//! The attestation statement reduction, for the
//! statement binding layer of the trust pass
//! (DECISIONS.md, attestation statement subject
//! binding). The statement's own signature is the
//! attester's cryptography and is not judged here;
//! the claim is. A statement reduces to the digests
//! and names its subject carries, in the shapes the
//! sources name: an in-toto Statement's subject (an
//! array of items with a name and a digest object),
//! an ARD-shaped subject object as the draft's own
//! envelope subject carries it (an identifier and a
//! digest in <alg>:<hex> form), read through one
//! DSSE unwrap when the document is an envelope
//! typed for in-toto whose payload decodes to the
//! statement. Malformed input follows the house
//! posture for reductions: a document that does not
//! parse, or carries neither shape, reduces to no
//! claims, which the judge reports as unverified
//! rather than honoring silently.

/// The subject claims one attestation statement
/// makes: the digests it names, normalized to
/// <alg>:<hex> with a lowercase algorithm, and the
/// names it names.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StatementClaims {
    pub digests: Vec<String>,
    pub names: Vec<String>,
}

/// One digest claim in the form the judge compares:
/// the algorithm lowercased, the hex as written.
/// Both sides of the comparison are built through
/// this helper so they cannot drift.
pub fn normalize_digest(alg: &str, hex: &str) -> String {
    format!("{}:{}", alg.to_lowercase(), hex)
}

/// Reduce a fetched attestation document to its
/// subject claims.
pub fn reduce_statement(text: &str) -> StatementClaims {
    let mut claims = StatementClaims::default();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return claims;
    };
    let statement = unwrap_dsse(&value).unwrap_or(value);
    if let Some(subject) = statement.get("subject") {
        match subject {
            serde_json::Value::Array(items) => {
                for item in items {
                    collect_subject_item(item, &mut claims);
                }
            }
            serde_json::Value::Object(_) => collect_subject_item(subject, &mut claims),
            _ => {}
        }
    }
    claims
}

/// One DSSE unwrap: an envelope carrying a base64
/// payload typed as an in-toto statement, whose
/// payload parses as the statement. Any other
/// payload type is not an envelope this build
/// unwraps, and such a document is read at its own
/// top level instead.
fn unwrap_dsse(value: &serde_json::Value) -> Option<serde_json::Value> {
    let payload = value.get("payload")?.as_str()?;
    if value.get("payloadType")?.as_str()? != "application/vnd.in-toto+json" {
        return None;
    }
    let bytes = decode_base64(payload)?;
    serde_json::from_slice::<serde_json::Value>(&bytes).ok()
}

/// Base64 in either alphabet, padding optional, as
/// DSSE payloads and their kin arrive. A character
/// outside the alphabets refuses the whole decode;
/// padding and whitespace are skipped wherever they
/// appear, and a trailing residue too short for a
/// byte is dropped, after which the payload simply
/// fails to parse and the document reduces to no
/// claims.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value_of(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | value_of(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            // Keep only the bits not yet pushed, so the
            // accumulator stays small however long the
            // payload runs.
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn collect_subject_item(item: &serde_json::Value, claims: &mut StatementClaims) {
    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
        claims.names.push(name.to_string());
    }
    if let Some(identifier) = item.get("identifier").and_then(|v| v.as_str()) {
        claims.names.push(identifier.to_string());
    }
    match item.get("digest") {
        Some(serde_json::Value::String(digest)) => {
            if let Some((alg, hex)) = digest.split_once(':') {
                claims.digests.push(normalize_digest(alg, hex));
            }
        }
        Some(serde_json::Value::Object(map)) => {
            for (alg, hex) in map {
                if let Some(hex) = hex.as_str() {
                    claims.digests.push(normalize_digest(alg, hex));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_in_toto_subject_reduces_to_its_claims() {
        let text = r#"{
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [
                {"name": "demo", "digest": {"sha256": "AAAA", "sha512": "BBBB"}},
                {"name": "other", "digest": {"gitBlob": "CCCC"}}
            ],
            "predicateType": "https://slsa.dev/provenance/v1"
        }"#;
        let claims = reduce_statement(text);
        assert_eq!(claims.names, ["demo", "other"]);
        assert_eq!(
            claims.digests,
            ["sha256:AAAA", "sha512:BBBB", "gitblob:CCCC"]
        );
    }

    #[test]
    fn an_ard_subject_and_a_dsse_envelope_reduce() {
        let claims = reduce_statement(
            r#"{"subject": {"identifier": "urn:air:example.com:mcp:demo", "digest": "SHA256:FF"}}"#,
        );
        assert_eq!(claims.names, ["urn:air:example.com:mcp:demo"]);
        assert_eq!(claims.digests, ["sha256:FF"]);
        // base64 of {"subject":[{"name":"demo","digest":{"sha256":"AA"}}]}
        let wrapped = r#"{"payload": "eyJzdWJqZWN0IjpbeyJuYW1lIjoiZGVtbyIsImRpZ2VzdCI6eyJzaGEyNTYiOiJBQSJ9fV19", "payloadType": "application/vnd.in-toto+json", "signatures": []}"#;
        let claims = reduce_statement(wrapped);
        assert_eq!(claims.names, ["demo"]);
        assert_eq!(claims.digests, ["sha256:AA"]);
    }

    #[test]
    fn a_document_without_a_subject_claim_reduces_to_none() {
        assert_eq!(reduce_statement("a PDF report").digests.len(), 0);
        assert_eq!(
            reduce_statement(r#"{"runs": []}"#),
            StatementClaims::default()
        );
    }
}
