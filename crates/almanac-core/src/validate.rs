//! Catalog validation beyond schema.
//!
//! Schema says a catalog parses. Validation says it holds
//! together. Every entry anchored to the publisher's domain,
//! exactly one of url or data per entry, digests in the draft's
//! format, representative queries in the count the draft asks
//! for, the dual path pair in agreement, and a trust manifest
//! whose shape and expiry hold up. Amendment D bars signing
//! and key management, not verification: the keyed entry
//! point below additionally judges a manifest that presents
//! itself as the draft's signed envelope, subject binding,
//! identity binding, and the signature itself, under the
//! draft's section 3.5 rules.
//!
//! The functions are pure. The CLI reads the files and keys
//! and passes the current date and time in, so the same
//! catalog always produces the same findings on the same day.

use crate::Severity;
use crate::catalog::{Catalog, Entry};
use crate::drift::Finding;
use crate::jcs;
use crate::trust::{SignatureVerdict, TrustKey, verify_detached_jws};

fn finding(severity: Severity, kind: &str, entry: Option<&str>, detail: String) -> Finding {
    Finding {
        severity,
        kind: kind.to_string(),
        entry: entry.map(|e| e.to_string()),
        detail,
    }
}

/// Days from civil, the standard algorithm, so dates compare
/// and subtract without a date crate.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parse a YYYY-MM-DD prefix into a day number.
fn parse_date(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: i64 = text.get(5..7)?.parse().ok()?;
    let day: i64 = text.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn valid_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Validate one catalog, and its legacy twin when one is given.
/// `today` is YYYY-MM-DD and `now_unix` the current time in
/// seconds, both supplied by the caller.
pub fn validate_catalog(
    published: &Catalog,
    legacy: Option<&Catalog>,
    today: &str,
    now_unix: i64,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    if let Some(legacy) = legacy
        && legacy.entries != published.entries
    {
        findings.push(finding(
            Severity::Critical,
            "path-divergence",
            None,
            "ard.json and ai-catalog.json entries differ".into(),
        ));
    }

    for entry in &published.entries {
        let id = entry.identifier.as_str();
        if entry.has_shape_error() {
            findings.push(finding(
                Severity::Critical,
                "entry-shape",
                Some(id),
                "entry carries both url and data, or neither".into(),
            ));
        }
        if !entry.anchored_to(&published.host.domain) {
            findings.push(finding(
                Severity::Critical,
                "urn-not-anchored",
                Some(id),
                format!(
                    "entry is not anchored to publisher domain {}",
                    published.host.domain
                ),
            ));
        }
        if entry.display_name.trim().is_empty() {
            findings.push(finding(
                Severity::Major,
                "entry-incomplete",
                Some(id),
                "entry displayName is empty".into(),
            ));
        }
        if entry.media_type.trim().is_empty() {
            findings.push(finding(
                Severity::Major,
                "entry-incomplete",
                Some(id),
                "entry type is empty".into(),
            ));
        }
        if entry.description.trim().is_empty() {
            findings.push(finding(
                Severity::Minor,
                "entry-incomplete",
                Some(id),
                "entry description is empty".into(),
            ));
        }
        let queries = entry.representative_queries.len();
        if queries == 0 {
            findings.push(finding(
                Severity::Minor,
                "representative-queries",
                Some(id),
                "entry carries no representative queries, and the draft asks for two to five"
                    .into(),
            ));
        } else if queries == 1 || queries > 5 {
            findings.push(finding(
                Severity::Minor,
                "representative-queries",
                Some(id),
                format!(
                    "entry carries {queries} representative queries, and the draft asks for two to five"
                ),
            ));
        }
        match &entry.digest {
            Some(digest) if !valid_digest(digest) => findings.push(finding(
                Severity::Major,
                "digest-format",
                Some(id),
                "entry digest is not a sha256: prefixed lowercase hex digest".into(),
            )),
            None if entry.url.is_some() => findings.push(finding(
                Severity::Minor,
                "digest-missing",
                Some(id),
                "entry references an artifact by url and carries no digest".into(),
            )),
            _ => {}
        }
        if let Some(manifest) = &entry.trust_manifest {
            validate_trust_manifest(manifest, id, today, now_unix, &mut findings);
        }
    }

    sort_findings(&mut findings);
    findings
}

fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.entry.cmp(&b.entry))
            .then_with(|| a.detail.cmp(&b.detail))
    });
}

