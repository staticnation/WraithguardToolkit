//! Reading what the page sends.
//!
//! The page holds the config being edited — that is input, and input belongs to the
//! side with the keyboard. What it must not hold is a second definition of the file
//! format, so it sends its model as JSON and the engine writes every byte of TOML that
//! reaches disk. This is the reader that makes that possible, and the payload it is
//! given comes from `JSON.stringify`, so it has to survive what JavaScript emits.

use gardenfell_bytes::json::{parse, Val};

#[test]
fn the_shapes_a_config_is_made_of() {
    let v = parse(r#"{"a":1,"b":"two","c":true,"d":null,"e":[1,2,3],"f":{"g":-4.5e2}}"#).unwrap();
    assert_eq!(v.get("a").and_then(Val::as_f64), Some(1.0));
    assert_eq!(v.get("b").and_then(Val::as_str), Some("two"));
    assert_eq!(v.get("c").and_then(Val::as_bool), Some(true));
    assert_eq!(v.get("d"), Some(&Val::Null));
    assert_eq!(v.get("e").and_then(Val::as_arr).map(|a| a.len()), Some(3));
    assert_eq!(v.get("f").and_then(|f| f.get("g")).and_then(Val::as_f64), Some(-450.0));
    assert_eq!(v.get("nope"), None);
}

#[test]
fn whitespace_and_empty_containers() {
    let v = parse(" {\n  \"a\" : [ ] ,\t\"b\" : { }\r\n} ").unwrap();
    assert_eq!(v.get("a").and_then(Val::as_arr).map(|a| a.is_empty()), Some(true));
    assert_eq!(v.get("b"), Some(&Val::Obj(vec![])));
}

/// A checkbox reaches the boundary as `1` at least as often as `true`, because the
/// page has been storing switches as numbers since before any of this existed.
#[test]
fn a_number_reads_as_a_boolean() {
    let v = parse(r#"{"on":1,"off":0}"#).unwrap();
    assert_eq!(v.get("on").and_then(Val::as_bool), Some(true));
    assert_eq!(v.get("off").and_then(Val::as_bool), Some(false));
    assert!(v.bool_or("on", false));
    assert!(!v.bool_or("missing", false));
}

/// Mesh paths are the reason this matters: every one of them is full of backslashes,
/// and `JSON.stringify` doubles each.
#[test]
fn escapes_come_back_as_what_was_sent() {
    let v = parse(r#"{"mesh":"Grass\\bc\\g_01.nif","q":"say \"hi\"","nl":"a\nb","tab":"a\tb"}"#)
        .unwrap();
    assert_eq!(v.get("mesh").and_then(Val::as_str), Some(r"Grass\bc\g_01.nif"));
    assert_eq!(v.get("q").and_then(Val::as_str), Some("say \"hi\""));
    assert_eq!(v.get("nl").and_then(Val::as_str), Some("a\nb"));
    assert_eq!(v.get("tab").and_then(Val::as_str), Some("a\tb"));
}

/// Community configs are full of accented mod names, and a reader that walks bytes and
/// pushes `c as char` double-encodes every one of them — compounding on each save. The
/// engine's TOML reader had exactly that bug once.
#[test]
fn text_beyond_ascii_survives_both_ways_of_sending_it() {
    // Raw UTF-8, which is what JSON.stringify emits by default…
    let v = parse("{\"a\":\"Vvardenfell — Åke's mod\"}").unwrap();
    assert_eq!(v.get("a").and_then(Val::as_str), Some("Vvardenfell — Åke's mod"));
    // …and the escaped form, which some encoders prefer.
    let e = parse(r#"{"a":"Vvardenfell — Åke"}"#).unwrap();
    assert_eq!(e.get("a").and_then(Val::as_str), Some("Vvardenfell — Åke"));
    // Above the BMP arrives as a surrogate pair.
    let s = parse(r#"{"a":"🌿"}"#).unwrap();
    assert_eq!(s.get("a").and_then(Val::as_str), Some("🌿"));
}

#[test]
fn nesting_deep_enough_for_a_config() {
    let text = r#"{"selectors":[
        {"texture":"T_a","slots":[{"mesh":"a.nif","chance":60},{"mesh":"b.nif","chance":40}]},
        {"texture":"T_b","slots":[]}
    ]}"#;
    let v = parse(text).unwrap();
    let sel = v.get("selectors").and_then(Val::as_arr).unwrap();
    assert_eq!(sel.len(), 2);
    let slots = sel[0].get("slots").and_then(Val::as_arr).unwrap();
    assert_eq!(slots[1].str_or("mesh", ""), "b.nif");
    assert_eq!(slots[0].f32_or("chance", 0.0), 60.0);
    assert!(sel[1].get("slots").and_then(Val::as_arr).unwrap().is_empty());
}

/// Malformed input is a bug on the other side of the boundary, and the message has to
/// be enough to find it. Silently returning an empty config would look like the user's
/// rules vanishing.
#[test]
fn broken_input_is_refused_rather_than_guessed_at() {
    for bad in [
        "{",
        "{\"a\":}",
        "{\"a\" 1}",
        "[1,2",
        "{\"a\":1}}",
        "{\"a\":tru}",
        "\"unterminated",
        "",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} should not parse");
    }
}

/// The defaults path, which is most of how the config reader will actually use this:
/// a key that is absent, or present with the wrong type, falls back rather than
/// throwing away the rest of the config.
#[test]
fn a_missing_or_wrongly_typed_field_falls_back() {
    let v = parse(r#"{"gap":"not a number","texture":"T_a"}"#).unwrap();
    assert_eq!(v.f32_or("gap", 100.0), 100.0, "wrong type");
    assert_eq!(v.f32_or("absent", 100.0), 100.0, "not there");
    assert_eq!(v.str_or("texture", ""), "T_a");
    assert_eq!(v.str_or("absent", "fallback"), "fallback");
}
