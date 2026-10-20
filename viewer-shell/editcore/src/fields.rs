//! What each field of a record is - the crate-declared kind, the flags a flag set has,
//! the variants an enum field takes - from the tables the Python Editor uses
//! (`data/fields.json`, written by `tools/gen_editcore_tables.py` from
//! `wraithguard/patch/field_types.py` and `enum_data.py`; `tests/test_editcore_tables.py`
//! fails when they drift). The rules of `wraithguard/patch/fieldtypes.py` and `enums.py`.
//!
//! A kind is one of `float`, `int:<min>:<max>`, `str`, `bool`, `enum`,
//! `flags:<FlagsName>`, or `list`.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

struct Tables {
    record_fields: HashMap<String, HashMap<String, String>>,
    flag_variants: HashMap<String, Vec<String>>,
    field_enums: HashMap<String, Vec<String>>,
    filter_choices: Value,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("../data/fields.json")).expect("data/fields.json is JSON");
        let strings = |x: &Value| x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let lists = |key: &str| -> HashMap<String, Vec<String>> {
            v[key].as_object().map(|m| m.iter().map(|(k, x)| (k.clone(), strings(x))).collect()).unwrap_or_default()
        };
        let record_fields = v["record_fields"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(t, fields)| {
                        let f = fields.as_object().map(|f| f.iter().map(|(p, k)| (p.clone(), k.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
                        (t.clone(), f)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Tables { record_fields, flag_variants: lists("flag_variants"), field_enums: lists("field_enums"), filter_choices: v["filter_choices"].clone() }
    })
}

/// The crate-declared kind of a field, or `None` when it is not known.
pub fn field_kind(record_type: &str, path: &str) -> Option<&'static str> {
    tables().record_fields.get(record_type)?.get(path).map(String::as_str)
}

/// The inclusive `(min, max)` of an `int:min:max` kind.
pub fn int_bounds(kind: &str) -> Option<(i128, i128)> {
    let rest = kind.strip_prefix("int:")?;
    let (lo, hi) = rest.split_once(':')?;
    Some((lo.parse().ok()?, hi.parse().ok()?))
}

/// The flag set's name of a `flags:Name` kind.
pub fn flags_name(kind: &str) -> Option<&str> {
    kind.strip_prefix("flags:")
}

/// A flag set's flag names, in declaration order (none for an unknown set).
pub fn flag_options(name: &str) -> &'static [String] {
    tables().flag_variants.get(name).map(Vec::as_slice).unwrap_or(&[])
}

/// The known variants of an enum field, by the field's leaf name (none when unknown).
pub fn enum_options(path: &str) -> &'static [String] {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    tables().field_enums.get(leaf).map(Vec::as_slice).unwrap_or(&[])
}

/// What a dialogue condition can be: `{types, functions, comparisons}`, by name
/// (`workflow.filter_choices`, from the Python port's enums).
pub fn filter_choices() -> Value {
    let t = &tables().filter_choices;
    // In the Python's order (the tables file sorts its keys).
    serde_json::json!({"types": t["types"], "functions": t["functions"], "comparisons": t["comparisons"]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_answer_as_the_python_ones() {
        assert_eq!(field_kind("Weapon", "data.weight"), Some("float"));
        assert_eq!(int_bounds(field_kind("Alchemy", "data.value").unwrap()), Some((0, 4294967295)));
        assert_eq!(flags_name("flags:ObjectFlags"), Some("ObjectFlags"));
        assert!(!flag_options("ObjectFlags").is_empty());
        assert!(enum_options("data.apparatus_type").iter().any(|v| v == "Alembic"));
        assert!(field_kind("Weapon", "no.such").is_none() && enum_options("nothing").is_empty());
    }
}
