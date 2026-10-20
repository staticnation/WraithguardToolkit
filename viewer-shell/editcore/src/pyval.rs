//! JSON values read and shown the way the Python Editor reads and shows them, so the two
//! answer the same to what the viewer sends: Python's `float()` of a text, its `str()`
//! and `repr()` of a value, and the type names it reports. Used by `crate::edit`, whose
//! messages quote values as the Python ones did (`{raw!r}`).

use serde_json::Value;

/// Python's `float(text)`: surrounding white space, a sign, `inf`/`infinity`/`nan` in any
/// case, an exponent, and underscores between digits (`1_000`). `None` where Python
/// raises.
pub fn float_of(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let low = t.to_ascii_lowercase();
    let (sign, body) = match low.strip_prefix('-') {
        Some(b) => (-1.0, b),
        None => (1.0, low.strip_prefix('+').unwrap_or(&low)),
    };
    match body {
        "inf" | "infinity" => return Some(sign * f64::INFINITY),
        "nan" => return Some(f64::NAN),
        _ => {}
    }
    // Underscores only between two digits.
    let b = body.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'_' && !(i > 0 && i + 1 < b.len() && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit()) {
            return None;
        }
    }
    let plain: String = body.chars().filter(|&c| c != '_').collect();
    // Rust takes what Python does here, less words ("inf" above) and more: refuse those.
    if !plain.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'+' | b'-')) || !plain.bytes().any(|c| c.is_ascii_digit()) {
        return None;
    }
    plain.parse::<f64>().ok().map(|x| sign * x)
}

/// Python's `repr()` of a float: the shortest digits that read back, in fixed notation
/// from 1e-4 up to 1e16 (always with a `.`), else in exponent notation (`1e+16`, `1e-05`).
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let sci = format!("{x:e}"); // shortest digits: "1.2345e-5"
    let (mant, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("an exponent");
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let sign = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        let point = exp + 1; // digits before the point
        let s = if point <= 0 {
            format!("0.{}{}", "0".repeat((-point) as usize), digits)
        } else if point as usize >= digits.len() {
            format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        };
        format!("{sign}{s}")
    } else {
        let m = if digits.len() > 1 { format!("{}.{}", &digits[..1], &digits[1..]) } else { digits };
        format!("{sign}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

/// Python's `repr()` of a string (single quotes unless it holds one and no double one).
pub fn str_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// A number as Python shows it: an int as its digits, a float by [`float_repr`].
fn number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        float_repr(n.as_f64().unwrap_or(f64::NAN))
    }
}

/// Python's `repr()` of a JSON-decoded value.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => number(n),
        Value::String(s) => str_repr(s),
        Value::Array(a) => format!("[{}]", a.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Object(m) => format!("{{{}}}", m.iter().map(|(k, x)| format!("{}: {}", str_repr(k), repr(x))).collect::<Vec<_>>().join(", ")),
    }
}

/// Python's `format(x, "g")`: six significant digits, trailing zeros dropped, exponent
/// notation below 1e-4 and from 1e6 (`1e-05`, `1.23457e+06`).
pub fn g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let sci = format!("{x:.5e}");
    let (mant, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("an exponent");
    let trim = |s: &str| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s.to_string() };
    if (-4..6).contains(&exp) {
        trim(&format!("{:.*}", (5 - exp) as usize, x))
    } else {
        format!("{}e{}{:02}", trim(mant), if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

/// Python's `str()` of a JSON-decoded value (a string as itself, anything else its repr).
pub fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => repr(other),
    }
}

/// What a value is, as Python's `isinstance` sorts the JSON-decoded value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    Bool,
    Int,
    Float,
    Str,
    List,
    Dict,
}

pub fn kind(v: &Value) -> Kind {
    match v {
        Value::Null => Kind::None,
        Value::Bool(_) => Kind::Bool,
        Value::Number(n) if n.is_f64() => Kind::Float,
        Value::Number(_) => Kind::Int,
        Value::String(_) => Kind::Str,
        Value::Array(_) => Kind::List,
        Value::Object(_) => Kind::Dict,
    }
}

/// Python's `type(value).__name__`.
pub fn type_name(v: &Value) -> &'static str {
    match kind(v) {
        Kind::None => "NoneType",
        Kind::Bool => "bool",
        Kind::Int => "int",
        Kind::Float => "float",
        Kind::Str => "str",
        Kind::List => "list",
        Kind::Dict => "dict",
    }
}

/// A float as a JSON value (`None` where JSON has no number: the caller refuses it).
pub fn float_value(x: f64) -> Value {
    serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
}

/// An integer as a JSON value, past 64 bits as a float (as Python would carry it).
pub fn int_value(i: i128) -> Value {
    if let Ok(v) = i64::try_from(i) {
        Value::from(v)
    } else if let Ok(v) = u64::try_from(i) {
        Value::from(v)
    } else {
        float_value(i as f64)
    }
}

/// Python's `==` between two JSON-decoded values: an int equals the float of the same
/// value (`1 == 1.0`), `True == 1`, and dicts compare without regard to order.
pub fn eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => num(x) == num(y),
        (Value::Bool(x), Value::Number(y)) | (Value::Number(y), Value::Bool(x)) => num(y) == if *x { 1.0 } else { 0.0 },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| eq(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| eq(v, w))),
        _ => a == b,
    }
}

fn num(n: &serde_json::Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_read_and_show_as_python_has_them() {
        assert_eq!(float_of(" 1_000.5 "), Some(1000.5));
        assert_eq!(float_of("1e3"), Some(1000.0));
        assert_eq!(float_of("-Infinity"), Some(f64::NEG_INFINITY));
        assert!(float_of("1__0").is_none() && float_of("_1").is_none() && float_of("abc").is_none() && float_of("").is_none());
        assert!(float_of("True").is_none() && float_of("0x10").is_none());
        for (x, s) in [(1.5, "1.5"), (2.0, "2.0"), (1e16, "1e+16"), (1e15, "1000000000000000.0"), (0.0001, "0.0001"), (0.00001, "1e-05"), (-0.5, "-0.5"), (123456789.125, "123456789.125"), (0.1 + 0.2, "0.30000000000000004")] {
            assert_eq!(float_repr(x), s);
        }
        assert_eq!(repr(&json!({"a": [1, 2.5, "x", null, true]})), "{'a': [1, 2.5, 'x', None, True]}");
        assert_eq!(str_repr("it's"), "\"it's\"");
        for (x, s) in [(10.0, "10"), (0.1, "0.1"), (0.00001, "1e-05"), (1234567.0, "1.23457e+06"), (2.5, "2.5"), (100000.0, "100000"), (1e6, "1e+06"), (0.0001234, "0.0001234")] {
            assert_eq!(g(x), s, "{x}");
        }
        assert!(eq(&json!(1), &json!(1.0)) && eq(&json!(true), &json!(1)) && !eq(&json!("1"), &json!(1)));
    }
}