/// Validate with trust manifest verification, the validate
/// command's entry point. Everything the plain pass judges,
/// plus, for every trust manifest an entry carries, the
/// expiry judgment for third-party manifests and the
/// envelope verification of the design record: a manifest
/// carrying identity, subject, or signature presents itself
/// as the draft's signed envelope and is judged as one.
/// `keys` are the verification keys this run holds, from a
/// keys file or a resolved did:web document; an empty set
/// leaves every signature unverified, and the report says
/// so rather than passing it silently.
pub fn validate_catalog_with_keys(
    published: &Catalog,
    legacy: Option<&Catalog>,
    today: &str,
    now_unix: i64,
    keys: &[TrustKey],
) -> Vec<Finding> {
    let mut findings = validate_catalog(published, legacy, today, now_unix);
    for entry in &published.entries {
        // Identity binds to the publisher segment of the
        // entry's own URN, the draft's section 3.5.1 rule;
        // the host domain stands in only when the URN does
        // not parse, a case the plain pass already judges.
        let publisher = urn_publisher(&entry.identifier)
            .unwrap_or(published.host.domain.as_str())
            .to_string();
        if let Some(manifest) = &entry.trust_manifest {
            verify_envelope(manifest, entry, &publisher, keys, false, &mut findings);
        }
        for manifest in &entry.trust_manifests {
            validate_trust_manifest(manifest, &entry.identifier, today, now_unix, &mut findings);
            verify_envelope(manifest, entry, &publisher, keys, true, &mut findings);
        }
    }
    sort_findings(&mut findings);
    findings
}

/// The publisher segment of a domain anchored URN,
/// urn:air:<publisher>:..., when the identifier has
/// that shape.
fn urn_publisher(identifier: &str) -> Option<&str> {
    let rest = identifier.strip_prefix("urn:air:")?;
    let segment = rest.split(':').next()?;
    if segment.is_empty() {
        None
    } else {
        Some(segment)
    }
}

/// Whether a manifest presents itself as the draft's
/// signed envelope: it carries any of the envelope's
/// defining members. A manifest carrying none of them
/// is the earlier expiry-only form, and the plain pass
/// has already judged all of it this build judges.
fn is_envelope(manifest: &serde_json::Value) -> bool {
    manifest.get("identity").is_some()
        || manifest.get("subject").is_some()
        || manifest.get("signature").is_some()
}

/// Whether an identity binds to a publisher domain, the
/// draft's publisher profile: a did:web identity naming
/// the domain, an https identity on it, or a SPIFFE id
/// whose trust domain it is.
fn identity_binds(identity: &str, domain: &str) -> bool {
    if let Some(rest) = identity.strip_prefix("did:web:") {
        return rest == domain || rest.starts_with(&format!("{domain}:"));
    }
    if let Some(rest) = identity.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or("");
        return host == domain;
    }
    if let Some(rest) = identity.strip_prefix("spiffe://") {
        return rest == domain || rest.starts_with(&format!("{domain}/"));
    }
    false
}

