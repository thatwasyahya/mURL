//! Strict JSON parsing for manifests: standard JSON, with **duplicate
//! object members rejected** at every level.
//!
//! Why this exists (threat T-15): `{"target":"https://safe","target":
//! "file:///etc/passwd"}` is legal JSON that different parsers interpret
//! differently (first-wins vs last-wins). Inside one implementation that is
//! survivable — verification and interpretation share a parser — but across
//! implementations it is a signature-confusion vector: two conformant
//! consumers could verify the same signature yet *act* on different targets.
//! The v0.2 format therefore makes duplicates invalid outright, and this
//! parser is the enforcement point: it fails before a `Value` ever exists.
//!
//! Built as a `DeserializeSeed` over serde_json's deserializer, so its
//! recursion-depth protection (default 128) still applies.

use std::fmt;

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Parse JSON bytes into a [`Value`], rejecting duplicate object members.
pub fn from_slice_strict(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = NoDupSeed.deserialize(&mut de)?;
    de.end()?;
    Ok(value)
}

struct NoDupSeed;

impl<'de> DeserializeSeed<'de> for NoDupSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NoDupVisitor)
    }
}

struct NoDupVisitor;

impl<'de> Visitor<'de> for NoDupVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        // Floats are structurally parseable but invalid in manifests; the
        // validator reports them with a path. Non-finite is malformed JSON
        // anyway.
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite number"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(NoDupSeed)? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(NoDupSeed)?;
            if out.insert(key.clone(), value).is_some() {
                return Err(A::Error::custom(format!("duplicate object member `{key}`")));
            }
        }
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_normal_json() {
        let v = from_slice_strict(br#"{"a":1,"b":[true,null,"x"],"c":{"d":2}}"#).unwrap();
        assert_eq!(v, json!({"a":1,"b":[true,null,"x"],"c":{"d":2}}));
    }

    #[test]
    fn rejects_top_level_duplicates() {
        let err =
            from_slice_strict(br#"{"target":"https://safe","target":"file:///etc"}"#).unwrap_err();
        assert!(
            err.to_string().contains("duplicate object member `target`"),
            "{err}"
        );
    }

    #[test]
    fn rejects_nested_duplicates() {
        assert!(from_slice_strict(br#"{"a":{"k":1,"k":2}}"#).is_err());
        assert!(from_slice_strict(br#"{"a":[{"k":1},{"k":1,"k":2}]}"#).is_err());
    }

    #[test]
    fn distinct_keys_across_objects_are_fine() {
        assert!(from_slice_strict(br#"[{"k":1},{"k":2}]"#).is_ok());
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(from_slice_strict(br#"{"a":1} extra"#).is_err());
    }

    #[test]
    fn depth_limit_still_applies() {
        let mut s = String::new();
        for _ in 0..200 {
            s.push('[');
        }
        for _ in 0..200 {
            s.push(']');
        }
        assert!(from_slice_strict(s.as_bytes()).is_err());
    }
}
