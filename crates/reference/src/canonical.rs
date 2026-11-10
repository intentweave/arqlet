//! Canonical JSON: the exact bytes that durable hashes are computed over.
//!
//! Rules of `CANONICAL_FORMAT` 1; a change to any rule needs a new format number and new fixtures:
//! - object keys in code-point order, no insignificant whitespace;
//! - unsigned and signed integers in plain decimal;
//! - finite floats: zero (including negative zero) as `0`; integral values below 2^53 as plain integers, so `1.0`
//!   and `1` are one value; every other float as the shortest decimal that parses back to the same `f64`, in the
//!   notation `serde_json` writes (`1e+21`, the same exponent form as JavaScript);
//! - strings with the escaping of `serde_json`.
//!
//! Non-finite numbers cannot be represented in `serde_json::Value`, so they are rejected when the value is built.

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const CANONICAL_FORMAT: u32 = 1;

const SAFE_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53

/// The canonical text of a JSON value.
pub fn canonical_json(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_value(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => write_number(number, out)?,
        Value::String(text) => {
            out.push_str(&serde_json::to_string(text).map_err(|e| e.to_string())?)
        }
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
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).map_err(|e| e.to_string())?);
                out.push(':');
                write_value(&map[key.as_str()], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_number(number: &serde_json::Number, out: &mut String) -> Result<(), String> {
    if let Some(unsigned) = number.as_u64() {
        out.push_str(&unsigned.to_string());
        return Ok(());
    }
    if let Some(signed) = number.as_i64() {
        out.push_str(&signed.to_string());
        return Ok(());
    }
    let float = number.as_f64().ok_or("number is not representable")?;
    if !float.is_finite() {
        return Err("non-finite numbers are not part of the canonical form".into());
    }
    if float == 0.0 {
        out.push('0');
    } else if float.fract() == 0.0 && float.abs() < SAFE_INTEGER_LIMIT {
        out.push_str(&(float as i64).to_string());
    } else {
        out.push_str(&serde_json::to_string(&float).map_err(|e| e.to_string())?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_keys_are_sorted_and_whitespace_is_removed() {
        let value = json!({"b": [1, {"d": true, "c": null}], "a": "x"});
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"a":"x","b":[1,{"c":null,"d":true}]}"#
        );
    }

    #[test]
    fn integral_floats_equal_integers() {
        assert_eq!(canonical_json(&json!(1.0)).unwrap(), "1");
        assert_eq!(canonical_json(&json!(1)).unwrap(), "1");
        assert_eq!(canonical_json(&json!(-3.0)).unwrap(), "-3");
    }

    #[test]
    fn negative_zero_is_zero() {
        assert_eq!(canonical_json(&json!(-0.0)).unwrap(), "0");
        assert_eq!(canonical_json(&json!(0.0)).unwrap(), "0");
    }

    #[test]
    fn fractions_use_the_shortest_round_trip_decimal() {
        assert_eq!(canonical_json(&json!(0.5)).unwrap(), "0.5");
        assert_eq!(canonical_json(&json!(0.1)).unwrap(), "0.1");
        assert_eq!(canonical_json(&json!(-12.25)).unwrap(), "-12.25");
    }

    #[test]
    fn large_magnitudes_keep_their_value() {
        assert_eq!(
            canonical_json(&json!(u64::MAX)).unwrap(),
            "18446744073709551615"
        );
        assert_eq!(
            canonical_json(&json!(9_007_199_254_740_993u64)).unwrap(),
            "9007199254740993"
        );
        assert_eq!(canonical_json(&json!(1e21)).unwrap(), "1e+21");
        assert_eq!(canonical_json(&json!(1.5e300)).unwrap(), "1.5e+300");
    }

    #[test]
    fn null_and_missing_keys_are_distinct() {
        assert_ne!(
            canonical_json(&json!({"a": null})).unwrap(),
            canonical_json(&json!({})).unwrap()
        );
    }

    #[test]
    fn canonical_text_is_a_fixed_point() {
        let value = json!({"z": [0.1, -0.0, 3.0, "é\"\n"], "a": {"k": u64::MAX}});
        let once = canonical_json(&value).unwrap();
        let reparsed: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(canonical_json(&reparsed).unwrap(), once);
    }

    #[test]
    fn sha256_matches_the_standard_test_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