/// The envelope layers over one trust manifest. Shape
/// first (the envelope's defining members present, at
/// major), then subject binding and, for the publisher's
/// own manifest, identity binding (critical on a miss,
/// the draft's reject cases), then the signature:
/// invalid is critical, and a signature this run cannot
/// attempt is a minor naming why, never a silent pass.
fn verify_envelope(
    manifest: &serde_json::Value,
    entry: &Entry,
    publisher: &str,
    keys: &[TrustKey],
    third_party: bool,
    findings: &mut Vec<Finding>,
) {
    if !manifest.is_object() || !is_envelope(manifest) {
        return;
    }
    let id = entry.identifier.as_str();
    let identity = manifest.get("identity").and_then(|v| v.as_str());
    if identity.is_none_or(|text| text.trim().is_empty()) {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(id),
            "trust manifest carries no identity".into(),
        ));
    }
    let subject = manifest.get("subject");
    let subject_object = subject.filter(|s| s.is_object());
    if subject_object.is_none() {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(id),
            "trust manifest carries no subject object".into(),
        ));
    } else if let Some(subject) = subject_object {
        // Absent identifier or type is a shape absence,
        // layer 1's judgment; a present one that differs
        // is a binding failure, layer 2's.
        let subject_id = subject.get("identifier").and_then(|v| v.as_str());
        if subject_id.is_none() {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(id),
                "trust manifest subject carries no identifier".into(),
            ));
        }
        let subject_type = subject.get("type").and_then(|v| v.as_str());
        if subject_type.is_none() {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(id),
                "trust manifest subject carries no type".into(),
            ));
        }
        let mut mismatched: Vec<&str> = Vec::new();
        if let Some(subject_id) = subject_id
            && subject_id != id
        {
            mismatched.push("identifier");
        }
        if let Some(subject_type) = subject_type
            && subject_type != entry.media_type
        {
            mismatched.push("type");
        }
        // Digest and version compare where both sides
        // carry them, with the design's one exception:
        // a subject digest absent while the entry
        // carries one is itself a mismatch, because
        // the digest is the binding the draft seals.
        let subject_digest = subject.get("digest").and_then(|v| v.as_str());
        let digest_mismatch = match (subject_digest, entry.digest.as_deref()) {
            (Some(claimed), Some(actual)) => claimed != actual,
            (None, Some(_)) => true,
            _ => false,
        };
        if digest_mismatch {
            mismatched.push("digest");
        }
        let subject_version = subject.get("version").and_then(|v| v.as_str());
        if let (Some(claimed), Some(actual)) = (subject_version, entry.version.as_deref())
            && claimed != actual
        {
            mismatched.push("version");
        }
        if !mismatched.is_empty() {
            findings.push(finding(
                Severity::Critical,
                "trust-manifest-subject-mismatch",
                Some(id),
                format!(
                    "trust manifest subject does not match the entry on {}",
                    mismatched.join(", ")
                ),
            ));
        }
    }
    if !third_party
        && let Some(identity) = identity
        && !identity_binds(identity, publisher)
    {
        findings.push(finding(
            Severity::Critical,
            "trust-manifest-identity-unbound",
            Some(id),
            format!(
                "trust manifest identity {identity} does not bind to publisher domain {publisher}"
            ),
        ));
    }
    let Some(signature) = manifest.get("signature") else {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(id),
            "trust manifest carries no signature".into(),
        ));
        return;
    };
    let Some(signature) = signature.as_str() else {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(id),
            "trust manifest signature is not a string".into(),
        ));
        return;
    };
    let mut unsigned = manifest.clone();
    unsigned
        .as_object_mut()
        .expect("an envelope is an object")
        .remove("signature");
    let canonical = match jcs::canonicalize(&unsigned) {
        Ok(bytes) => bytes,
        Err(reason) => {
            findings.push(finding(
                Severity::Minor,
                "trust-manifest-signature-unverified",
                Some(id),
                format!("the trust manifest could not be canonicalized: {reason}"),
            ));
            return;
        }
    };
    match verify_detached_jws(signature, &canonical, keys) {
        SignatureVerdict::Valid => {}
        SignatureVerdict::Invalid(reason) => findings.push(finding(
            Severity::Critical,
            "trust-manifest-signature-invalid",
            Some(id),
            format!("trust manifest signature does not verify: {reason}"),
        )),
        SignatureVerdict::Unverified(reason) => findings.push(finding(
            Severity::Minor,
            "trust-manifest-signature-unverified",
            Some(id),
            format!("trust manifest signature went unverified: {reason}"),
        )),
    }
}

