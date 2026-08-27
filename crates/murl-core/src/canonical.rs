//! mURL Canonical Form 1 (MCF-1) for JSON documents.
//!
//! Signatures and integrity hashes need a deterministic byte representation
//! of a manifest. MCF-1 is a deliberately small canonicalization:
//!
//! * Object members sorted by Unicode code point of the member name
//!   (byte order of UTF-8, which is identical).
//! * No insignificant whitespace.
//! * String escaping: `\"`, `\\`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\u00XX`
//!   for remaining control characters; all other characters emitted as raw
//!   UTF-8.
//! * Numbers MUST be integers representable in `i64`/`u64`, emitted in
//!   decimal with no exponent, no fraction, no leading zeros. Non-integer
//!   numbers are a hard error.
//!
//! Relation to RFC 8785 (JCS): MCF-1 output is byte-identical to JCS for
//! every document the manifest schema allows (ASCII member names, integer
//! numbers). It differs from JCS in two deliberate restrictions: member names
//! are sorted by code point rather than UTF-16 code unit (identical for BMP
//! member names, which the schema requires in practice), and non-integer
//! numbers are rejected instead of being formatted with ECMAScript rules.
//! Rejecting floats removes the single hardest-to-reimplement part of JCS and
//! with it an entire class of cross-implementation signature mismatches.

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CanonicalError {
    #[error("canonical form forbids non-integer numbers (found {0})")]
    NonIntegerNumber(String),
}

/// Serialize a JSON value into MCF-1 canonical bytes.
pub fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_value(value: &Value, out: &mut String) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                return Err(CanonicalError::NonIntegerNumber(n.to_string()));
            }
        }
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // serde_json's default Map is a BTreeMap: iteration is already
            // sorted by key (byte order == code point order for UTF-8).
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(v, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_keys_and_strips_whitespace() {
        let v: Value =
            serde_json::from_str(r#"{ "b": 1, "a": { "z": [1, 2], "y": null } }"#).unwrap();
        let c = String::from_utf8(canonical_json_bytes(&v).unwrap()).unwrap();
        assert_eq!(c, r#"{"a":{"y":null,"z":[1,2]},"b":1}"#);
    }

    #[test]
    fn escapes_exactly_like_jcs() {
        let v = json!({"s": "a\"b\\c\nd\te\u{0001}f"});
        let c = String::from_utf8(canonical_json_bytes(&v).unwrap()).unwrap();
        assert_eq!(c, "{\"s\":\"a\\\"b\\\\c\\nd\\te\\u0001f\"}");
    }

    #[test]
    fn preserves_unicode_raw() {
        let v = json!({"n": "café ☕"});
        let c = String::from_utf8(canonical_json_bytes(&v).unwrap()).unwrap();
        assert_eq!(c, "{\"n\":\"café ☕\"}");
    }

    #[test]
    fn rejects_floats() {
        let v = json!({"x": 1.5});
        assert!(canonical_json_bytes(&v).is_err());
    }

    #[test]
    fn accepts_integer_extremes() {
        let v = json!({"a": i64::MIN, "b": u64::MAX});
        let c = String::from_utf8(canonical_json_bytes(&v).unwrap()).unwrap();
        assert_eq!(c, format!("{{\"a\":{},\"b\":{}}}", i64::MIN, u64::MAX));
    }

    #[test]
    fn deterministic_across_input_orderings() {
        let a: Value = serde_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"y":2,"x":1}"#).unwrap();
        assert_eq!(canonical_json_bytes(&a), canonical_json_bytes(&b));
    }
}
