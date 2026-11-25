//! The scalar functions that need no store (provisional, ADR 0021, "The function table"), and the limit on the size of a
//! computed value.
//!
//! A function answers null for a null argument, except where Cypher raises an error (a null position or length). Text is
//! counted and cut in characters (Unicode code points), never in bytes. Where the result in another engine is not clear
//! to us, the call is refused instead of answered differently.

use crate::plan::Function;
use crate::value::Value;
use crate::QueryError;

/// The most a computed value may hold: bytes of text, plus 16 for every value in it (provisional). Text and lists can
/// double with every step of a query, so a short query could ask for any amount of memory without it.
pub const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn too_large() -> QueryError {
    QueryError::limit(format!(
        "stopped: a computed value would hold more than {MAX_VALUE_BYTES} bytes (the value limit)"
    ))
}

/// How much a value holds, counted as [`MAX_VALUE_BYTES`] counts.
pub(crate) fn weight(v: &Value) -> usize {
    match v {
        Value::String(s) => s.len().saturating_add(16),
        Value::List(items) => items
            .iter()
            .fold(16, |sum: usize, item| sum.saturating_add(weight(item))),
        _ => 16,
    }
}

/// Refuses a size beyond the limit. In `u64`: `usize` is 32-bit in WASM.
pub(crate) fn fits(bytes: u64) -> Result<(), QueryError> {
    if bytes > MAX_VALUE_BYTES as u64 {
        Err(too_large())
    } else {
        Ok(())
    }
}

fn wrong(name: &str, wanted: &str, got: &Value) -> QueryError {
    QueryError::runtime(format!("`{name}` expects {wanted}, not a {}", got.kind()))
}

fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str, QueryError> {
    match v {
        Value::String(s) => Ok(s),
        other => Err(wrong(name, "a string", other)),
    }
}

/// A position or a length: an integer of 0 or more. Null is an error, as in Cypher.
fn count(v: &Value, name: &str) -> Result<usize, QueryError> {
    match v {
        Value::Int(n) if *n >= 0 => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
        Value::Int(n) => Err(QueryError::runtime(format!(
            "`{name}` takes a position or length of 0 or more, not {n}"
        ))),
        other => Err(wrong(name, "an integer", other)),
    }
}

fn string(s: String) -> Result<Value, QueryError> {
    fits(s.len() as u64)?;
    Ok(Value::String(s))
}

/// Calls a function of [`Function`] that reads no store; `args` have the number the front end checked.
pub(crate) fn call(function: Function, args: Vec<Value>) -> Result<Value, QueryError> {
    let name = function.name();
    let first = args.first().cloned().unwrap_or(Value::Null);
    // Null in, null out. For `substring`, `left` and `right` only the text decides: their numbers may not be null.
    let null_in = match function {
        Function::Substring | Function::Left | Function::Right => first.is_null(),
        _ => args.iter().any(Value::is_null),
    };
    if null_in {
        return Ok(Value::Null);
    }
    match (function, args.as_slice()) {
        (Function::Size, [Value::String(s)]) => Ok(Value::Int(s.chars().count() as i64)),
        (Function::Size, [Value::List(items)]) => Ok(Value::Int(items.len() as i64)),
        (Function::Size, [other]) => Err(wrong(name, "a string or a list", other)),
        (Function::Reverse, [Value::String(s)]) => string(s.chars().rev().collect()),
        (Function::Reverse, [Value::List(items)]) => {
            Ok(Value::List(items.iter().rev().cloned().collect()))
        }
        (Function::Reverse, [other]) => Err(wrong(name, "a string or a list", other)),
        (Function::ToString, [value]) => to_string(value),
        (Function::ToUpper, [s]) => string(text(s, name)?.to_uppercase()),
        (Function::ToLower, [s]) => string(text(s, name)?.to_lowercase()),
        // Whitespace is what Unicode calls White_Space.
        (Function::Trim, [s]) => string(text(s, name)?.trim().to_string()),
        (Function::LTrim, [s]) => string(text(s, name)?.trim_start().to_string()),
        (Function::RTrim, [s]) => string(text(s, name)?.trim_end().to_string()),
        (Function::Replace, [s, search, with]) => {
            let (s, search, with) = (text(s, name)?, text(search, name)?, text(with, name)?);
            // The size of the result before it is built. An empty `search` matches before every character and at the end.
            let found = s.matches(search).count() as u64;
            let kept = s.len() as u64 - found.saturating_mul(search.len() as u64);
            fits(kept.saturating_add(found.saturating_mul(with.len() as u64)))?;
            string(s.replace(search, with))
        }
        (Function::Split, [s, delimiter]) => {
            let (s, delimiter) = (text(s, name)?, text(delimiter, name)?);
            if delimiter.is_empty() {
                return Err(QueryError::runtime(
                    "`split` with an empty delimiter is not decided yet (ADR 0021)",
                ));
            }
            let parts = s.matches(delimiter).count() as u64 + 1;
            fits(parts.saturating_mul(16).saturating_add(s.len() as u64))?;
            Ok(Value::List(
                s.split(delimiter)
                    .map(|part| Value::String(part.to_string()))
                    .collect(),
            ))
        }
        (Function::Substring, [s, start]) => {
            let (s, start) = (text(s, name)?, count(start, name)?);
            string(s.chars().skip(start).collect())
        }
        (Function::Substring, [s, start, length]) => {
            let (s, start, length) = (text(s, name)?, count(start, name)?, count(length, name)?);
            string(s.chars().skip(start).take(length).collect())
        }
        (Function::Left, [s, length]) => {
            let (s, length) = (text(s, name)?, count(length, name)?);
            string(s.chars().take(length).collect())
        }
        (Function::Right, [s, length]) => {
            let (s, length) = (text(s, name)?, count(length, name)?);
            let skip = s.chars().count().saturating_sub(length);
            string(s.chars().skip(skip).collect())
        }
        // `coalesce` and the functions of nodes and relationships are the executor's; a count of arguments the front end
        // let through wrongly ends here too.
        _ => Err(QueryError::runtime(format!(
            "`{name}` cannot be called with {} arguments",
            args.len()
        ))),
    }
}

