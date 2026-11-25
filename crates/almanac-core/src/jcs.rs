//! RFC 8785 JSON Canonicalization Scheme (JCS).
//!
//! The ARD draft seals a trust manifest by signing the
//! JCS serialization of the manifest with its signature
//! member removed, so verification needs the canonical
//! bytes exactly: object members sorted by the UTF-16
//! code units of their names, strings escaped the way
//! ECMAScript escapes them, and numbers serialized by
//! the ECMAScript number algorithm the RFC defers to.
//! That algorithm is the hard edge. The module is proven
//! against the RFC's own example values in its tests
//! before any signature is judged with it.
//!
//! A value this build cannot serialize exactly (a
//! non-finite number, which parsed JSON cannot carry)
//! is refused rather than approximated, and the caller
//! reports the manifest unverified instead of invalid.

use serde_json::Value;

/// Serialize a JSON value to its JCS canonical bytes.
pub fn canonicalize(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_value(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&write_number(number)?),
        Value::String(text) => write_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut members: Vec<(&String, &Value)> = map.iter().collect();
            members.sort_by_cached_key(|(name, _)| utf16_units(name));
            out.push('{');
            for (index, (name, member)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(name, out);
                out.push(':');
                write_value(member, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// A string's UTF-16 code units, the JCS sort key for
/// object member names.
fn utf16_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Serialize a string with the ECMAScript escapes JCS
/// requires: quotation mark, reverse solidus, and the
/// control characters, using the short forms where
/// they exist and a four digit unicode escape, lower
/// case hex, for the rest. Every other character is
/// output as itself, including all non-ASCII.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{0009}' => out.push_str("\\t"),
            '\u{000A}' => out.push_str("\\n"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{000D}' => out.push_str("\\r"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Serialize a number by the ECMAScript algorithm.
/// Integers inside the range a double holds exactly
/// print as themselves; anything else goes through
/// the floating point path, which is also where a
/// large integer lands, matching what a parser that
/// holds numbers as doubles would have read.
fn write_number(number: &serde_json::Number) -> Result<String, String> {
    if let Some(int) = number.as_i64() {
        if int.unsigned_abs() < (1u64 << 53) {
            return Ok(int.to_string());
        }
        return Ok(write_double(int as f64));
    }
    if let Some(int) = number.as_u64() {
        if int < (1u64 << 53) {
            return Ok(int.to_string());
        }
        return Ok(write_double(int as f64));
    }
    let double = number
        .as_f64()
        .ok_or_else(|| "a number this build cannot read".to_string())?;
    if !double.is_finite() {
        return Err("a non-finite number cannot be canonicalized".into());
    }
    Ok(write_double(double))
}

/// The ECMAScript Number::toString serialization, the
/// form JCS adopts. The digits are the shortest string
/// that round trips to the same double. Rust's own
/// float formatting does not always produce them: its
/// digits can run one long, and the RFC's first
/// example is such a value, formatting with a trailing
/// 25 where ECMAScript prints the shorter 3. The ryu
/// crate computes the provably shortest digits, the
/// algorithm ECMAScript implementations use, and this
/// module takes its digits and decimal position from
/// ryu's serialization; the placement of the decimal
/// point and the choice between plain and exponential
/// form are then this module's own, by the ECMAScript
/// rules.
fn write_double(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let negative = value < 0.0;
    let mut buffer = ryu::Buffer::new();
    let (digits, n) = shortest_digits(buffer.format_finite(value.abs()));
    let k = digits.len() as i32;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if (1..=21).contains(&n) && n >= k {
        out.push_str(&digits);
        out.push_str(&"0".repeat((n - k) as usize));
    } else if (1..=21).contains(&n) && n < k {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if (-5..=0).contains(&n) {
        // -6 < n <= 0: leading zeros after the point.
        out.push_str("0.");
        out.push_str(&"0".repeat((-n) as usize));
        out.push_str(&digits);
    } else {
        // Exponential form.
        if k == 1 {
            out.push_str(&digits);
        } else {
            out.push_str(&digits[..1]);
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        let point_exp = n - 1;
        if point_exp >= 0 {
            out.push('+');
        }
        out.push_str(&point_exp.to_string());
    }
    out
}

/// Split a ryu serialization, plain or exponential,
/// into its significant digits and the ECMAScript
/// position n, where the value reads as the digits
/// times 10^(n - digits.len()). Place holding zeros
/// in a plain serialization are not significant: the
/// leading ones fall away, and each trailing one
/// moves into the scale, which leaves n unchanged.
fn shortest_digits(text: &str) -> (String, i32) {
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exp)) => (
            mantissa,
            exp.parse::<i32>().expect("the ryu exponent is an integer"),
        ),
        None => (text, 0),
    };
    let (point, raw) = match mantissa.split_once('.') {
        Some((head, tail)) => (head.len() as i32, format!("{head}{tail}")),
        None => (mantissa.len() as i32, mantissa.to_string()),
    };
    let mut scale = exponent + point - raw.len() as i32;
    let mut digits = raw.trim_start_matches('0');
    while digits.len() > 1 && digits.ends_with('0') {
        digits = &digits[..digits.len() - 1];
        scale += 1;
    }
    let digits = digits.to_string();
    let n = scale + digits.len() as i32;
    (digits, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn canon(value: Value) -> String {
        String::from_utf8(canonicalize(&value).expect("canonicalizes")).expect("utf8")
    }

    #[test]
    fn the_rfc_number_examples_serialize_as_the_rfc_prints_them() {
        // The values of the RFC 8785 appendix example,
        // with the serializations the RFC prints. The
        // first value is parsed from the RFC's printed
        // text rather than written as a float literal:
        // the text is the source, and the literal's
        // precision is the point of the example.
        let cases = [
            (
                serde_json::from_str("333333333.33333329").expect("the RFC value parses"),
                "333333333.3333333",
            ),
            (json!(1e30), "1e+30"),
            (json!(4.50), "4.5"),
            (json!(2e-3), "0.002"),
            (json!(1e-27), "1e-27"),
            (json!(0), "0"),
            (json!(1e21), "1e+21"),
            (json!(1e20), "100000000000000000000"),
            (json!(0.000001), "0.000001"),
            (json!(0.0000001), "1e-7"),
            (json!(-1.5), "-1.5"),
            (json!(1234567890), "1234567890"),
        ];
        for (value, expected) in cases {
            assert_eq!(canon(value), expected);
        }
    }

    #[test]
    fn objects_sort_by_utf16_and_strings_escape_as_ecmascript() {
        // The RFC's string example: the euro sign and
        // the dollar sign pass through, the control
        // characters take their escapes.
        let value = json!({"b": 1, "a": "\u{20ac}$\u{000f}\nA'B\"\\/"});
        assert_eq!(
            canon(value),
            "{\"a\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\/\",\"b\":1}"
        );
        // Sorting is by UTF-16 code units: U+10000,
        // whose first surrogate is 0xD800, sorts before
        // U+E000, whose only unit is 0xE000, although
        // its code point is smaller.
        let value = json!({"\u{e000}": 1, "\u{10000}": 2});
        assert_eq!(canon(value), "{\"\u{10000}\":2,\"\u{e000}\":1}");
    }

    #[test]
    fn the_drafts_example_trust_manifest_canonicalizes() {
        // The trust manifest of the ARD draft's section
        // 3.5 example, with the signature member removed
        // as verification removes it, canonicalized to
        // the bytes a signature over it would seal.
        let manifest = json!({
            "identity": "did:web:example.com",
            "issuedAt": "2026-03-20T14:00:00Z",
            "subject": {
                "identifier": "urn:air:example.com:agent:flight-finder",
                "type": "application/a2a-agent-card+json",
                "version": "1.2.0",
                "digest": "sha256:9f86d081884c7d659a2feaa0c55ad015...",
            },
            "attestations": [{
                "type": "https://slsa.dev/provenance/v1",
                "uri": "https://example.com/attestations/slsa.json",
                "mediaType": "application/vnd.in-toto+json",
                "digest": "sha256:7f83b1657ff1fc53b92dc18148a1d65d...",
            }],
        });
        assert_eq!(
            canon(manifest),
            "{\"attestations\":[{\"digest\":\"sha256:7f83b1657ff1fc53b92dc18148a1d65d...\",\"mediaType\":\"application/vnd.in-toto+json\",\"type\":\"https://slsa.dev/provenance/v1\",\"uri\":\"https://example.com/attestations/slsa.json\"}],\"identity\":\"did:web:example.com\",\"issuedAt\":\"2026-03-20T14:00:00Z\",\"subject\":{\"digest\":\"sha256:9f86d081884c7d659a2feaa0c55ad015...\",\"identifier\":\"urn:air:example.com:agent:flight-finder\",\"type\":\"application/a2a-agent-card+json\",\"version\":\"1.2.0\"}}"
        );
    }

    #[test]
    fn the_rfc_object_example_canonicalizes() {
        let value = json!({
            "numbers": [
                serde_json::from_str::<Value>("333333333.33333329").expect("the RFC value parses"),
                1e30, 4.50, 2e-3, 1e-27,
            ],
            "literals": [null, true, false],
        });
        assert_eq!(
            canon(value),
            "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27]}"
        );
    }
}
