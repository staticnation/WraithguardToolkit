//! Which MGE is installed, and therefore which file its fog settings live in; and (18al,
//! at the end) which Code Patch features the game's executable carries.
//!
//! ## The rule this module exists for (Robin, round 18h)
//!
//! *"We have rules to identify what install we are using, and that should identify what
//! we do in how we render the scene. If G7's fork of MGE XE is installed, always check
//! the mgeXE.toml as well for relevant entries."* And: *"I want the tool to be used by
//! anyone, no matter what version of MGE XE they use, or OpenMW. So we need to be able
//! to read any of the engines that exist and fetch the relevant information."*
//!
//! So `find` is the first question of any rendering feature, not only the fog's: which
//! engine runs this install, and which of its files holds the entries that bear on what
//! is being drawn — `mgeXE.toml` for the G7 fork, `mge3\MGE.ini` for Hrnchamd's and UF,
//! `settings.cfg` for OpenMW, and `Morrowind.ini` for what the game itself owns (light
//! attenuation, weather). The preview follows those entries and names the file and the
//! entry it followed. A constant tuned to a screenshot is a fallback for an install
//! that says nothing, never the model. (`weather::mge_json` reads the fog today; the
//! rest is round 18i's engine profile — see memory.md "Open".)
//!
//! Round 17m. Gardenfell read `mge3\MGE.ini` and nothing else. Robin runs G7's fork of
//! MGE XE, which keeps every setting in `mgeXE.toml` in the game root and never writes
//! the ini at all — so the preview was fogging from a file his game had not touched in
//! three months (5-cell fog end against the 8 he actually plays with, which is most of a
//! factor of two in how far you can see).
//!
//! **The question is settled by the shim Morrowind loads.** `d3d8.dll` is MGE: whatever
//! owns that file *is* the active install, because loading it is what makes it active.
//! And each build names its own config file inside itself — checked against all three
//! that exist:
//!
//! | build                | `mgeXE.toml` | `MGE.ini`          |
//! |----------------------|--------------|--------------------|
//! | MGE-XE G7 Fork 0.20  | yes          | no                 |
//! | MGE XE (Hrnchamd)    | no           | yes (`MGE3/MGE.ini`) |
//! | MGE XE UF 0.19.1     | no           | yes (`MGE3/MGE.ini`) |
//!
//! So the test is "which config does this shim read", asked of the shim. That is better
//! than fingerprinting a fork by name or by the files it ships: UF also mentions
//! `mgeHost64`, which is the marker the G7 fork would otherwise have been known by, and
//! a fourth fork tomorrow answers the question correctly without anybody teaching this
//! code its name.
//!
//! Robin's timestamp idea was the alternative, and the fork's own README rules it out:
//! *"We do not migrate `MGE.ini` into `mgeXE.toml`."* The two files are independent, so a
//! date only says when that file's own GUI last saved — one hand-edit of a stale ini
//! flips the answer, silently.
//!
//! ## Root Builder
//!
//! Kezyma's Root Builder deploys a mod's `Root\` folder into the game folder while MO2
//! runs something, and **clears it again afterwards** (`onFinishedRun` → `clear()`, which
//! restores the originals from its backup). So for anyone using it, the `d3d8.dll` lying
//! in the game folder with the game shut is the one that will *not* load — it is the
//! displaced original. In USVFS mode the real one is never on disk at all. Reading the
//! game folder alone is not merely incomplete for these users, it is backwards, so the
//! deployed file is looked for first:
//!
//!   1. `plugins\data\RootBuilder\*\*\BuildData.json` — Root Builder's own record of what
//!      is deployed right now, and the same file it uses as that test. Its entries carry
//!      the absolute `Source` path. Also catches the leftovers of a crashed session,
//!      which are real: clear only runs on MO2's exit event.
//!   2. Failing that, the build it *would* make: the first `Root\d3d8.dll` down the mod
//!      priority, `overwrite\` first. `Layout::roots` is already in exactly that order —
//!      overwrite, then modlist top-down — because the Data overlay obeys the same rule.
//!   3. Failing that, the game folder.
//!
//! The config file is looked for the same way, since a mod may ship one in its `Root\`.