/// The text of a value. A float is written only where its plain decimal form is the one openCypher engines on the JVM
/// give too (zero, and magnitudes from 0.001 up to 10 000 000); outside that their scientific notation differs.
fn to_string(value: &Value) -> Result<Value, QueryError> {
    match value {
        Value::String(s) => Ok(Value::String(s.clone())),
        Value::Bool(b) => Ok(Value::String(b.to_string())),
        Value::Int(i) => Ok(Value::String(i.to_string())),
        Value::Float(f) if *f == 0.0 || (f.abs() >= 1e-3 && f.abs() < 1e7) => {
            Ok(Value::String(format!("{f:?}")))
        }
        Value::Float(_) => Err(QueryError::runtime(
            "`toString` of a float outside 0.001 to 10000000 is not decided yet (ADR 0021)",
        )),
        other => Err(wrong("toString", "a string, a number or a boolean", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorClass;

    fn s(t: &str) -> Value {
        Value::String(t.to_string())
    }

    fn ok(f: Function, args: &[Value]) -> Value {
        call(f, args.to_vec()).unwrap()
    }

    fn class(f: Function, args: &[Value]) -> ErrorClass {
        call(f, args.to_vec()).unwrap_err().class
    }

    #[test]
    fn text_is_counted_and_cut_in_characters() {
        assert_eq!(ok(Function::Size, &[s("añ😀")]), Value::Int(3));
        assert_eq!(ok(Function::Reverse, &[s("añ😀")]), s("😀ña"));
        assert_eq!(
            ok(
                Function::Substring,
                &[s("añ😀b"), Value::Int(1), Value::Int(2)]
            ),
            s("ñ😀")
        );
        assert_eq!(ok(Function::Left, &[s("😀ab"), Value::Int(1)]), s("😀"));
        assert_eq!(ok(Function::Right, &[s("ab😀"), Value::Int(1)]), s("😀"));
        assert_eq!(ok(Function::Right, &[s("ab"), Value::Int(9)]), s("ab"));
        assert_eq!(
            ok(Function::Left, &[s("ab"), Value::Int(i64::MAX)]),
            s("ab")
        );
    }

    #[test]
    fn a_null_position_or_length_is_an_error_a_null_text_is_null() {
        assert_eq!(ok(Function::Left, &[Value::Null, Value::Null]), Value::Null);
        assert_eq!(
            class(Function::Left, &[s("a"), Value::Null]),
            ErrorClass::Runtime
        );
        assert_eq!(
            class(Function::Substring, &[s("a"), Value::Int(-1)]),
            ErrorClass::Runtime
        );
        assert_eq!(
            ok(Function::Replace, &[s("a"), s("a"), Value::Null]),
            Value::Null
        );
    }

    #[test]
    fn a_wrong_kind_of_value_is_an_error_not_a_conversion() {
        assert_eq!(
            class(Function::ToUpper, &[Value::Int(1)]),
            ErrorClass::Runtime
        );
        assert_eq!(class(Function::Size, &[Value::Int(1)]), ErrorClass::Runtime);
        assert_eq!(
            class(Function::ToString, &[Value::List(vec![])]),
            ErrorClass::Runtime
        );
        assert_eq!(
            class(Function::Split, &[s("ab"), s("")]),
            ErrorClass::Runtime
        );
    }

    #[test]
    fn a_float_is_written_only_in_the_range_where_its_text_is_agreed() {
        for (f, t) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (0.001, "0.001"),
            (2.0, "2.0"),
            (-1.5, "-1.5"),
            (9_999_999.0, "9999999.0"),
            (0.1 + 0.2, "0.30000000000000004"),
        ] {
            assert_eq!(ok(Function::ToString, &[Value::Float(f)]), s(t), "{f}");
        }
        for f in [0.0009, 1e7, -1e300, f64::INFINITY] {
            assert_eq!(
                class(Function::ToString, &[Value::Float(f)]),
                ErrorClass::Runtime,
                "{f}"
            );
        }
    }

    #[test]
    fn a_result_beyond_the_value_limit_is_refused_before_it_is_built() {
        let long = s(&"a".repeat(1 << 20));
        // 2^20 characters, each replaced by 2^20: a tebibyte if it were built.
        assert_eq!(
            class(Function::Replace, &[long.clone(), s("a"), long.clone()]),
            ErrorClass::Limit
        );
        assert_eq!(
            class(Function::Replace, &[long.clone(), s(""), long.clone()]),
            ErrorClass::Limit
        );
        assert_eq!(
            class(Function::Split, &[s(&"a".repeat(2 << 20)), s("a")]),
            ErrorClass::Limit
        );
        assert_eq!(ok(Function::Replace, &[long, s("a"), s("")]), s(""));
    }
}
