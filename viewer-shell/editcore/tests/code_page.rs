//! The Editor's code page (`plugin::set_code_page`): a Russian edition's plugin, its text in
//! Windows-1251, read as Cyrillic and written back in it; a name typed in Cyrillic written
//! as the Russian game reads it. A process of its own (the code page is the process's).

use editcore::plugin;
use tes3::esp::{Header, MiscItem, Plugin, TES3Object};

fn russian_plugin() -> Vec<u8> {
    let header = Header { author: String::from("Сергей").into(), ..Default::default() };
    let item = MiscItem { id: "misc_lamp".into(), name: "Лампа".into(), ..Default::default() };
    let mut p = Plugin { objects: vec![TES3Object::Header(header), TES3Object::MiscItem(item)] };
    p.save_bytes_with_encoding(bytes_io::encoding_rs::WINDOWS_1251).unwrap()
}

#[test]
fn a_russian_plugin_reads_and_writes_in_its_code_page() {
    let bytes = russian_plugin();
    assert_eq!(plugin::code_page_named("win1251"), Some(bytes_io::encoding_rs::WINDOWS_1251));
    assert_eq!(plugin::code_page_named("CP1250"), Some(bytes_io::encoding_rs::WINDOWS_1250));
    assert_eq!(plugin::code_page_named("utf-8"), None);

    // Windows-1252 (the default): other letters, but every byte written back as it was.
    assert_eq!(plugin::code_page(), bytes_io::encoding_rs::WINDOWS_1252);
    let json = plugin::records_json(&bytes, None).unwrap();
    assert!(!json.contains("Лампа"));
    assert_eq!(plugin::records_bytes(&json).unwrap(), bytes);

    // The install's code page: the names as the Russian game shows them.
    plugin::set_code_page("win1251");
    let json = plugin::records_json(&bytes, None).unwrap();
    assert!(json.contains("Лампа") && json.contains("Сергей"), "{json}");
    assert_eq!(plugin::records_bytes(&json).unwrap(), bytes, "written back byte for byte");
    // A name typed in Cyrillic, written in the code page.
    let typed = json.replace("Лампа", "Фонарь");
    let out = plugin::records_bytes(&typed).unwrap();
    let mut back = Plugin::new();
    back.load_bytes_with_encoding(&out, bytes_io::encoding_rs::WINDOWS_1251).unwrap();
    assert_eq!(back.objects_of_type::<MiscItem>().next().unwrap().name, "Фонарь");
    let objs = plugin::objects(&out, None, false).unwrap();
    assert!(objs.iter().any(|o| matches!(o, TES3Object::MiscItem(m) if m.name == "Фонарь")));

    // Back to Windows-1252: Cyrillic cannot be written in it, and says so.
    plugin::set_code_page("");
    assert!(plugin::records_bytes(&typed).is_err());
}