use crate::layout::Layout;
use std::path::{Path, PathBuf};

/// Which file a build keeps its settings in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfigKind {
    /// `mgeXE.toml` in the game folder — G7's fork.
    Toml,
    /// `mge3\MGE.ini` — MGE XE and MGE XE UF.
    Ini,
}

impl ConfigKind {
    pub fn file(&self) -> &'static str {
        match self {
            ConfigKind::Toml => "mgeXE.toml",
            ConfigKind::Ini => "mge3\\MGE.ini",
        }
    }
}

/// What was found, and how — the "how" is shown to the reader, because a detector whose
/// guess is invisible is how the original bug survived three months.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub dll: Option<PathBuf>,
    /// Where the shim came from: "the game folder", "Root Builder", "a Root mod".
    pub dll_from: String,
    pub kind: Option<ConfigKind>,
    pub config: Option<PathBuf>,
    /// How the config file was decided, for the sources line.
    pub how: String,
}

/// Case-insensitive byte search — the DLL is not text and has no encoding to speak of.
fn contains_ascii_ci(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    let lower = |b: u8| b.to_ascii_lowercase();
    hay.windows(needle.len()).any(|w| w.iter().map(|&b| lower(b)).eq(needle.iter().map(|&b| lower(b))))
}

/// Which config file this shim reads, by asking it.
pub fn dll_kind(bytes: &[u8]) -> Option<ConfigKind> {
    let toml = contains_ascii_ci(bytes, b"mgeXE.toml");
    // `MGE3/MGE.ini` is how both ini-based builds spell it; matching the shorter string
    // catches a build that writes the path some other way.
    let ini = contains_ascii_ci(bytes, b"MGE.ini");
    match (toml, ini) {
        // A build that named both would be reading both; the newer format wins, and the
        // sources line says so.
        (true, _) => Some(ConfigKind::Toml),
        (false, true) => Some(ConfigKind::Ini),
        (false, false) => None,
    }
}