/// Shape and expiry for one trust manifest. The expiry is read
/// where version 1 knows to look, an expiresAt date on the
/// manifest or on its inner manifest document, or an exp claim
/// in seconds. The envelope layers (subject, identity, and
/// signature) are verify_envelope's, in the keyed pass.
fn validate_trust_manifest(
    manifest: &serde_json::Value,
    entry_id: &str,
    today: &str,
    now_unix: i64,
    findings: &mut Vec<Finding>,
) {
    if !manifest.is_object() {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(entry_id),
            "trust manifest is not a JSON object".into(),
        ));
        return;
    }
    let inner = manifest.get("manifest");
    let date_text = manifest
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .or_else(|| {
            inner
                .and_then(|m| m.get("expiresAt"))
                .and_then(|v| v.as_str())
        });
    let exp = manifest
        .get("exp")
        .and_then(|v| v.as_i64())
        .or_else(|| inner.and_then(|m| m.get("exp")).and_then(|v| v.as_i64()));

    let today_days = parse_date(today);
    let mut judged = false;
    if let Some(text) = date_text {
        match (parse_date(text), today_days) {
            (Some(expiry), Some(today)) => {
                judged = true;
                if expiry < today {
                    findings.push(finding(
                        Severity::Major,
                        "trust-manifest-expired",
                        Some(entry_id),
                        format!("trust manifest expired on {}", &text[..10.min(text.len())]),
                    ));
                } else if expiry - today <= 30 {
                    findings.push(finding(
                        Severity::Minor,
                        "trust-manifest-expiring",
                        Some(entry_id),
                        format!(
                            "trust manifest expires on {}, within thirty days",
                            &text[..10.min(text.len())]
                        ),
                    ));
                }
            }
            _ => findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(entry_id),
                "trust manifest expiresAt is not a readable date".into(),
            )),
        }
    }
    if let Some(exp) = exp {
        judged = true;
        if exp < now_unix {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-expired",
                Some(entry_id),
                "trust manifest exp claim is in the past".into(),
            ));
        } else if exp - now_unix <= 30 * 86_400 {
            findings.push(finding(
                Severity::Minor,
                "trust-manifest-expiring",
                Some(entry_id),
                "trust manifest exp claim falls within thirty days".into(),
            ));
        }
    }
    if !judged && date_text.is_none() {
        findings.push(finding(
            Severity::Minor,
            "trust-manifest-shape",
            Some(entry_id),
            "trust manifest carries no expiry this build can read, so its expiry went unvalidated"
                .into(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Entry, Host};

    fn catalog_with(entry: Entry) -> Catalog {
        Catalog {
            host: Host {
                display_name: "Example".into(),
                domain: "example.com".into(),
            },
            entries: vec![entry],
        }
    }

    fn entry() -> Entry {
        Entry {
            identifier: "urn:air:example.com:mcp:demo".into(),
            display_name: "Demo".into(),
            media_type: "application/mcp-server-card+json".into(),
            url: Some("https://example.com/mcp".into()),
            data: None,
            description: "A demo server.".into(),
            capabilities: vec![],
            representative_queries: vec!["one".into(), "two".into()],
            tags: vec![],
            version: None,
            digest: Some(format!("sha256:{}", "a".repeat(64))),
            updated_at: None,
            trust_manifest: None,
            trust_manifests: vec![],
        }
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn a_sound_catalog_validates_clean() {
        let catalog = catalog_with(entry());
        assert!(validate_catalog(&catalog, Some(&catalog), "2026-11-21", NOW).is_empty());
    }

    #[test]
    fn shape_anchor_and_digest_format_fire() {
        let mut bad = entry();
        bad.data = Some(serde_json::json!({}));
        bad.identifier = "urn:air:other.example:mcp:demo".into();
        bad.digest = Some("sha256:XYZ".into());
        let findings = validate_catalog(&catalog_with(bad), None, "2026-11-21", NOW);
        assert!(findings.iter().any(|f| f.kind == "entry-shape"));
        assert!(findings.iter().any(|f| f.kind == "urn-not-anchored"));
        assert!(findings.iter().any(|f| f.kind == "digest-format"));
    }

    #[test]
    fn query_count_outside_two_to_five_warns() {
        let mut one = entry();
        one.representative_queries = vec!["only".into()];
        let findings = validate_catalog(&catalog_with(one), None, "2026-11-21", NOW);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "representative-queries");
        assert_eq!(findings[0].severity, Severity::Minor);
    }

    // A real ES256 fixture: a P-256 key generated for
    // these tests, its JWK, and a detached JWS over the
    // JCS form of the manifest below, signed with
    // openssl. The signature segment is only valid for
    // the manifest exactly as written.
    const FIXTURE_JWK: &str = r#"{"kty":"EC","crv":"P-256","kid":"did:web:example.com#key-1","x":"KFvtK60fJVnMb4stS4hJSzf4U37o4zyW2oy0fuOL_Ow","y":"C43rXNNfvh4McIabWEirso0xH2eDtSDCVfupwMHs7mE"}"#;
    const FIXTURE_SIGNATURE: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImRpZDp3ZWI6ZXhhbXBsZS5jb20ja2V5LTEifQ.._JNWqS8t8CWplUrJOvIPW_UiNQ_6pwTC8NTbZyEJEHivmugANeHTwTDP3UYw7SU6Yirz5DknxMsRln6MIdFdXw";

    fn signed_entry() -> Entry {
        let mut signed = entry();
        signed.trust_manifest = Some(serde_json::json!({
            "identity": "did:web:example.com",
            "issuedAt": "2026-11-20T00:00:00Z",
            "expiresAt": "2027-06-01",
            "subject": {
                "identifier": "urn:air:example.com:mcp:demo",
                "type": "application/mcp-server-card+json",
                "digest": format!("sha256:{}", "a".repeat(64)),
            },
            "signature": FIXTURE_SIGNATURE,
        }));
        signed
    }

    fn fixture_keys() -> Vec<TrustKey> {
        crate::trust::trust_keys_from_jwks(
            &serde_json::from_str::<serde_json::Value>(FIXTURE_JWK).expect("the JWK parses"),
        )
    }

    #[test]
    fn a_signed_envelope_verifies_under_its_key() {
        let findings = validate_catalog_with_keys(
            &catalog_with(signed_entry()),
            None,
            "2026-11-21",
            NOW,
            &fixture_keys(),
        );
        assert!(findings.is_empty(), "findings were {findings:?}");
    }

    #[test]
    fn a_signed_envelope_without_a_key_reports_unverified_never_silent() {
        let findings =
            validate_catalog_with_keys(&catalog_with(signed_entry()), None, "2026-11-21", NOW, &[]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "trust-manifest-signature-unverified");
        assert_eq!(findings[0].severity, Severity::Minor);
        // The plain pass, which check uses, judges no
        // envelope layer at all.
        assert!(
            validate_catalog(&catalog_with(signed_entry()), None, "2026-11-21", NOW).is_empty()
        );
    }

    #[test]
    fn a_tampered_envelope_fails_its_signature_and_its_subject() {
        // One field of the signed fixture changes: the
        // subject's claimed digest. The binding breaks
        // against the entry, and the signature, which
        // sealed the original subject, breaks with it.
        let mut tampered = signed_entry();
        tampered
            .trust_manifest
            .as_mut()
            .and_then(|m| m.get_mut("subject"))
            .and_then(|s| s.as_object_mut())
            .expect("the fixture carries a subject object")
            .insert(
                "digest".into(),
                serde_json::Value::String(format!("sha256:{}", "b".repeat(64))),
            );
        let findings = validate_catalog_with_keys(
            &catalog_with(tampered),
            None,
            "2026-11-21",
            NOW,
            &fixture_keys(),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "trust-manifest-subject-mismatch")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "trust-manifest-signature-invalid")
        );
    }

    #[test]
    fn an_unbound_identity_is_critical_and_a_third_party_is_exempt() {
        let mut unbound = signed_entry();
        unbound
            .trust_manifest
            .as_mut()
            .and_then(|m| m.as_object_mut())
            .expect("the fixture manifest is an object")
            .insert(
                "identity".into(),
                serde_json::Value::String("did:web:other.example".into()),
            );
        let findings = validate_catalog_with_keys(
            &catalog_with(unbound),
            None,
            "2026-11-21",
            NOW,
            &fixture_keys(),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unbound")
        );

        // The same manifest as a third-party envelope is
        // not held to the publisher binding.
        let mut third = entry();
        third.trust_manifests = vec![serde_json::json!({
            "identity": "did:web:auditor.example.org",
            "subject": {
                "identifier": "urn:air:example.com:mcp:demo",
                "type": "application/mcp-server-card+json",
                "digest": format!("sha256:{}", "a".repeat(64)),
            },
            "signature": FIXTURE_SIGNATURE,
        })];
        let findings =
            validate_catalog_with_keys(&catalog_with(third), None, "2026-11-21", NOW, &[]);
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unbound"),
            "findings were {findings:?}"
        );
    }

    #[test]
    fn trust_manifest_expiry_is_judged_by_date() {
        let mut expired = entry();
        expired.trust_manifest = Some(serde_json::json!({"expiresAt": "2026-01-01"}));
        let findings = validate_catalog(&catalog_with(expired), None, "2026-11-21", NOW);
        assert!(findings.iter().any(|f| f.kind == "trust-manifest-expired"));

        let mut soon = entry();
        soon.trust_manifest = Some(serde_json::json!({"manifest": {"exp": NOW + 10 * 86_400}}));
        let findings = validate_catalog(&catalog_with(soon), None, "2026-11-21", NOW);
        assert!(findings.iter().any(|f| f.kind == "trust-manifest-expiring"));

        let mut sound = entry();
        sound.trust_manifest = Some(serde_json::json!({"expiresAt": "2027-06-01"}));
        let findings = validate_catalog(&catalog_with(sound), None, "2026-11-21", NOW);
        assert!(findings.is_empty(), "findings were {findings:?}");
    }
}
