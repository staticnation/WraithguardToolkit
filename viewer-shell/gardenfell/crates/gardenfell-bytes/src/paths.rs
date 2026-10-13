//! One spelling for a name: asset paths as the overlay and the archives key them, mesh
//! and texture paths as the per-mesh setups key them, and the test for "is this file a
//! plugin". Round 18ge: these sat in `vfs`, `statics` and `world`, which is how the
//! archive reader came to depend on the overlay, the NIF reader on the paint store and
//! the overlay on the world. Nothing here depends on anything.

/// One spelling for a relative asset path: forward slashes, lower case, and no leading
/// rubbish.
///
/// Round 18bf (I17): a leading separator and a `Data Files/` prefix are stripped, and
/// runs of separators collapse. The index's keys are relative to a root with no leading
/// slash, so a sloppy mod's `\Grass\x.nif` used to normalise to `/grass/x.nif` and
/// resolve nowhere at all — and a plugin or an `.ini` that names `Data Files\Grass\x.nif`
/// is naming the same file a different way, not a file that does not exist. Both are
/// things real mods do; neither is a reason to report a mesh as missing.
///
/// Archive keys go through here too ([`crate::bsa`]), so both sides of a lookup are
/// spelled the same way by construction.
///
/// Round 18gq: lowercased through the whole alphabet when a path has a letter outside
/// ASCII, as [`norm_mesh`] has been since 18bd. A plugin read in its own code page now
/// decodes a Russian mod's `Meshes\Трава\Трава.nif` to the letters it means, and the
/// overlay index keys the file by the name the disk gives it; the two only meet if both
/// fold the same way, and `to_ascii_lowercase` leaves every Cyrillic capital as it is. The
/// ASCII path is unchanged and still costs one pass.
pub fn norm(p: &str) -> String {
    let trimmed = p.trim().trim_matches('"');
    let mut s = if trimmed.is_ascii() {
        trimmed.replace('\\', "/").to_ascii_lowercase()
    } else {
        trimmed.replace('\\', "/").to_lowercase()
    };
    // `./` at the front, then any number of leading slashes.
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    s = s.trim_start_matches('/').to_string();
    /* The game's own data folder, spelled as a prefix. Only at the front, and only this
       one name: a mod may well have a folder called `data` deeper in — or at the top —
       and that one is real. */
    if let Some(rest) = s.strip_prefix("data files/") {
        s = rest.trim_start_matches('/').to_string();
    }
    if s.contains("//") {
        while s.contains("//") {
            s = s.replace("//", "/");
        }
    }
    s
}

/// A texture file as a key: the same rule [`norm_mesh`] applies to meshes, for the same
/// reason — `Tx_Moss.dds` written three ways is one texture.
pub fn norm_tex(t: &str) -> String {
    norm_mesh(t)
}

/// A mesh path as a key: lowercased, and slashes both ways read as one.
///
/// `Grass\g.nif`, `grass/g.nif` and `GRASS\G.NIF` are one mesh. A set authored on one
/// person's machine and loaded on another must not miss because of it.
///
/// Round 18bd (C5): lowercased **through the whole alphabet**, not only the ASCII half.
/// `to_ascii_lowercase` leaves high bytes exactly as they are, so `Skały\GŁAZ.nif` in a
/// plugin and `skały\głaz.nif` in a `.statics.toml` became two independent setups that
/// never saw each other — which is precisely the case the paragraph above promises to
/// handle, and Polish, Russian and German mod packs do ship such paths. Nothing has to be
/// migrated: the key is recomputed from the file's own spelling every time a set is read
/// (`setup::read_set`), so an older file simply folds the new way on its next load.
/// The ASCII path is unchanged and still costs one pass.
pub fn norm_mesh(m: &str) -> String {
    let slashed = m.trim().replace('/', "\\");
    let n = if slashed.is_ascii() { slashed.to_ascii_lowercase() } else { slashed.to_lowercase() };
    /* And without the data folder's own prefix: plugins reference meshes *relative to*
       `Meshes\`, the browser hands paths *including* it, and a setup keyed one way
       looked up the other would quietly be two setups. The world's spelling — no
       prefix — is the canonical one, because it is what every placement carries. */
    n.strip_prefix("meshes\\").map(|s| s.to_string()).unwrap_or(n)
}

/// The extensions a load order entry can have and still be a file with records in it.
///
/// Round 18ab. OpenMW's `content=` is not only plugins: `.omwscripts` names a list of Lua
/// scripts, which has no records, no cells and no landscape. Robin's own cfg holds exactly
/// one `content=` line and it is one of those — so the load order was "not empty" by a
/// count of one, the world opened with nothing in it, and the program connected happily to
/// a game with no cells rather than saying what was wrong.
pub const PLUGIN_EXTS: [&str; 4] = [".esm", ".esp", ".omwgame", ".omwaddon"];

/// Whether a file name ends in one of [`PLUGIN_EXTS`] — `.omwaddon` and `.omwgame`
/// included, which is what makes an OpenMW install's own addons countable.
pub fn is_plugin_name(name: &str) -> bool {
    is_plugin(name)
}

pub fn is_plugin(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    PLUGIN_EXTS.iter().any(|e| n.ends_with(e))
}

/// Whether an order file is OpenMW's `openmw.cfg` - the one fact that says which engine
/// reads the load order, where the two engines read a malformed record differently
/// (round 18gf). The name is OpenMW's own and is the same on every platform.
pub fn is_openmw_cfg(order_file: &std::path::Path) -> bool {
    // The leaf after either separator: a Windows path is read on Linux too (the sandbox,
    // a test), where `file_name` would not split on the backslash.
    let s = order_file.to_string_lossy();
    let leaf = s.rsplit(['\\', '/']).next().unwrap_or("");
    leaf.eq_ignore_ascii_case("openmw.cfg")
}