/// Every `Root\<name>` a Root Builder build would deploy, in the order it resolves them:
/// `overwrite\` first, then the mods top-down. The winner is the first that exists.
fn root_mod_file(layout: &Layout, name: &str) -> Option<PathBuf> {
    for r in &layout.roots {
        // The game's own Data Files is in `roots` too and has no `Root\` of its own; the
        // `is_dir` test below skips it without needing to know which entry it is.
        let p = r.join("Root").join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// The `d3d8.dll` Root Builder has deployed *right now*, from its own build record.
///
/// `plugins\data\RootBuilder\<safe game path>\<safe version>\BuildData.json`. The two
/// folder names are the game path and version with characters stripped, so they are
/// globbed rather than reconstructed. The file exists exactly while a build is live —
/// `clear()` deletes it as it restores the game folder — which makes it the honest test
/// for "is something deployed", leftovers from a crashed session included.
fn build_data_dll(mo2: &Path, name: &str) -> Option<PathBuf> {
    let root = mo2.join("plugins").join("data").join("RootBuilder");
    let mut best: Option<PathBuf> = None;
    for game in std::fs::read_dir(&root).ok()?.flatten() {
        for ver in std::fs::read_dir(game.path()).ok().into_iter().flatten().flatten() {
            let f = ver.path().join("BuildData.json");
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            if let Some(src) = build_data_source(&text, name) {
                let p = PathBuf::from(src);
                if p.is_file() {
                    // Several instances can share the folder; a later one is as good as
                    // an earlier, so the first that resolves wins.
                    best = Some(p);
                    break;
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    best
}

/// The `Source` of one deployed file in a `BuildData.json`.
///
/// Read by hand rather than with a JSON parser: the shape is fixed and shallow —
/// `{"Copy":{"<lowercased relative path>":{"Source":"...","Relative":"...","Hash":"..."}}}`
/// with `Link` and `USVFS` beside it — and the file is Root Builder's, not ours, so the
/// less of it this depends on the better. The key is the relative path lowercased, which
/// for a game-root file is just the file name.
pub fn build_data_source(text: &str, name: &str) -> Option<String> {
    /* Round 18bf (I15): the key as **JSON** spells it.

       A path key is written with its backslashes escaped — `"mcpatch\\installed"` in the
       file, two characters — and this searched for the one-backslash form, which cannot
       occur in a JSON string at all. So for a Root Builder user `mcp_features` found no
       record, the Code Patch read as absent, and every banner animated in the preview
       that hangs still in his game: the exact symptom the whole Code Patch block here
       exists to explain (round 18al). Nothing noticed because `d3d8.dll` and
       `mgeXE.toml` — the only other things asked for — have no separator in them.

       Both spellings are tried, since a writer using forward slashes needs no escaping
       and there is no reason to care which one the file used. */
    let lower = text.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    let mut key = format!("\"{}\"", name.replace('\\', "\\\\"));
    let mut at = lower.find(&key);
    if at.is_none() && name.contains('\\') {
        key = format!("\"{}\"", name.replace('\\', "/"));
        at = lower.find(&key);
    }
    let at = at?;
    let rest = &text[at + key.len()..];
    let s = rest.find("\"Source\"")?;
    let rest = &rest[s + "\"Source\"".len()..];
    let open = rest.find('"')?;
    let mut out = String::new();
    let mut esc = false;
    for c in rest[open + 1..].chars() {
        if esc {
            out.push(c);
            esc = false;
            continue;
        }
        match c {
            '\\' => esc = true,
            '"' => return Some(out),
            _ => out.push(c),
        }
    }
    None
}

/// Where a file the game loads out of its own folder actually comes from.
fn active_file(layout: &Layout, name: &str) -> (Option<PathBuf>, String) {
    if let Some(mo2) = layout.mo2_base.as_deref() {
        if let Some(p) = build_data_dll(mo2, name) {
            return (Some(p), "Root Builder".into());
        }
        if let Some(p) = root_mod_file(layout, name) {
            return (Some(p), "a Root Builder mod".into());
        }
    }
    let g = layout.game_dir.as_ref().map(|g| g.join(name)).filter(|p| p.is_file());
    (g, "the game folder".into())
}

/// The active MGE install's config file.
pub fn find(layout: &Layout) -> Found {
    let mut out = Found::default();
    let (dll, from) = active_file(layout, "d3d8.dll");
    out.dll = dll;
    out.dll_from = from;
    if let Some(d) = &out.dll {
        // 32 MB is far past any shim; the cap is here so a wrong path cannot read a disc.
        if let Ok(bytes) = std::fs::read(d) {
            if bytes.len() <= 32 << 20 {
                out.kind = dll_kind(&bytes);
            }
        }
    }

    let toml_at = |l: &Layout| -> Option<PathBuf> {
        active_file(l, "mgeXE.toml").0.or_else(|| {
            l.game_dir.as_ref().map(|g| g.join("mgeXE.toml")).filter(|p| p.is_file())
        })
    };
    let ini_at = |l: &Layout| -> Option<PathBuf> {
        root_mod_file(l, "mge3\\MGE.ini")
            .or_else(|| root_mod_file(l, "mge3/MGE.ini"))
            .or_else(|| l.game_dir.as_ref().map(|g| g.join("mge3").join("MGE.ini")).filter(|p| p.is_file()))
    };

    match out.kind {
        Some(ConfigKind::Toml) => {
            out.config = toml_at(layout);
            out.how = format!("d3d8.dll in {} reads mgeXE.toml", out.dll_from);
        }
        Some(ConfigKind::Ini) => {
            out.config = ini_at(layout);
            out.how = format!("d3d8.dll in {} reads MGE.ini", out.dll_from);
        }
        None => {
            /* No shim to ask — MGE is not installed, or its DLL could not be read. Fall
               back to whatever is on disk, and when both are, to the newer, saying so.
               This is the weak answer Robin proposed as the whole design and it is kept
               only for the case where there is nothing better. */
            let (t, i) = (toml_at(layout), ini_at(layout));
            let age = |p: &Option<PathBuf>| {
                p.as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .and_then(|m| m.modified().ok())
            };
            match (&t, &i) {
                (Some(_), None) => {
                    out.kind = Some(ConfigKind::Toml);
                    out.config = t;
                    out.how = "no MGE found; only mgeXE.toml is present".into();
                }
                (None, Some(_)) => {
                    out.kind = Some(ConfigKind::Ini);
                    out.config = i;
                    out.how = "no MGE found; only MGE.ini is present".into();
                }
                (Some(_), Some(_)) => {
                    let newer_toml = match (age(&t), age(&i)) {
                        (Some(a), Some(b)) => a >= b,
                        _ => true,
                    };
                    out.kind = Some(if newer_toml { ConfigKind::Toml } else { ConfigKind::Ini });
                    out.config = if newer_toml { t } else { i };
                    out.how = "no MGE found; both configs present, took the newer".into();
                }
                (None, None) => {
                    out.how = "no MGE settings found".into();
                }
            }
        }
    }
    if out.config.is_none() {
        if let Some(k) = out.kind {
            out.how = format!("{}, but {} is missing", out.how, k.file());
        }
    }
    out
}

/* ---- the Code Patch ---------------------------------------------------------------------

   Round 18al. Robin: "There are some banners that move in Gardenfell that doesn't in my
   load order, like f\xFurn_bannerD_welcome_01.NIF … What makes them not move in the game?"
   Nothing in the load order: the records carry `OutsideBanner` as they always did, the
   meshes and `.kf` are the vanilla ones, no plugin touches the script. What his install
   has that a plain one does not is the Morrowind Code Patch with feature 132, "Improved
   animation support", whose readme says of a `PlayGroup`/`LoopGroup` with no flag: "The
   controller waits until the current animation is complete before starting the given
   animation." The script never passes a flag, an activator is born in `Idle`, and a
   banner's `Idle` is one instant that, by his report, is never counted complete — so the
   breeze stays queued and the banner hangs. An engine without the patch does leave `Idle`
   (OpenMW issue #3385 is a vanilla-exe player watching them blow), which is why the rule
   is read off the install rather than assumed: "the install decides how the scene is
   drawn" (round 18h). */

/// Code Patch feature 132, "Improved animation support".
pub const MCP_IMPROVED_ANIMATION: u32 = 132;

/// The Code Patch's own record of what it applied: `mcpatch\installed` beside
/// `Morrowind.exe` — `MCP2`, then one little-endian `u32` per feature it knows, the id
/// itself when applied and its two's-complement negative when left out. Found the way
/// the MGE shim is (Root Builder's record, a `Root\` mod, the game folder). None when
/// there is no such file: no Code Patch, or an OpenMW install with no game folder at all.
pub fn mcp_features(layout: &Layout) -> Option<Vec<u32>> {
    let (p, _) = active_file(layout, "mcpatch\\installed");
    let p = p.or_else(|| active_file(layout, "mcpatch/installed").0)?;
    let bytes = std::fs::read(&p).ok()?;
    parse_mcp_installed(&bytes)
}

/// The applied feature ids in an `installed` file, or None for a file that is not one.
pub fn parse_mcp_installed(bytes: &[u8]) -> Option<Vec<u32>> {
    if bytes.len() < 4 || &bytes[..4] != b"MCP2" {
        return None;
    }
    let mut out = Vec::new();
    for w in bytes[4..].chunks_exact(4) {
        let v = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        // The high bit set is a feature the patcher listed and did not apply.
        if v & 0x8000_0000 == 0 {
            out.push(v);
        }
    }
    Some(out)
}

