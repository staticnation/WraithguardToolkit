//! `wraithguard tes3cmd ...` is tes3cmd itself (tes3cmd-rs, built in): its help, its commands
//! and its exit codes, the program's own flags not read.

use std::process::Command;

#[test]
fn tes3cmd_passes_straight_through() {
    let run = |args: &[&str], dir: &std::path::Path| Command::new(env!("CARGO_BIN_EXE_wraithguard")).args(args).current_dir(dir).env("PWD", dir).output().unwrap();
    let dir = std::env::temp_dir().join(format!("wg_cli_tes3cmd_{}", std::process::id()));
    let data = dir.join("Data Files");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(dir.join("Morrowind.ini"), "[Game Files]\n").unwrap();
    // tes3cmd's own help, not this program's (which needs --cfg).
    let help = run(&["tes3cmd", "help"], &data);
    assert!(help.status.success(), "{help:?}");
    assert!(String::from_utf8_lossy(&help.stdout).contains("clean"), "{}", String::from_utf8_lossy(&help.stdout));
    // A command on a plugin that is not there: tes3cmd's own words and failure, nothing of ours.
    let bad = run(&["tes3cmd", "dump", "--type", "TES3", "Nope.esp"], &data);
    let all = format!("{}{}", String::from_utf8_lossy(&bad.stdout), String::from_utf8_lossy(&bad.stderr));
    assert!(!all.contains("--cfg") && !all.to_lowercase().contains("usage: wraithguard"), "{all}");
    // A real one: the header of a plugin written here.
    let mut p = b"TES3".to_vec();
    let hedr = {
        let mut h = Vec::new();
        h.extend_from_slice(&1.3f32.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(&[0u8; 32]);
        h.extend_from_slice(&[0u8; 256]);
        h.extend_from_slice(&0u32.to_le_bytes());
        h
    };
    let mut sub = b"HEDR".to_vec();
    sub.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
    sub.extend_from_slice(&hedr);
    p.extend_from_slice(&(sub.len() as u32).to_le_bytes());
    p.extend_from_slice(&[0u8; 8]);
    p.extend_from_slice(&sub);
    std::fs::write(data.join("Mine.esp"), &p).unwrap();
    let header = run(&["tes3cmd", "header", "--author", "Static", "Mine.esp"], &data);
    assert!(header.status.success(), "{header:?}");
    let written = std::fs::read(data.join("Mine.esp")).unwrap();
    assert!(written.windows(6).any(|w| w == b"Static"), "the author was written");
    let _ = std::fs::remove_dir_all(&dir);
}
