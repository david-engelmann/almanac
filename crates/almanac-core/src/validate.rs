//! Catalog validation beyond schema.
//!
//! Schema says a catalog parses. Validation says it holds
//! together. Every entry anchored to the publisher's domain,
//! exactly one of url or data per entry, digests in the draft's
//! format, representative queries in the count the draft asks
//! for, the dual path pair in agreement, and a trust manifest
//! whose shape and expiry hold up. Amendment D fixes the trust
//! manifest scope for version 1, shape and expiry only, no
//! signature verification and no key management.
//!
//! The function is pure. The CLI reads the files and passes the
//! current date and time in, so the same catalog always produces
//! the same findings on the same day.

use crate::Severity;
use crate::catalog::Catalog;
use crate::drift::Finding;

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

    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.entry.cmp(&b.entry))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    findings
}

/// Shape and expiry for one trust manifest. The expiry is read
/// where version 1 knows to look, an expiresAt date on the
/// manifest or on its inner manifest document, or an exp claim
/// in seconds. Anything else about the manifest, its signature
/// above all, is outside Amendment D's scope and is not judged.
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
