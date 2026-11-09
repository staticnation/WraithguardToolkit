//! `wraithguard --merge-lands`: the Tools page's merge (`landcore::service`) from the command
//! line. The Python's command line had no Merge Lands, so there is no recorded answer to hold
//! it to (`cli_cases.json` is the Python's); instead the binary's `Merged Lands.esp` must be
//! the one `build_merged_lands` writes for the same load order, byte for byte, and its lines
//! the ones the merge says. The merge itself is held to the Python by
//! `landcore/tests/merge_cases.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use landcore::service::{build_merged_lands, MergeOptions, DEFAULT_NAME};

const SIZE: usize = 65;

fn sub(tag: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
    v.extend(payload);
    v
}

fn record(tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
    v.extend([0u8; 8]);
    v.extend(body);
    v
}

/// A cell at (0, 0): flat at 400, with a hill of `bump` at (`cx`, `cy`) when `bump` is not 0.
fn land(cx: i32, cy: i32, bump: i32) -> Vec<u8> {
    let at = |x: i32, y: i32| 400 + if (x - cx).pow(2) + (y - cy).pow(2) <= 36 { bump } else { 0 };
    let mut body = sub(b"INTV", &[0i32.to_le_bytes(), 0i32.to_le_bytes()].concat());
    body.extend(sub(b"DATA", &1u32.to_le_bytes()));
    body.extend(sub(b"VNML", &vec![0u8; 3 * SIZE * SIZE]));
    let mut vhgt = (at(0, 0) as f32).to_le_bytes().to_vec();
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let d = if x == 0 { if y == 0 { 0 } else { at(0, y) - at(0, y - 1) } } else { at(x, y) - at(x - 1, y) };
            vhgt.push(d as i8 as u8);
        }
    }
    vhgt.extend([0, 0, 0]);
    body.extend(sub(b"VHGT", &vhgt));
    record(b"LAND", &body)
}

fn plugin(master: bool, masters: &[&str], land: Vec<u8>) -> Vec<u8> {
    let mut hedr = 1.3f32.to_le_bytes().to_vec();
    hedr.extend(u32::from(master).to_le_bytes());
    hedr.extend([0u8; 288]);
    hedr.extend(1u32.to_le_bytes());
    let mut body = sub(b"HEDR", &hedr);
    for m in masters {
        body.extend(sub(b"MAST", &[m.as_bytes(), &[0]].concat()));
        body.extend(sub(b"DATA", &0u64.to_le_bytes()));
    }
    [record(b"TES3", &body), land].concat()
}

/// Base.esm flat, A.esp a hill in one corner, B.esp one in the other: both survive the merge.
fn setup(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("wg_cli_merge_{}_{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let data = root.join("Data Files");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("Base.esm"), plugin(true, &[], land(0, 0, 0))).unwrap();
    fs::write(data.join("A.esp"), plugin(false, &["Base.esm"], land(20, 20, 60))).unwrap();
    fs::write(data.join("B.esp"), plugin(false, &["Base.esm"], land(48, 48, 40))).unwrap();
    let cfg = format!("data=\"{}\"\ncontent=Base.esm\ncontent=A.esp\ncontent=B.esp\n", data.display());
    fs::write(root.join("openmw.cfg"), cfg).unwrap();
    root
}

fn wraithguard(root: &Path, extra: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_wraithguard"))
        .arg("--cfg")
        .arg(root.join("openmw.cfg"))
        .args(extra)
        .output()
        .unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n");
    (text(&out.stdout), text(&out.stderr), out.status.code().unwrap_or(-1))
}

#[test]
fn the_command_line_writes_the_merge_the_tools_page_writes() {
    let root = setup("write");
    let data = root.join("Data Files");
    let (stdout, stderr, code) = wraithguard(&root, &["--merge-lands"]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains(" MERGE LANDS\n"), "{stdout}");
    assert!(stdout.contains("  3 plugin(s) in the load order, 1 data folder(s)\n"), "{stdout}");
    // No folder given: the one holding the load order, as the Tools page offers.
    assert!(stdout.contains(&format!("  writing into: {}\n", data.display())), "{stdout}");
    assert!(stdout.contains("  Load it last: add content=Merged Lands.esp at the end of the cfg's content= lines.\n"), "{stdout}");
    let got = fs::read(data.join(DEFAULT_NAME)).unwrap();
    assert!(data.join("Merged Lands.mergedlands.toml").is_file());

    let want_dir = root.join("want");
    let order: Vec<String> = ["Base.esm", "A.esp", "B.esp"].iter().map(|s| s.to_string()).collect();
    let opts = MergeOptions { output: Some(want_dir.join(DEFAULT_NAME)), ..Default::default() };
    let mut said = Vec::new();
    let r = build_merged_lands(std::slice::from_ref(&data), &order, &opts, &mut |l| said.push(l.to_string())).unwrap();
    assert_eq!(r.cells_written, 1);
    assert_eq!(got, fs::read(want_dir.join(DEFAULT_NAME)).unwrap(), "the command line's Merged Lands.esp differs");
    // Each line the merge said, as it said it (the paths aside: the service wrote elsewhere).
    for line in said.iter().filter(|l| !l.starts_with("wrote ")) {
        assert!(stdout.contains(&format!("  {line}\n")), "missing {line:?} in\n{stdout}");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_dry_run_and_a_failure_write_nothing() {
    let root = setup("dry");
    let data = root.join("Data Files");
    let out = root.join("out");
    let (stdout, _, code) = wraithguard(&root, &["--merge-lands", out.to_str().unwrap(), "--dry-run", "--merge-model", "loadorder"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("  each plugin measured against the load order before it\n"), "{stdout}");
    assert!(stdout.contains("  dry run: nothing was written\n") || stdout.contains("  nothing to merge; no plugin written\n"), "{stdout}");
    assert!(!out.exists() && !data.join(DEFAULT_NAME).exists());

    // The masters ignored: nothing to measure against, so it stops.
    let (stdout, stderr, code) = wraithguard(&root, &["--merge-lands", "--merge-ignore", "Base.esm"]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stderr.starts_with("Merge Lands stopped: no masters in the load order."), "{stderr}");
    assert!(!data.join(DEFAULT_NAME).exists());
    let _ = fs::remove_dir_all(&root);
}
