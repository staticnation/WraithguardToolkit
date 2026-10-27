//! Patching from a conflict (port step 4.3, slice 8): the rules behind "Add record", "Take
//! this field" and "Define a value" in the conflict window (`gui/conflicts.py`
//! `_patch_whole_record`, `_patch_field`, `_ask_patch_winner`, `_value_seed`,
//! `_patch_field_value`; `wraithguard/patch/enums.py` `value_options`). The queue and the
//! patch build are editcore's (`editcore::queue`, `editcore::build`); the dialogs are the
//! page's.

use serde_json::Value;

use crate::fields::FieldDiff;

/// Why a patch cannot be offered for a record or field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// One plugin defines it: nothing to choose between.
    OnlyOne,
    /// The field says which record this is (`IDENTITY`): a typed value would make another
    /// record.
    Identity(String),
}

impl Refusal {
    /// The message the window gives (English).
    pub fn message(&self) -> String {
        match self {
            Refusal::OnlyOne => "Only one plugin defines this record, so there is nothing to patch.".into(),
            Refusal::Identity(path) => format!(
                "{path} says which record this is -- giving it a typed value would make a different record, not a patched one."
            ),
        }
    }
}

/// The choice "which version should win" offers: the plugins in load order, the last (the
/// current winner) chosen to start with.
pub fn winner_choice(plugins: &[String]) -> Result<(&[String], &str), Refusal> {
    match plugins {
        [_, .., last] => Ok((plugins, last.as_str())),
        _ => Err(Refusal::OnlyOne),
    }
}

/// Whether a field can be given a typed value (not one of `editcore::edit::IDENTITY`).
pub fn can_define(path: &str) -> Result<(), Refusal> {
    if editcore::edit::IDENTITY.contains(&path) { Err(Refusal::Identity(path.to_string())) } else { Ok(()) }
}

/// A field's current value as the entry's starting text (`_value_seed`): the string itself,
/// JSON for a list or object, the number or flag as Python's `str` gives it, or empty when
/// absent or null.
pub fn value_seed(current: Option<&Value>) -> String {
    match current {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v @ (Value::Array(_) | Value::Object(_))) => py_json(v),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.into(),
        Some(Value::Number(n)) => match n.as_f64().filter(|_| n.is_f64()) {
            Some(f) => py_float(f),
            None => n.to_string(),
        },
    }
}

/// Python's `repr` of a float, as far as a field value needs it.
fn py_float(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.into();
    }
    let s = format!("{f:?}");
    if s.contains('e') {
        // Rust writes 1e20, Python 1e+20.
        let (m, e) = s.split_once('e').unwrap_or((&s, ""));
        let e = if e.starts_with('-') { e.to_string() } else { format!("+{e}") };
        let e = match e.as_bytes() {
            [sign, d] => format!("{}0{}", *sign as char, *d as char),
            _ => e,
        };
        return format!("{m}e{e}");
    }
    s
}

/// `json.dumps` with its default separators (`", "` and `": "`), non-ASCII escaped.
fn py_json(v: &Value) -> String {
    fn esc(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                    let mut buf = [0u16; 2];
                    for unit in c.encode_utf16(&mut buf) {
                        out.push_str(&format!("\\u{unit:04x}"));
                    }
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => match n.as_f64().filter(|_| n.is_f64()) {
                Some(f) if f.is_nan() => out.push_str("NaN"),
                Some(f) if f.is_infinite() => out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" }),
                Some(f) => out.push_str(&py_float(f)),
                None => out.push_str(&n.to_string()),
            },
            Value::String(s) => esc(s, out),
            Value::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    walk(x, out);
                }
                out.push(']');
            }
            Value::Object(m) => {
                out.push('{');
                for (i, (k, x)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    esc(k, out);
                    out.push_str(": ");
                    walk(x, out);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    walk(v, &mut out);
    out
}

/// The dropdown for a string field (`value_options`): the current value first when it is a
/// string, then the field's known variants (`editcore::fields::enum_options`), then the
/// strings the plugins use, each once.
pub fn value_options(current: Option<&Value>, path: &str, observed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(Value::String(s)) = current {
        out.push(s.clone());
    }
    for v in editcore::fields::enum_options(path).iter().chain(observed) {
        if !out.contains(v) {
            out.push(v.clone());
        }
    }
    out
}

/// What "Define a value" starts from for one field of a diffed record: the winner's value
/// (`None`: the winner lacks it), and every string a plugin uses for it, in load order.
pub fn define_start<'a>(d: &'a FieldDiff, path: &str) -> (Option<&'a Value>, Vec<String>) {
    let values = d.values(path);
    let current = values.last().copied().flatten();
    let observed = values.iter().filter_map(|v| v.and_then(Value::as_str)).map(str::to_string).collect();
    (current, observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_last_plugin_starts_chosen_and_one_alone_is_refused() {
        let ps: Vec<String> = vec!["A.esp".into(), "B.esp".into()];
        assert_eq!(winner_choice(&ps).unwrap().1, "B.esp");
        assert_eq!(winner_choice(&ps[..1]), Err(Refusal::OnlyOne));
        assert!(can_define("data.grid").is_err());
        assert!(can_define("data.level").is_ok());
    }

    #[test]
    fn a_seed_is_what_python_would_write() {
        assert_eq!(value_seed(None), "");
        assert_eq!(value_seed(Some(&json!(null))), "");
        assert_eq!(value_seed(Some(&json!("x"))), "x");
        assert_eq!(value_seed(Some(&json!(true))), "True");
        assert_eq!(value_seed(Some(&json!(3))), "3");
        assert_eq!(value_seed(Some(&json!(1.5))), "1.5");
        assert_eq!(value_seed(Some(&json!(2.0))), "2.0");
        assert_eq!(value_seed(Some(&json!([1, "é", {"a": 1.0}]))), "[1, \"\\u00e9\", {\"a\": 1.0}]");
    }

    #[test]
    fn options_put_the_current_value_first_and_repeat_nothing() {
        let got = value_options(Some(&json!("b")), "zz_unknown_field", &["a".into(), "b".into(), "a".into()]);
        assert_eq!(got, ["b", "a"]);
    }
}
