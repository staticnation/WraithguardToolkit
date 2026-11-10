//! `wraithguard --data-files` / `--mo2` / `--plain-mlox`: the whole load order sorted by the
//! mlox rules alone (`ordercore::plain`), printed, and written only with `--write-cfg`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_wraithguard")
}

fn game(d: &Path) -> PathBuf {
    let data = d.join("Data Files");
    fs::create_dir_all(&data).unwrap();
    for n in ["Morrowind.esm", "A.esp", "B.esp"] {
        fs::write(data.join(n), b"").unwrap();
    }
    fs::write(d.join("Morrowind.ini"), "[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=A.esp\r\nGameFile2=B.esp\r\n").unwrap();
    fs::write(d.join("mlox_user.txt"), "[Order]\nB.esp\nA.esp\n").unwrap();
    data
}

fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new(bin()).args(args).env("WRAITHGUARD_CLI_NOW", "2026-11-10T12:00:00").output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_game_install_is_sorted_and_written_only_when_asked() {
    let d = std::env::temp_dir().join(format!("wg_cli_plain_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let data = game(&d);
    let rules = d.join("mlox_user.txt");
    let (ok, out) = run(&["--data-files", data.to_str().unwrap(), "--rules", rules.to_str().unwrap()]);
    assert!(ok, "{out}");
    assert!(out.contains("PLAIN MLOX SORT (Morrowind.ini; no MOMW list)") && out.contains("B.esp: 3 -> 2"), "{out}");
    assert!(out.contains("would be written"), "{out}");
    assert!(fs::read_to_string(d.join("Morrowind.ini")).unwrap().contains("GameFile1=A.esp"));
    let (ok, out) = run(&["--data-files", data.to_str().unwrap(), "--rules", rules.to_str().unwrap(), "--write-cfg"]);
    assert!(ok, "{out}");
    assert!(fs::read_to_string(d.join("Morrowind.ini")).unwrap().contains("GameFile1=B.esp\r\nGameFile2=A.esp"), "{out}");
    assert!(d.join("Morrowind.ini.bak-20261110-120000").is_file());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_cfg_with_plain_mlox_gets_its_content_lines_sorted() {
    let d = std::env::temp_dir().join(format!("wg_cli_plaincfg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let data = game(&d);
    let cfg = d.join("openmw.cfg");
    fs::write(&cfg, format!("data=\"{}\"\ncontent=Morrowind.esm\ncontent=A.esp\ncontent=B.esp\n", data.display())).unwrap();
    let rules = d.join("mlox_user.txt");
    let (ok, out) = run(&["--cfg", cfg.to_str().unwrap(), "--plain-mlox", "--rules", rules.to_str().unwrap(), "--write-cfg"]);
    assert!(ok, "{out}");
    assert!(fs::read_to_string(&cfg).unwrap().ends_with("content=B.esp\ncontent=A.esp\n"), "{out}");
    // --emit-toml: the customizations that move A.esp (the curated list's) after B.esp.
    let toml = d.join("customizations.toml");
    fs::write(&cfg, format!("data=\"{}\"\ncontent=Morrowind.esm\ncontent=A.esp\ncontent=B.esp\n", data.display())).unwrap();
    let (ok, out) = run(&["--cfg", cfg.to_str().unwrap(), "--plain-mlox", "--rules", rules.to_str().unwrap(), "--emit-toml", toml.to_str().unwrap()]);
    assert!(ok, "{out}");
    let t = fs::read_to_string(&toml).unwrap();
    assert!(t.contains("'A.esp',") && t.contains("insert = 'A.esp'\nafter = 'B.esp'"), "{t}");
    assert!(fs::read_to_string(&cfg).unwrap().ends_with("content=A.esp\ncontent=B.esp\n"), "no --write-cfg: the cfg stays");
    let _ = fs::remove_dir_all(&d);
}
