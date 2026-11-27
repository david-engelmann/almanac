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
use crate::trust::{
    PinObservation, PinOutcome, SignatureVerdict, TrustKey, is_envelope, verify_detached_jws,
};

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
/// so rather than passing it silently. `pins` are the
/// run's attestation observations when it engaged the
/// pin layer; without them the pin layer makes no claim.
pub fn validate_catalog_with_keys(
    published: &Catalog,
    legacy: Option<&Catalog>,
    today: &str,
    now_unix: i64,
    keys: &[TrustKey],
    pins: Option<&[PinObservation]>,
) -> Vec<Finding> {
    let mut findings = validate_catalog(published, legacy, today, now_unix);
    // The plain pass already judged the singular
    // manifests' expiry, so the shared pass skips it.
    envelope_pass(
        published,
        today,
        now_unix,
        Some(keys),
        pins,
        false,
        &mut findings,
    );
    sort_findings(&mut findings);
    findings
}

/// The envelope findings a check run assembles over
/// the published catalog: expiry and the envelope
/// layers for every manifest an entry carries, the
/// singular manifest's expiry included, since check
/// runs no plain validation pass to have judged it.
/// `keys` is the run's key set when it holds one;
/// without one the signature layer makes no claim,
/// the scoping the check integration design records.
/// `pins` is the run's attestation observations when
/// it engaged the pin layer; without them the pin
/// layer likewise makes no claim.
/// Findings come back in the report's deterministic
/// order, as the validate entry points return them.
/// The removal judgment against the baseline is the
/// drift module's, beside the other baseline classes.
pub fn envelope_findings(
    published: &Catalog,
    today: &str,
    now_unix: i64,
    keys: Option<&[TrustKey]>,
    pins: Option<&[PinObservation]>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    envelope_pass(published, today, now_unix, keys, pins, true, &mut findings);
    sort_findings(&mut findings);
    findings
}

/// The one envelope pass both verification surfaces
/// run, so a manifest is judged identically wherever
/// it surfaces: validate's keyed pass over the plain
/// pass's findings, and check's trust pass standing
/// alone. `judge_singular_expiry` is set when no
/// plain pass ran ahead of this one to judge the
/// singular manifests' expiry. A manifest repeated
/// across the slots, or twice in the array, is
/// judged once, in its first slot.
fn envelope_pass(
    published: &Catalog,
    today: &str,
    now_unix: i64,
    keys: Option<&[TrustKey]>,
    pins: Option<&[PinObservation]>,
    judge_singular_expiry: bool,
    findings: &mut Vec<Finding>,
) {
    for entry in &published.entries {
        // Identity binds to the publisher segment of the
        // entry's own URN, the draft's section 3.5.1 rule;
        // the host domain stands in only when the URN does
        // not parse, a case the plain pass already judges.
        let publisher = urn_publisher(&entry.identifier)
            .unwrap_or(published.host.domain.as_str())
            .to_string();
        let mut judged: Vec<&serde_json::Value> = Vec::new();
        if let Some(manifest) = &entry.trust_manifest {
            if judge_singular_expiry {
                validate_trust_manifest(manifest, &entry.identifier, today, now_unix, findings);
            }
            verify_envelope(manifest, entry, &publisher, keys, pins, false, findings);
            judged.push(manifest);
        }
        for manifest in &entry.trust_manifests {
            if judged.contains(&manifest) {
                continue;
            }
            judged.push(manifest);
            validate_trust_manifest(manifest, &entry.identifier, today, now_unix, findings);
            verify_envelope(manifest, entry, &publisher, keys, pins, true, findings);
        }
    }
}

/// The publisher segment of a domain anchored URN,
/// urn:air:<publisher>:..., when the identifier has
/// that shape.
pub fn urn_publisher(identifier: &str) -> Option<&str> {
    let rest = identifier.strip_prefix("urn:air:")?;
    let segment = rest.split(':').next()?;
    if segment.is_empty() {
        None
    } else {
        Some(segment)
    }
}

/// The host a url names, lowercased, without scheme,
/// userinfo, or port; None when the url carries no
/// authority to name one. An IPv6 literal compares
/// as written inside its brackets. The parse follows
/// the WHATWG authority rules where they decide the
/// host: a backslash terminates the authority for
/// the special schemes, and slashes before the
/// authority are skipped, so a spelling cannot name
/// one host to this parser and another to a browser.
/// A single trailing dot, the DNS root, is dropped.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let rest = rest.trim_start_matches(['/', '\\']);
    let authority = rest.split(['/', '\\', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next()?
    } else {
        host_port.split(':').next()?
    };
    let host = if host.contains(':') {
        host
    } else {
        host.strip_suffix('.').unwrap_or(host)
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// The host an entry's url names when the entry
/// claims a resource cross-origin: the entry carries
/// a publisher envelope in the singular slot, names
/// a url, and the url's host differs from the
/// publisher its identifier anchors to (the catalog
/// host, port normalized away, standing in when the
/// identifier is not a domain anchored URN). None
/// for every other entry, which is most of them.
/// The cross-origin judge and its collector share
/// this one gate so the two cannot drift apart.
pub fn cross_origin_host(entry: &crate::catalog::Entry, fallback_domain: &str) -> Option<String> {
    entry
        .trust_manifest
        .as_ref()
        .filter(|m| crate::trust::is_envelope(m))?;
    let url = entry.url.as_deref()?;
    let publisher = match urn_publisher(&entry.identifier) {
        Some(segment) => segment.to_lowercase(),
        None => url_host(&format!("https://{fallback_domain}"))?,
    };
    let host = url_host(url)?;
    if host == publisher { None } else { Some(host) }
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
/// the draft's reject cases), then the attestations
/// (judge_attestations), then the signature: invalid
/// is critical, and a signature this run cannot
/// attempt is a minor naming why, never a silent pass.
/// A keyless run makes one further judgment at that
/// exit: a plural envelope claiming the publisher's
/// own identity draws a minor naming that its
/// authorship went unverified in the run, the
/// thirty-sixth slice's reporting closure.
#[allow(clippy::too_many_arguments)]
fn verify_envelope(
    manifest: &serde_json::Value,
    entry: &Entry,
    publisher: &str,
    keys: Option<&[TrustKey]>,
    pins: Option<&[PinObservation]>,
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
    judge_attestations(manifest, entry, pins, findings);
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
    // The verdict runs only in a run that holds keys.
    // A run without them judges the envelope's shape
    // and bindings and makes no signature claim at all,
    // the check gate's scoping in the design record.
    let Some(keys) = keys else {
        // One judgment the keyless run still makes: a
        // plural envelope whose identity binds to the
        // publisher presents itself as the publisher's
        // own attestation, and with no keys its
        // authorship rests on nothing this run
        // examined. The run names that as a minor
        // rather than passing it in silence, the
        // thirty-sixth slice's reporting closure; a
        // foreign identity is a third-party claim,
        // which is what it appears to be, and the
        // singular envelope's identity binding is
        // judged in every run above.
        if third_party
            && let Some(identity) = identity
            && identity_binds(identity, publisher)
        {
            findings.push(finding(
                Severity::Minor,
                "trust-manifest-identity-unverified",
                Some(id),
                format!(
                    "an attestation in the plural slot claims the publisher identity {identity}, and this run holds no keys to verify its authorship"
                ),
            ));
        }
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

/// The attestation judgments over one envelope, the
/// draft's sections 3.5.2 and 6.4. Item shape at major
/// in every run: attestations that are not an array,
/// an item that is not an object, an item missing its
/// type or its uri, a digest with no uri to fetch, and
/// a digest outside the <alg>:<hex> form. The pin
/// itself is judged only in a run holding observations:
/// a fetched document whose sha256 differs from the
/// sealed digest is critical, the substitution the pin
/// exists to catch, and a pin the run could not check
/// (a failed fetch, an algorithm this build does not
/// compute) is a minor naming why, never a silent
/// pass. An item without a digest carries no pin and
/// is judged on its statement's subject claim alone,
/// as judge_statement records; a malformed digest is
/// a shape finding and the item is still judged on
/// that claim, so a broken pin never shields the
/// statement behind it.
fn judge_attestations(
    manifest: &serde_json::Value,
    entry: &Entry,
    pins: Option<&[PinObservation]>,
    findings: &mut Vec<Finding>,
) {
    let id = entry.identifier.as_str();
    let Some(attestations) = manifest.get("attestations") else {
        return;
    };
    let Some(items) = attestations.as_array() else {
        findings.push(finding(
            Severity::Major,
            "trust-manifest-shape",
            Some(id),
            "trust manifest attestations are not an array".into(),
        ));
        return;
    };
    // An item repeated verbatim in one array is one
    // claim, judged once; the pass's manifest dedupe
    // covers repetition across slots, this covers it
    // within the array.
    let mut judged_items: Vec<&serde_json::Value> = Vec::new();
    for item in items {
        if judged_items.contains(&item) {
            continue;
        }
        judged_items.push(item);
        let Some(item) = item.as_object() else {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(id),
                "an attestation in the trust manifest is not an object".into(),
            ));
            continue;
        };
        if item
            .get("type")
            .and_then(|v| v.as_str())
            .is_none_or(|text| text.trim().is_empty())
        {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(id),
                "an attestation in the trust manifest carries no type".into(),
            ));
        }
        let uri = item
            .get("uri")
            .and_then(|v| v.as_str())
            .filter(|text| !text.trim().is_empty());
        let digest = item.get("digest");
        let Some(uri) = uri else {
            findings.push(finding(
                Severity::Major,
                "trust-manifest-shape",
                Some(id),
                if digest.is_some() {
                    "an attestation in the trust manifest carries a digest but no uri".into()
                } else {
                    "an attestation in the trust manifest carries no uri".into()
                },
            ));
            continue;
        };
        // The digest, when the item carries one, must
        // be a well-formed pin before it is compared.
        // A malformed digest is a shape finding above
        // its own item and the pin simply does not
        // stand; the item still reaches the statement
        // judgment below, as an item carrying no digest
        // does.
        let pin = match digest {
            None => None,
            Some(digest) => {
                let as_str = digest.as_str();
                if as_str.is_none() {
                    findings.push(finding(
                        Severity::Major,
                        "trust-manifest-shape",
                        Some(id),
                        "an attestation digest in the trust manifest is not a string".into(),
                    ));
                }
                let well_formed = as_str.and_then(|digest| {
                    digest.split_once(':').filter(|(alg, hex)| {
                        !alg.is_empty()
                            && !hex.is_empty()
                            && hex.chars().all(|c| c.is_ascii_hexdigit())
                    })
                });
                match (as_str, well_formed) {
                    (Some(digest), Some((alg, hex))) => Some((digest, alg, hex)),
                    (Some(digest), None) => {
                        findings.push(finding(
                            Severity::Major,
                            "trust-manifest-shape",
                            Some(id),
                            format!("the attestation digest {digest} is not in <alg>:<hex> form"),
                        ));
                        None
                    }
                    _ => None,
                }
            }
        };
        let Some(pins) = pins else {
            continue;
        };
        match pins
            .iter()
            .find(|o| o.entry_id == id && o.uri == uri)
            .map(|o| &o.outcome)
        {
            Some(PinOutcome::Fetched {
                sha256: served,
                text,
            }) => {
                if let Some((digest, alg, hex)) = pin {
                    if alg != "sha256" {
                        findings.push(finding(
                            Severity::Minor,
                            "trust-manifest-attestation-unverified",
                            Some(id),
                            format!(
                                "the attestation pin for {uri} names {alg}, and this build computes sha256 only"
                            ),
                        ));
                    } else if !served.eq_ignore_ascii_case(hex) {
                        findings.push(finding(
                            Severity::Critical,
                            "trust-manifest-attestation-mismatch",
                            Some(id),
                            format!(
                                "the attestation document at {uri} hashes to sha256:{served}, not the pinned {digest}"
                            ),
                        ));
                    }
                }
                judge_statement(text, entry, uri, findings);
            }
            Some(PinOutcome::Failed(reason)) => findings.push(finding(
                Severity::Minor,
                "trust-manifest-attestation-unverified",
                Some(id),
                format!("the attestation document at {uri} could not be fetched ({reason})"),
            )),
            None => findings.push(finding(
                Severity::Minor,
                "trust-manifest-attestation-unverified",
                Some(id),
                format!("the attestation document at {uri} was not fetched"),
            )),
        }
    }
}

/// The statement layer of the attestation judgments,
/// the subject binding of the draft's sections
/// 3.5.2 and 6.4 as the attestation statement
/// section of DECISIONS.md records it. The fetched
/// statement's subject claims are read against this
/// entry: a claimed digest equal to the entry's
/// digest, or a claimed name equal to its identifier
/// or its url, binds the statement to the entry. A
/// statement claiming a different subject on a
/// channel this entry can answer (names against its
/// identifier or url, digests against a digest it
/// records) is the transplant those sections exist
/// to prevent, at critical. A statement carrying no
/// subject claim this build can read, or claiming
/// only digests to an entry that records none, is
/// unverified at minor, never silently honored. The
/// statement's own signature is the attester's and
/// is not judged.
fn judge_statement(text: &str, entry: &Entry, uri: &str, findings: &mut Vec<Finding>) {
    let id = entry.identifier.as_str();
    let claims = crate::statement::reduce_statement(text);
    let digest_binds = entry.digest.as_deref().is_some_and(|own| {
        let own = own
            .split_once(':')
            .map(|(alg, hex)| crate::statement::normalize_digest(alg, hex))
            .unwrap_or_else(|| own.to_string());
        claims
            .digests
            .iter()
            .any(|claimed| claimed.eq_ignore_ascii_case(&own))
    });
    let name_binds = claims
        .names
        .iter()
        .any(|name| name == id || entry.url.as_deref() == Some(name.as_str()));
    if digest_binds || name_binds {
        return;
    }
    if claims.digests.is_empty() && claims.names.is_empty() {
        findings.push(finding(
            Severity::Minor,
            "trust-manifest-attestation-unverified",
            Some(id),
            format!(
                "the attestation statement at {uri} carries no subject claim this build can read"
            ),
        ));
    } else if claims.names.is_empty() && entry.digest.is_none() {
        findings.push(finding(
            Severity::Minor,
            "trust-manifest-attestation-unverified",
            Some(id),
            format!(
                "the attestation statement at {uri} claims a digest this entry does not record"
            ),
        ));
    } else {
        let claimed = claims
            .names
            .first()
            .map(|name| format!("subject {name}"))
            .or_else(|| {
                claims
                    .digests
                    .first()
                    .map(|digest| format!("subject digest {digest}"))
            })
            .unwrap_or_default();
        findings.push(finding(
            Severity::Critical,
            "trust-manifest-attestation-unbound",
            Some(id),
            format!(
                "the attestation statement at {uri} claims {claimed}, which does not bind {id}"
            ),
        ));
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
            None,
        );
        assert!(findings.is_empty(), "findings were {findings:?}");
    }

    #[test]
    fn a_signed_envelope_without_a_key_reports_unverified_never_silent() {
        let findings = validate_catalog_with_keys(
            &catalog_with(signed_entry()),
            None,
            "2026-11-21",
            NOW,
            &[],
            None,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "trust-manifest-signature-unverified");
        assert_eq!(findings[0].severity, Severity::Minor);
        // The plain pass judges no envelope layer at
        // all; the layers are the keyed pass's, and
        // check's through envelope_findings.
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
            None,
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
    fn check_envelope_findings_judge_bindings_without_keys_and_verdicts_with() {
        // The check gate's scoping: without keys the
        // envelope layers judge shape, bindings and
        // expiry and make no signature claim; with keys
        // the verdict joins, and the fixture's signature
        // is valid, so a keyed run is clean.
        let catalog = catalog_with(signed_entry());
        let unkeyed = envelope_findings(&catalog, "2026-11-25", NOW, None, None);
        assert!(
            unkeyed
                .iter()
                .all(|f| !f.kind.starts_with("trust-manifest-signature")),
            "{unkeyed:?}"
        );
        let keyed = envelope_findings(&catalog, "2026-11-25", NOW, Some(&fixture_keys()), None);
        assert!(keyed.is_empty(), "{keyed:?}");
        // A tampered subject is a binding failure in
        // both scopings.
        let mut tampered = signed_entry();
        tampered.trust_manifest.as_mut().unwrap()["subject"]["digest"] =
            serde_json::json!(format!("sha256:{}", "b".repeat(64)));
        let unkeyed = envelope_findings(&catalog_with(tampered), "2026-11-25", NOW, None, None);
        assert!(
            unkeyed
                .iter()
                .any(|f| f.kind == "trust-manifest-subject-mismatch"),
            "{unkeyed:?}"
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
            None,
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
            validate_catalog_with_keys(&catalog_with(third), None, "2026-11-21", NOW, &[], None);
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unbound"),
            "findings were {findings:?}"
        );
    }

    #[test]
    fn a_publisher_claim_in_the_plural_slot_is_named_when_unkeyed() {
        // The thirty-sixth slice's reporting closure:
        // a plural envelope whose identity binds to the
        // publisher, in a run holding no keys, draws the
        // identity-unverified minor; keyed, foreign, and
        // singular cases draw none.
        let mut claimed = entry();
        claimed.trust_manifests = vec![serde_json::json!({
            "identity": "did:web:example.com",
            "subject": {
                "identifier": "urn:air:example.com:mcp:demo",
                "type": "application/mcp-server-card+json",
                "digest": format!("sha256:{}", "a".repeat(64)),
            },
            "signature": FIXTURE_SIGNATURE,
        })];
        let findings = envelope_findings(&catalog_with(claimed), "2026-11-21", NOW, None, None);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unverified"
                    && f.severity == Severity::Minor),
            "findings were {findings:?}"
        );

        // The same envelope as the singular manifest is
        // the publisher's own slot; its identity binding
        // is judged in every run and the closure does
        // not speak for it.
        let findings =
            envelope_findings(&catalog_with(signed_entry()), "2026-11-21", NOW, None, None);
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unverified"),
            "findings were {findings:?}"
        );

        // A foreign identity in the plural slot is a
        // third-party attestation and draws nothing.
        let mut foreign = entry();
        foreign.trust_manifests = vec![serde_json::json!({
            "identity": "did:web:auditor.example.org",
            "subject": {
                "identifier": "urn:air:example.com:mcp:demo",
                "type": "application/mcp-server-card+json",
                "digest": format!("sha256:{}", "a".repeat(64)),
            },
            "signature": FIXTURE_SIGNATURE,
        })];
        let findings = envelope_findings(&catalog_with(foreign), "2026-11-21", NOW, None, None);
        assert!(
            !findings
                .iter()
                .any(|f| f.kind == "trust-manifest-identity-unverified"),
            "findings were {findings:?}"
        );

        // In a keyed run the signature layer speaks for
        // the envelope's authorship, and the closure is
        // silent: the exact signed manifest standing in
        // the plural slot verifies under its key.
        let mut keyed = entry();
        keyed.trust_manifests = vec![
            signed_entry()
                .trust_manifest
                .expect("the signed fixture carries its manifest"),
        ];
        let findings = validate_catalog_with_keys(
            &catalog_with(keyed),
            None,
            "2026-11-21",
            NOW,
            &fixture_keys(),
            None,
        );
        assert!(findings.is_empty(), "findings were {findings:?}");
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

    #[test]
    fn attestation_shape_stands_always_and_pins_only_in_engaged_runs() {
        let uri = "https://example.com/attestations/slsa.json";
        // The served document is a statement binding
        // this entry by its digest, so the statement
        // layer is silent wherever this test does not
        // exercise it.
        let document = serde_json::json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [{
                "name": "demo",
                "digest": {"sha256": "a".repeat(64)},
            }],
            "predicateType": "https://slsa.dev/provenance/v1",
        })
        .to_string();
        let served = crate::hash::sha256_hex(document.as_bytes());
        let fetched = |sha256: String, text: String| PinOutcome::Fetched { sha256, text };
        let mut pinned = signed_entry();
        pinned.trust_manifest.as_mut().unwrap()["attestations"] = serde_json::json!([{
            "type": "https://slsa.dev/provenance/v1",
            "uri": uri,
            "digest": format!("sha256:{served}"),
        }]);
        let catalog = catalog_with(pinned);
        let observation = |outcome| {
            vec![PinObservation {
                entry_id: "urn:air:example.com:mcp:demo".into(),
                uri: uri.into(),
                outcome,
            }]
        };
        // Not engaged, the pin is silent.
        let silent = envelope_findings(&catalog, "2026-11-25", NOW, None, None);
        assert!(
            silent.iter().all(|f| !f.kind.contains("attestation")),
            "{silent:?}"
        );
        // Engaged, the served document matches its pin.
        let matched = envelope_findings(
            &catalog,
            "2026-11-25",
            NOW,
            None,
            Some(&observation(fetched(served.clone(), document.clone()))),
        );
        assert!(
            matched.iter().all(|f| !f.kind.contains("attestation")),
            "{matched:?}"
        );
        // Engaged, the uri serves another document: the
        // substitution the pin exists to catch, critical.
        let swapped = envelope_findings(
            &catalog,
            "2026-11-25",
            NOW,
            None,
            Some(&observation(fetched(
                crate::hash::sha256_hex(b"a different document"),
                "a different document".into(),
            ))),
        );
        assert!(
            swapped
                .iter()
                .any(|f| f.kind == "trust-manifest-attestation-mismatch"
                    && f.severity == Severity::Critical),
            "{swapped:?}"
        );
        // Engaged, the fetch failed: a minor naming why.
        let failed = envelope_findings(
            &catalog,
            "2026-11-25",
            NOW,
            None,
            Some(&observation(PinOutcome::Failed("it timed out".into()))),
        );
        assert!(
            failed
                .iter()
                .any(|f| f.kind == "trust-manifest-attestation-unverified"
                    && f.severity == Severity::Minor),
            "{failed:?}"
        );
        // Shape stands in every run: a digest with no
        // uri and a digest outside the pinned form are
        // majors; an item with no digest draws no shape
        // finding (in an engaged run it is fetched and
        // judged on its statement, the next test's
        // ground).
        let mut shapeless = signed_entry();
        shapeless.trust_manifest.as_mut().unwrap()["attestations"] = serde_json::json!([
            { "type": "https://slsa.dev/provenance/v1", "digest": format!("sha256:{served}") },
            { "type": "https://slsa.dev/provenance/v1", "uri": uri, "digest": "not-a-digest" },
            { "type": "https://slsa.dev/provenance/v1", "uri": "https://example.com/a.json" },
        ]);
        let shaped = envelope_findings(&catalog_with(shapeless), "2026-11-25", NOW, None, None);
        assert_eq!(
            shaped
                .iter()
                .filter(|f| f.kind == "trust-manifest-shape")
                .count(),
            2,
            "{shaped:?}"
        );
        // Attestations that are not an array are the
        // same major, and a pin naming an algorithm
        // this build does not compute reports
        // unverified in an engaged run, silent in an
        // unengaged one.
        let mut not_array = signed_entry();
        not_array.trust_manifest.as_mut().unwrap()["attestations"] =
            serde_json::json!("not an array");
        let findings = envelope_findings(&catalog_with(not_array), "2026-11-25", NOW, None, None);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == "trust-manifest-shape" && f.severity == Severity::Major),
            "{findings:?}"
        );
        let mut other_alg = signed_entry();
        other_alg.trust_manifest.as_mut().unwrap()["attestations"] = serde_json::json!([{
            "type": "https://slsa.dev/provenance/v1",
            "uri": uri,
            "digest": format!("sha512:{served}"),
        }]);
        let engaged = envelope_findings(
            &catalog_with(other_alg.clone()),
            "2026-11-25",
            NOW,
            None,
            Some(&observation(fetched(served.clone(), document.clone()))),
        );
        assert!(
            engaged
                .iter()
                .any(|f| f.kind == "trust-manifest-attestation-unverified"
                    && f.severity == Severity::Minor),
            "{engaged:?}"
        );
        let unengaged = envelope_findings(&catalog_with(other_alg), "2026-11-25", NOW, None, None);
        assert!(
            unengaged.iter().all(|f| !f.kind.contains("attestation")),
            "{unengaged:?}"
        );
    }

    #[test]
    fn attestation_statements_bind_by_their_subject_claims() {
        // The statement layer over a statement-only
        // item (a uri, no pin): a statement binding by
        // digest, by identifier, or by the entry's url
        // is silent; a statement claiming another
        // subject is the transplant at critical; a
        // statement with no readable claim is
        // unverified at minor; and none of it speaks
        // in an unengaged run.
        let uri = "https://example.com/attestations/slsa.json";
        let mut stated = signed_entry();
        stated.trust_manifest.as_mut().unwrap()["attestations"] = serde_json::json!([{
            "type": "https://slsa.dev/provenance/v1",
            "uri": uri,
        }]);
        let catalog = catalog_with(stated);
        let run = |text: &str| {
            let observations = vec![PinObservation {
                entry_id: "urn:air:example.com:mcp:demo".into(),
                uri: uri.into(),
                outcome: PinOutcome::Fetched {
                    sha256: crate::hash::sha256_hex(text.as_bytes()),
                    text: text.into(),
                },
            }];
            envelope_findings(&catalog, "2026-11-25", NOW, None, Some(&observations))
        };
        let by_digest = run(&serde_json::json!({
            "subject": [{"name": "demo", "digest": {"sha256": "a".repeat(64)}}],
        })
        .to_string());
        assert!(
            by_digest.iter().all(|f| !f.kind.contains("attestation")),
            "{by_digest:?}"
        );
        let by_identifier = run(&serde_json::json!({
            "subject": {"identifier": "urn:air:example.com:mcp:demo", "digest": "sha256:ff"},
        })
        .to_string());
        assert!(
            by_identifier
                .iter()
                .all(|f| !f.kind.contains("attestation")),
            "{by_identifier:?}"
        );
        let by_url = run(&serde_json::json!({
            "subject": [{"name": "https://example.com/mcp", "digest": {"sha256": "ff"}}],
        })
        .to_string());
        assert!(
            by_url.iter().all(|f| !f.kind.contains("attestation")),
            "{by_url:?}"
        );
        let transplant = run(&serde_json::json!({
            "subject": [{"name": "someone else", "digest": {"sha256": "b".repeat(64)}}],
        })
        .to_string());
        assert!(
            transplant.iter().any(|f| {
                f.kind == "trust-manifest-attestation-unbound" && f.severity == Severity::Critical
            }),
            "{transplant:?}"
        );
        let unreadable = run("a PDF compliance report, as served");
        assert!(
            unreadable.iter().any(|f| {
                f.kind == "trust-manifest-attestation-unverified" && f.severity == Severity::Minor
            }),
            "{unreadable:?}"
        );
        assert!(
            unreadable
                .iter()
                .all(|f| f.kind != "trust-manifest-attestation-unbound"),
            "{unreadable:?}"
        );
        let unengaged = envelope_findings(&catalog, "2026-11-25", NOW, None, None);
        assert!(
            unengaged.iter().all(|f| !f.kind.contains("attestation")),
            "{unengaged:?}"
        );
        // A statement claiming only digests, to an
        // entry recording none, cannot be answered
        // either way: unverified, not the transplant.
        let mut digestless = signed_entry();
        digestless.digest = None;
        digestless.trust_manifest.as_mut().unwrap()["attestations"] = serde_json::json!([{
            "type": "https://slsa.dev/provenance/v1",
            "uri": uri,
        }]);
        let text = serde_json::json!({
            "subject": [{"digest": {"sha256": "b".repeat(64)}}],
        })
        .to_string();
        let observations = vec![PinObservation {
            entry_id: "urn:air:example.com:mcp:demo".into(),
            uri: uri.into(),
            outcome: PinOutcome::Fetched {
                sha256: crate::hash::sha256_hex(text.as_bytes()),
                text,
            },
        }];
        let unanswered = envelope_findings(
            &catalog_with(digestless),
            "2026-11-25",
            NOW,
            None,
            Some(&observations),
        );
        assert!(
            unanswered.iter().any(|f| {
                f.kind == "trust-manifest-attestation-unverified" && f.severity == Severity::Minor
            }),
            "{unanswered:?}"
        );
        assert!(
            unanswered
                .iter()
                .all(|f| f.kind != "trust-manifest-attestation-unbound"),
            "{unanswered:?}"
        );
    }
}
