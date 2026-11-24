// Cell viewer command layer: every call the viewer page makes, registered as a Tauri
// command. Taken from Gardenfell's shell (src-tauri/src/main.rs, (c) 2026 Robin Hjelte,
// MIT) and running on the `viewcore` crate. Line-for-line with its source for now so
// later pruning is reviewable as a diff.
// Tauri shell around the generation engine.
//
// The window loads the existing Gardenfell HTML unchanged. Everything the page
// used to do through the File System Access API — pick a folder, index it, read
// a plugin, scatter a cell, write an .esp — is a command here instead, so the
// heavy work happens on native threads and the UI stays responsive.
//
// The world is loaded once and kept in state; commands borrow it rather than
// re-reading the load order, which is what makes cell preview feel instant
// after the first scan.
//
// # Every command here is `#[tauri::command(async)]`, and that is a rule
//
// Not a preference. Tauri runs a command *without* the `async` marker **on the main
// thread**, and on Windows the WebView2 window lives on that same thread — so a plain
// `#[tauri::command]` doing a hundred milliseconds of work is a hundred milliseconds in
// which the window cannot paint, cannot take a click and cannot spin. That is not a
// background task that takes a while; it is a frozen application. Robin's report was
// "the engine freezes and stutters", and this was the whole of it.
//
// Almost none of these commands touches a `Window` or an `AppHandle` — they are state
// in, JSON out — so every one of them is safe off the main thread, and the marker is
// applied to all of them rather than to a list somebody has to maintain. The one
// exception is `cursor_park`, which moves the mouse pointer and says at its own
// definition why it must and why it is still safe: it does no work, and the runtime
// dispatches the window call to the main thread itself. `build.rs` in
// `gdn_tauri_check` refuses to build if a bare `#[tauri::command]` appears here, because
// the failure it would cause is invisible in every test we have: the browser suites run
// the engine as a separate process behind a pipe, where the page genuinely waits
// instead of blocking.
//
// The bodies are synchronous and contain no `.await`, so the `MutexGuard` on the state
// never crosses a suspension point. They do now run concurrently, and contend on that
// one mutex instead of on the main thread — which is the same serialisation as before,
// moved somewhere it does not stop the window.



use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use viewcore::json::{string_array, J};
use viewcore::world::{read_load_order, World};
use viewcore::layout;
use viewcore::land::{VERTS, VERT_STEP};
use viewcore::vfs::Vfs;

// The pages' own commands, one file each (`commands/`): everything in them is `pub(super)`
// and brought in here, so `builder()` below names every command as before and they call
// each other and what is here freely.
mod conflicts;
mod editor;
mod help;
mod land;
mod loadorder;
mod sound;
mod tools;
#[allow(unused_imports)]
use conflicts::*;
#[allow(unused_imports)]
use help::*;
#[allow(unused_imports)]
use land::*;
#[allow(unused_imports)]
use loadorder::*;
#[allow(unused_imports)]
use tools::*;
#[allow(unused_imports)]
use editor::*;
#[allow(unused_imports)]
use sound::*;

#[derive(Default)]
struct App {
    /// The overlay, shared for the same reason the world is: a cell load asks for a
    /// hundred meshes and textures, and each of those reads should not have to hold the
    /// state lock while it touches the disk.
    vfs: Option<std::sync::Arc<Vfs>>,
    /// The loaded install, shared rather than owned.
    ///
    /// A scatter is the only expensive thing in this file and it reads the world and
    /// nothing else. Behind an `Arc`, a command can take a handle under the lock, let
    /// the lock go, and do the work outside it — so nine cells can be scattered at once
    /// on nine threads rather than one after another behind one mutex (§46). Replaced
    /// wholesale when an install is connected; `Arc::make_mut` for the one thing that
    /// changes it in place, which is installing the synthetic preview patch.
    world: Option<std::sync::Arc<World>>,
    /// Where the connected install's load order came from, and the game's own folder —
    /// what the sky reads its weather from (round 13, `atmosphere`).
    order_file: Option<PathBuf>,
    game_dir: Option<PathBuf>,
    /// The whole resolved layout, kept because the sky needs more of it than the game
    /// folder: which MGE is installed is decided by a d3d8.dll that, under Root Builder,
    /// lives in a mod rather than in the game folder at all (round 17m, `viewcore::mge`).
    layout: Option<viewcore::layout::Layout>,
    /// Round 18bi: the overlay a `plugins_probe` built, waiting for the `open_install`
    /// that follows it.
    ///
    /// Indexing a big MO2 overlay means walking every file of every enabled mod, and the
    /// plugin chooser needs that walk done before it can show anything — so the connect
    /// would otherwise do it twice, once to ask and once to load. Keyed by the install it
    /// belongs to and **taken** rather than borrowed: good for one load, so an
    /// `open_install` that did not come through the chooser never trusts an index of
    /// unknown age.
    pending: Option<(InstallId, Opened)>,
    /// Every distinct mesh the world's STATs wear, fingerprinted: geometry hash
    /// (textures excluded — a retexture hashes the same) and the texture table in
    /// part order. Built once per install by `scan_meshes`, outside the lock, and
    /// dropped on connect for the reason `surfaces` is.
    mesh_scan: Option<std::sync::Arc<std::collections::HashMap<String, (u64, Vec<String>)>>>,
    /// The scan's account of itself while it runs, for the page's progress bar —
    /// shared with the worker threads, read by `statics_scan_note` without waiting.
    scan_note: std::sync::Arc<ScanNote>,
    /// Live placements per mesh, counted once per connected world and shared — the
    /// number beside every twin and every texture-grid tile. Recounting every
    /// reference in the world per question was most of round 4 item 5's "kind of
    /// slow, even with the index". Dropped on connect with the rest.
    ref_counts: Option<std::sync::Arc<std::collections::HashMap<String, u64>>>,
    /// Round 18n: bumped every time the world is replaced or rebuilt in place, so a
    /// cached scatter can never outlive the ground it was thrown on.
    world_rev: u64,
    /// Round 18cp: the one colour of every land texture the world's landscape uses,
    /// for the picker's map — a few hundred file reads, done once per world and keyed by
    /// `world_rev` so a reconnect never paints the old install's palette on the new one.
    /// Shared, because the tiles are painted outside the lock on the pool.
    map_palette: Option<(u64, std::sync::Arc<std::collections::HashMap<String, [u8; 3]>>)>,
}

/// How far through the install the mesh scan is — three numbers the page polls while
/// its dialogue shows a bar (round 3: "it takes a long time to do so with no feedback").
#[derive(Default)]
struct ScanNote {
    busy: std::sync::atomic::AtomicBool,
    done: std::sync::atomic::AtomicUsize,
    total: std::sync::atomic::AtomicUsize,
    /// The mesh being read right now (round 17k). A count moving is a bar; a *name*
    /// moving is the scan visibly doing the thing it says it is doing, which is what
    /// Robin asked the dialogue for: "Make the loading bar actually show that it's
    /// reading meshes, so the user gets a better understanding for when it's done."
    at: std::sync::Mutex<String>,
}

type State<'a> = tauri::State<'a, Mutex<App>>;

/// Round 18bc (B6): the state lock, taken back after a panic instead of lost with it.
///
/// `src-tauri` sets no `panic` profile, so the shipped build unwinds — and a panic while
/// the guard was held **poisoned the mutex**. From then on every command failed, including
/// `config_toml` and `profile_write`: the paint mask and the palette were still sitting in
/// memory but could no longer be read out, so the session's painting was unrecoverable
/// and the only way out was to kill the window. Nothing in the tree called `clear_poison`
/// or `into_inner`.
///
/// What hid it is that the CLI's `serve.rs` wraps every dispatch in `catch_unwind` and
/// answers "<cmd> panicked", so in the browser suites a panic is one failed call and the
/// run carries on; the desktop's "every later command fails forever" was exercised
/// nowhere.
///
/// `App` is a plain bag of data, and a half-applied edit beats a dead session — the
/// person can still save their work and restart. So the poison is stepped over.
trait LockApp {
    fn app(&self) -> std::sync::MutexGuard<'_, App>;
}

impl LockApp for Mutex<App> {
    fn app(&self) -> std::sync::MutexGuard<'_, App> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Connect an install and load the whole load order.
///
/// `kind` is "vanilla", "mo2" or "openmw"; `path` is the one thing the user
/// picked — a Data Files folder, an MO2 base folder, or an openmw.cfg. Working
/// the overlay out from that lives in the engine, so a single folder pick is
/// enough and the page never has to understand a mod manager's layout.
/* Round 18bi: everything a connect does *before* it parses a plugin, in one place.
   
   It used to be the first seventy lines of `open_install` and nothing else needed it.
   Now the plugin chooser does: to show the load order and say what needs what, it has to
   resolve the install and index the overlay, and that walk — every file of every enabled
   mod, and Robin has hundreds — is far too expensive to do twice per connect. So the
   probe does it, hands the result to `App.pending`, and the `open_install` that follows
   takes it rather than repeating it. */
struct Opened {
    l: layout::Layout,
    vfs: Vfs,
    /// What the install has switched **on**, in load order. This is what a run parses when
    /// nobody has chosen otherwise.
    order: Vec<String>,
    /// Round 18bl: every plugin file the overlay holds, with where each is on disk.
    ///
    /// Kept raw rather than arranged. Working out the display order means reading a header
    /// per plugin for OpenMW's dependency walk, and a connect that never opens the chooser
    /// — the command line, a scripted run, every browser suite — would be paying for a list
    /// nothing reads. `plugins::arrange` is called where the answer is wanted.
    on_disk: Vec<(String, PathBuf)>,
    order_file: String,
    dirs: Vec<(String, PathBuf)>,
    warnings: Vec<String>,
    /// Round 18cr: what the overlay cost, for the connect log - the layout, the index
    /// walk over every root, the archives, the plugin listing; milliseconds each.
    ms: [u64; 4],
    /// Wraithguard: how many roots came from the saved index and how many were walked.
    index_stats: viewcore::vfsindex::Stats,
}

/// The install identity a pending overlay belongs to. A connect to anything else rebuilds.
type InstallId = (String, String, Option<String>);

/* Wraithguard: which step a connect is on, for the busy card. `open_install` is one call
   that runs a minute on a large install (621 plugins: the file index, the archives, then
   reading, parsing and merging every plugin), and the card said "resolving the overlay"
   all through it (Static, 2026-10-18: 1m 05s at 8%). The page asks `connect_stage` while
   it waits; the steps are the ones this file can see from outside the world's load. */
static CONNECT_STEP: Mutex<(&str, u64, Option<std::time::Instant>)> = Mutex::new(("", 0, None));

fn connect_step(step: &'static str, n: u64) {
    if let Ok(mut s) = CONNECT_STEP.lock() {
        *s = (step, n, Some(std::time::Instant::now()));
    }
}

/// Clears the step when a connect ends, however it ends.
struct ConnectDone;
impl Drop for ConnectDone {
    fn drop(&mut self) {
        connect_step("", 0);
    }
}

/// The connect's current step (`index`, `archives`, `load`, `finish`; empty when none is
/// running), its count (roots for `index`, plugins for `load`) and how long it has run.
#[tauri::command(async)]
fn connect_stage() -> String {
    let (step, n, at) = CONNECT_STEP.lock().map(|s| *s).unwrap_or(("", 0, None));
    let mut o = J::obj();
    o.str("step", step).int("n", n).int("ms", at.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0));
    o.done()
}

fn open_overlay(kind: &str, path: &str, profile: Option<&str>) -> Result<Opened, String> {
    let p = PathBuf::from(path);
    let clock = std::time::Instant::now();
    let lap = |since: &mut u128| { let now = clock.elapsed().as_millis(); let d = now - *since; *since = now; d as u64 };
    let mut since = 0u128;
    let l = match kind {
        "mo2" => layout::mo2_profile(&p, profile)?,
        "openmw" => layout::openmw(&p)?,
        _ => layout::vanilla(&p)?,
    };

    let dirs: Vec<(String, PathBuf)> = l
        .roots
        .iter()
        .map(|p| {
            let label = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            (label, p.clone())
        })
        .collect();

    let order_file = l.order_file.to_string_lossy().to_string();
    let mut order = read_load_order(&l.order_file);
    /* Wraithguard: a plain Morrowind install loads its plugins by the dates of the files, masters
       first; its ini says which load, not in what order, and `read_load_order` gives the ini's
       `GameFileN` order. Every other page reads the setup through `ordercore::install`, which
       sorts it by date (`by_date`); the world has to be built in the same order or it draws
       what the game does not load. MO2's list and an OpenMW cfg are the order as written. */
    if l.mo2_base.is_none() && !viewcore::luascan::is_openmw_cfg(&l.order_file) {
        let data: Vec<PathBuf> = l.roots.iter().rev().filter(|d| d.is_dir()).cloned().collect();
        order = viewcore::ordercore::install::by_date(&order, &data);
    }
    /* Wraithguard: and the `groundcover=` plugins after them, as OpenMW loads them - their
       references are the grass the viewer draws (`World::groundcover`, addGroundcoverRuns).
       Gardenfell's `read_load_order` reads `content=` only (it makes its own grass), and
       since the 18hu rebase no grass was drawn at all (Static, 2026-10-13). */
    if viewcore::luascan::is_openmw_cfg(&l.order_file) {
        for g in viewcore::world::read_groundcover(&l.order_file) {
            if !order.iter().any(|o| o.eq_ignore_ascii_case(&g)) {
                order.push(g);
            }
        }
    }
    if order.is_empty() {
        /* Round 18ab. A config can name every directory in a setup and still not say what
           to load. Robin's does: 912 `data=` lines, the vanilla Data Files among them, and
           not one `content=` line naming a plugin — the shape a mod manager's OpenMW export
           leaves when it writes the directories but not the enabled plugin list, and the
           shape the launcher leaves before anything has been ticked in its Data Files tab.
           A data directory is only a place to look; a plugin is loaded because a `content=`
           line names it, and the order those lines are in *is* the load order. So the
           program had nothing to open and said so far too quietly. This says what is
           missing, proves the file was read by counting what it did find, and names the two
           ways the list gets written. */
        if kind == "openmw" {
            return Err(viewcore::msg!(
                if dirs.len() == 1 { "eng.openmw_no_content.one" } else { "eng.openmw_no_content.other" },
                file = order_file,
                n = dirs.len()
            ));
        }
        return Err(viewcore::msg!("eng.order_no_plugins", file = order_file));
    }

    let ms_layout = lap(&mut since);
    /* Wraithguard: the index from the last connect where its folders are unchanged
       (`vfsindex`): walking every file of every root was about 4 s on a large install. */
    let cache = app_dir().ok().map(|d| d.join(viewcore::vfsindex::FILE));
    connect_step("index", dirs.len() as u64);
    let (mut vfs, index_stats) = viewcore::vfsindex::overlay(&dirs, cache.as_deref());
    let ms_index = lap(&mut since);
    // The archives (round 11 item 10), opened by the overlay itself since round 18bo so
    // that `gdn_cli` cannot end up with a different one — see `Vfs::add_archives_named_by`.
    let mut warnings = l.warnings.clone();
    connect_step("archives", 0);
    warnings.extend(vfs.add_archives_named_by(&l.order_file));
    let ms_archives = lap(&mut since);
    let on_disk = vfs.plugin_files();
    let ms_plugins = lap(&mut since);
    Ok(Opened { l, vfs, order, on_disk, order_file, dirs, warnings, ms: [ms_layout, ms_index, ms_archives, ms_plugins], index_stats })
}

/// Every plugin an install has, read and put in the order that install would show them.
///
/// One header per plugin, and the arrangement needs them: OpenMW's unselected addons are
/// ordered by what they rest on. So the entries are built once against an unordered set and
/// then put in the order `arrange` decided, rather than read twice.
fn plugin_entries(
    l: &layout::Layout,
    vfs: &Vfs,
    order: &[String],
    on_disk: &[(String, PathBuf)],
) -> Vec<viewcore::plugins::Entry> {
    let mut names: Vec<String> = order.to_vec();
    let seen: std::collections::HashSet<String> =
        order.iter().map(|n| n.to_ascii_lowercase()).collect();
    for (n, _) in on_disk {
        if !seen.contains(&n.to_ascii_lowercase()) {
            names.push(n.clone());
        }
    }
    let read = viewcore::plugins::list(&names, order, |n| {
        vfs.resolve_disk(n).map(|p| p.to_path_buf())
    });
    let arranged = viewcore::plugins::arrange(l, order, on_disk, &read);
    let mut by_name: std::collections::HashMap<String, viewcore::plugins::Entry> =
        read.into_iter().map(|e| (e.name.to_ascii_lowercase(), e)).collect();
    arranged.iter().filter_map(|n| by_name.remove(&n.to_ascii_lowercase())).collect()
}


#[tauri::command(async)]
fn open_install(
    kind: String,
    path: String,
    profile: Option<String>,
    plugins: Option<Vec<String>>,
    last: Option<String>,
    state: State,
) -> Result<String, String> {
    /* The overlay the chooser already built, when this is the connect it was built for.
       Taken rather than borrowed: it is good for exactly one load, so a later
       `open_install` that never went through a probe always reads the disk afresh rather
       than trusting an index of unknown age. */
    let _done = ConnectDone;
    let want: InstallId = (kind.clone(), path.clone(), profile.clone());
    let reuse = {
        let mut app = state.app();
        match app.pending.take() {
            Some((id, o)) if id == want => Some(o),
            _ => None,
        }
    };
    let reused = reuse.is_some();
    let Opened { l, vfs, order, on_disk, order_file, dirs, mut warnings, ms: ms_overlay, index_stats } =
        match reuse {
            Some(o) => o,
            None => open_overlay(&kind, &path, profile.as_deref())?,
        };

    /* Round 18bi: which of the order to actually parse. Absent means all of it — never
       none of it, which is the mistake worth being deliberate about. The masters of
       anything chosen come along whether or not they were named, because a load order
       missing a master is one the game would refuse and a plugin generated against it
       would be wrong in ways nothing says out loud. */
    let order: Vec<String> = match &plugins {
        None => order,
        Some(want) => {
            /* Narrowed over everything the install **has**, not over what it had switched
               on — round 18bl let somebody tick a plugin their load order does not name,
               and filtering against the load order would drop it again on the way past.
               Its position comes from `available`, which is where the install would show
               it. */
            let all = plugin_entries(&l, &vfs, &order, &on_disk);
            let available: Vec<String> = all.iter().map(|e| e.name.clone()).collect();
            let chosen = viewcore::plugins::narrow(&available, Some(want));
            viewcore::plugins::with_masters(&all, &chosen)
        }
    };
    if plugins.is_some() {
        warnings.push(format!("loading {} of the load order's plugins, as chosen", order.len()));
    }
    /* Wraithguard: the plugin the Editor edits (its active file), drawn last, as the Editor
       reads it: a record a later plugin overrides shows as the plugin being edited has it,
       not as the game would (Static, 2026-11-10). A plugin's references name their masters
       by name, so its place in the order changes which version of a record wins and nothing
       else. */
    let mut order = order;
    // (No `let` chains: check-commands compiles this file as edition 2021.)
    let last = last.as_deref().map(str::trim).filter(|l| !l.is_empty()).and_then(|l| order.iter().position(|n| n.eq_ignore_ascii_case(l)));
    if let Some(at) = last.filter(|at| at + 1 < order.len()) {
        let mine = order.remove(at);
        warnings.push(format!("{mine} is drawn last: it is the plugin the Editor edits"));
        order.push(mine);
    }

    let paths: Vec<PathBuf> = order
        .iter()
        .filter_map(|n| vfs.resolve_disk(n).map(|p| p.to_path_buf()))
        .collect();
    let missing: Vec<String> = order
        .iter()
        .filter(|n| vfs.resolve_disk(n).is_none())
        .cloned()
        .collect();

    /* Round 18al: what the engine the install runs does with a script's PlayGroup - see
       `LoadOpts`. The Code Patch's own record is beside Morrowind.exe; under its "Improved
       animation support" a flagless PlayGroup waits behind the object's Idle, and a
       scripted banner hangs still in the game. Read and reported - and then set aside:
       Robin, on being told: "for banners we make an exception: Make them wave as they
       did before. Only outdoors though" - so a script always earns the twin here, and
       the indoors half is `World::indoor_plain`. */
    let mcp = viewcore::mge::mcp_features(&l);
    let mcp_still = mcp.as_ref().map(|f| f.contains(&viewcore::mge::MCP_IMPROVED_ANIMATION)).unwrap_or(false);
    /* Wraithguard: the engine the setup runs decides how a malformed MVRF reads (an
       openmw.cfg: OpenMW's positional rule); `World::load_with` adds OpenMW's rule for an
       id defined as two kinds. */
    connect_step("load", paths.len() as u64);
    let world = World::load_with(
        &paths,
        &vfs,
        true,
        &viewcore::world::LoadOpts { script_plays: true, ..viewcore::world::LoadOpts::for_order_file(&l.order_file) },
    );
    let mut world = world;
    connect_step("finish", 0);
    // Wraithguard: which plugins are grass (`groundcover=`), so their references go to
    // the grass renderer.
    let gc = if viewcore::luascan::is_openmw_cfg(&l.order_file) {
        viewcore::world::read_groundcover(&l.order_file)
    } else {
        Vec::new() // a Morrowind.ini has no groundcover
    };
    world.groundcover = world
        .plugins
        .iter()
        .enumerate()
        .filter(|(_, p)| gc.iter().any(|g| g.eq_ignore_ascii_case(p)))
        .map(|(i, _)| i)
        .collect();
    let s = world.stats;
    /* Round 18bc (B8): a plugin that resolved and then could not be read used to load as
       an empty file and be listed as loaded. It is a warning on the connect report now,
       beside the ones for a plugin that did not resolve at all. */
    for f in &world.plugin_failures {
        warnings.push(format!("plugin {f}"));
    }

    let mut out = J::obj();
    out.int("plugins", s.plugins as u64)
        /* Round 18al: the Code Patch, when its record was found, and whether the game
           under it keeps its scripted banners still - for the connect log's line. */
        .int("mcpFeatures", mcp.as_ref().map(|f| f.len() as u64).unwrap_or(0))
        .bool("mcpFound", mcp.is_some())
        .bool("mcpStillBanners", mcp_still)
        /* What the mod manager said was switched on, and what it skipped. The overlay is
           built from the first alone — this is that claim, as a number a person can check
           against their own manager. */
        .int("mods", l.mods_on as u64)
        .int("modsOff", l.mods_off as u64)
        .int("files", vfs.file_count() as u64)
        .int("archives", vfs.archives.len() as u64)
        .int("packed", vfs.packed_count() as u64)
        .raw("archiveNames", &string_array(&vfs.archives.iter().map(|a| a.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()).collect::<Vec<_>>()))
        .int("bytes", s.bytes)
        .int("records", s.records as u64)
        .int("cells", s.cells as u64)
        .int("lands", s.lands as u64)
        .int("refsKept", s.refs_merged as u64)
        .int("refsOverridden", s.refs_overridden as u64)
        .int("refsDeleted", s.refs_deleted as u64)
        // Gardenfell 18fc: later records for a deleted reference, dropped.
        .int("refsKeptDeleted", s.refs_kept_deleted as u64)
        .int("refsMoved", s.refs_moved as u64)
        // Gardenfell 18gf: the merge rules that changed, so the connect log can say what
        // moved a digest.
        .int("refsMoveDropped", s.refs_move_dropped as u64)
        .int("refsRelocated", s.refs_relocated as u64)
        .int("cellsDeleted", s.cells_deleted as u64)
        .int("recordsDeletedByFlag", s.records_deleted_by_flag as u64)
        .int("landsDefaulted", s.lands_defaulted as u64)
        .int("landsFlaggedOut", s.lands_flagged_out as u64)
        .int("msRead", s.ms_read as u64)
        .int("msParse", s.ms_parse as u64)
        .int("msMerge", s.ms_merge as u64)
        // Wraithguard: the merge's two halves - the loop, and the wait on the mesh read.
        .int("msMergeLoop", s.ms_merge_loop as u64)
        .int("msGeomWait", s.ms_geometry_wait as u64)
        .int("msMergeHistory", s.ms_merge_history as u64)
        .int("msMergeInserts", s.ms_merge_inserts as u64)
        // Wraithguard: the whole loop by part, `{"setup": ms, "objects": ms, ...}`.
        .raw("msMergeParts", &format!("{{{}}}", viewcore::world::MERGE_PARTS.iter()
            .zip(s.ms_merge_parts).map(|(k, v)| format!("\"{k}\":{v}")).collect::<Vec<_>>().join(",")))
        // Wraithguard: inside definitions - lights, models, actors, twin lookups, .kf reads
        // (ms), and model records, twin lookups, .kf files read (counts).
        .raw("msDefs", &format!("[{}]", s.ms_defs.map(|v| v.to_string()).join(",")))
        .raw("defsCounts", &format!("[{}]", s.defs_counts.map(|v| v.to_string()).join(",")))
        // Wraithguard: the twins answered ahead, before the loop - ms, then meshes asked
        // and .kf files read.
        .int("msTwinsAhead", s.ms_twins_ahead as u64)
        .raw("twinsAhead", &format!("[{}]", s.twins_ahead.map(|v| v.to_string()).join(",")))
        // Wraithguard: the mesh geometry read beside the merge - listing, count, wall time,
        // then summed over threads: reading, the .kf beside, parsing (ms), and bytes; and
        // what load_geometry kept: wanted, read late, not found, unreadable, its own ms.
        .int("msGeomList", s.ms_geometry_list as u64)
        .int("meshPre", s.mesh_pre as u64)
        .int("threads", viewcore::pool::threads() as u64)
        .int("msGeomRead", s.ms_geometry_read as u64)
        .raw("geomClock", &format!("[{}]", s.geometry_clock.map(|v| v.to_string()).join(",")))
        .raw("meshKept", &format!("[{},{},{},{},{}]", s.mesh_wanted, s.mesh_late, s.mesh_unresolved, s.mesh_unparsed, s.ms_meshes))
        /* Round 18cr: and the overlay's own clock - the layout, the index walk, the
           archives, the plugin listing - whether it was built here or by the probe
           before (`overlayReused` says which, so the page adds it to the right step). */
        .int("msLayout", ms_overlay[0]).int("msIndex", ms_overlay[1])
        .int("msArchives", ms_overlay[2]).int("msPlugins", ms_overlay[3])
        // Wraithguard: the saved file index - roots taken from it, roots walked again.
        .int("indexReused", index_stats.reused as u64).int("indexWalked", index_stats.walked as u64)
        .bool("overlayReused", reused)
        .raw("order", &string_array(&order))
        /* Per opened plugin, in `order` order, the masters its header names — the report
           lists them under each plugin and flags one the order does not load. */
        .raw("masters", &format!("[{}]", world.masters.iter().map(|m| string_array(m)).collect::<Vec<_>>().join(",")))
        .raw("missing", &string_array(&missing))
        .raw("roots", &string_array(&dirs.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>()))
        .raw("warnings", &string_array(&warnings))
        .str("orderFile", &order_file)
        .str("label", &l.label);

    /* Remembered only now, after the load order actually read. Recording the pick
       earlier would mean coming back tomorrow to a path that never worked. */
    if let Ok(dir) = viewcore::profiles::dir_beside_exe() {
        let _ = viewcore::profiles::remember_install(
            &dir,
            &viewcore::profiles::Install {
                kind: kind.clone(),
                path: path.clone(),
                profile: profile.clone().unwrap_or_default(),
            },
        );
    }

    let mut app = state.app();
    app.vfs = Some(std::sync::Arc::new(vfs));
    app.world = Some(std::sync::Arc::new(world));
    app.world_rev += 1;
    app.order_file = Some(l.order_file.clone());
    app.game_dir = l.game_dir.clone();
    app.layout = Some(l.clone());
    // The twin scan too: fingerprints of the files behind the old overlay. The counters
    // go with it, so a bar polled before the next scan starts does not show the old one.
    app.mesh_scan = None;
    app.ref_counts = None;
    {
        use std::sync::atomic::Ordering::Relaxed;
        app.scan_note.busy.store(false, Relaxed);
        app.scan_note.done.store(0, Relaxed);
        app.scan_note.total.store(0, Relaxed);
        if let Ok(mut a) = app.scan_note.at.lock() { a.clear(); }
    }
    Ok(out.done())
}

/// The install to reopen at startup, and where openmw.cfg lives on this machine.
///
/// `install` is null when nothing is remembered or when the remembered path has
/// gone — an unplugged drive is a reason to come up unconnected, not an error.
/// `openmwCfg` is returned whether or not the file exists, with `openmwCfgExists`
/// saying which, so the page can go straight there or merely start a picker there.
#[tauri::command(async)]
fn startup_install() -> Result<String, String> {
    let dir = viewcore::profiles::dir_beside_exe()?;
    let mut out = J::obj();
    match viewcore::profiles::install(&dir) {
        Some(i) => {
            let mut o = J::obj();
            o.str("kind", &i.kind).str("path", &i.path).str("profile", &i.profile);
            out.raw("install", &o.done());
        }
        None => {
            out.raw("install", "null");
        }
    }
    match viewcore::profiles::openmw_cfg() {
        Some(p) => {
            out.str("openmwCfg", &p.display().to_string()).bool("openmwCfgExists", p.exists());
        }
        None => {
            out.raw("openmwCfg", "null").bool("openmwCfgExists", false);
        }
    }
    /* Round 18cj: the colour theme, so the window can come up wearing it. Robin asked for
       it to be "saved both to the profile, and the profiles.toml file, so it can be set
       directly on program start" — the profile is read several steps later, after the
       install is reopened, and until then the window would be showing the default. */
    out.str("theme", &viewcore::profiles::theme(&dir).unwrap_or_default());
    /* Round 18cp: how the cell picker was last shown, so it opens the way it was left.
       Robin: "Remember List/Map between runs." */
    out.str("cellPicker", &viewcore::profiles::cell_picker(&dir));
    Ok(out.done())
}

/// Remembers the window's colour theme for the next start-up (round 18cj).
///
/// The profile is where the choice *lives* — a shared profile carries its author's theme
/// — and this is the machine-local copy that start-up can read before any profile has
/// been opened. Called when the theme is picked and when a profile that names one is
/// opened, so the two never disagree about what this machine last showed.
#[tauri::command(async)]
fn theme_set(code: String) -> Result<String, String> {
    let dir = viewcore::profiles::dir_beside_exe()?;
    viewcore::profiles::set_theme(&dir, &code)?;
    Ok(J::obj().str("theme", code.trim()).done())
}

/// Remembers how the cell picker is shown — `"map"` or `"list"` — for the next start-up
/// (round 18cp). Machine-local, like the theme's copy: it is a fact about how this person
/// likes to pick a cell on this machine, not about any profile.
#[tauri::command(async)]
fn picker_set(mode: String) -> Result<String, String> {
    let dir = viewcore::profiles::dir_beside_exe()?;
    viewcore::profiles::set_cell_picker(&dir, &mode)?;
    Ok(J::obj().str("cellPicker", &viewcore::profiles::cell_picker(&dir)).done())
}

/// The colours the map is painted with, made once per world and kept (round 18cp).
///
/// Outside the lock for the reading — a few hundred textures off the overlay — and under
/// it only to file the result, keyed by `world_rev` so a reconnect starts over.
fn map_palette(state: &State) -> Result<(std::sync::Arc<World>, std::sync::Arc<std::collections::HashMap<String, [u8; 3]>>), String> {
    let (w, v, rev, have) = {
        let app = state.app();
        let w = app.world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
        let v = app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
        let have = app.map_palette.as_ref().filter(|(r, _)| *r == app.world_rev).map(|(_, p)| p.clone());
        (w, v, app.world_rev, have)
    };
    if let Some(p) = have {
        return Ok((w, p));
    }
    // Wraithguard: the cheap read (`viewcore::distant::palette`): small mips, not whole files.
    let pal = std::sync::Arc::new(viewcore::distant::palette(&w, &v));
    {
        let mut app = state.app();
        // The world may have moved on while the textures were being read; a palette for
        // an older world is not filed, and the next asker reads again.
        if app.world_rev == rev {
            app.map_palette = Some((rev, pal.clone()));
        }
    }
    Ok((w, pal))
}

/// What the picker's map is made of (round 18cp): every cell with a landscape, north-up
/// row order, the tile size, and the world's extent — so the page can size its canvas
/// and ask for the tiles in batches. Reading the palette is the slow part of the first
/// call, and it is done here so `world_map_tiles` never has to.
///
/// Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed; a row a
/// cell, tens of thousands on a large install): `map` `us` (the tile size, the open sea's
/// CSS color), `bounds` `iiii` (min x y, max x y; no row when there are no cells) and
/// `cells` `ii`, in the order `world_map_tiles` counts them. Read by 31_cellmap.js.
#[tauri::command(async)]
fn world_map(state: State) -> Result<tauri::ipc::Response, String> {
    let (w, _) = map_palette(&state)?;
    let cells = viewcore::worldmap::cells(&w);
    let mut tb = Tables::new();
    tb.table("map", "us", 1);
    // Gardenfell 18hp: the open sea's color, so the page needs no literal of its own.
    tb.u(viewcore::worldmap::TILE as u64).s(&viewcore::worldmap::open_sea_css());
    tb.table("bounds", "iiii", usize::from(!cells.is_empty()));
    if !cells.is_empty() {
        let x0 = cells.iter().map(|c| c.0).min().unwrap_or(0);
        let y0 = cells.iter().map(|c| c.1).min().unwrap_or(0);
        let x1 = cells.iter().map(|c| c.0).max().unwrap_or(0);
        let y1 = cells.iter().map(|c| c.1).max().unwrap_or(0);
        tb.i(x0 as i64).i(y0 as i64).i(x1 as i64).i(y1 as i64);
    }
    tb.table("cells", "ii", cells.len());
    for (x, y) in &cells {
        tb.i(*x as i64).i(*y as i64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// A batch of the map's tiles (round 18cp): `count` cells from index `from` of the list
/// `world_map` gave, packed as `viewcore::worldmap::pack` says. Batched so a
/// four-thousand-cell world crosses the shell in pieces the page can paint as they land,
/// with a count under them, rather than in one twenty-megabyte reply.
#[tauri::command(async)]
fn world_map_tiles(from: u32, count: u32, state: State) -> Result<tauri::ipc::Response, String> {
    let (w, pal) = map_palette(&state)?;
    let cells = viewcore::worldmap::cells(&w);
    let a = (from as usize).min(cells.len());
    let b = (a + count as usize).min(cells.len());
    Ok(tauri::ipc::Response::new(viewcore::worldmap::pack(&w, &pal, &cells[a..b])))
}

/// Wraithguard: distant land (`viewcore::distant`) - every exterior within `radius`
/// cells of (`x`, `y`) and outside the `inner` rings the scene loads whole, coarse
/// heights and the ground's colour at each point. Read with the map's palette, which the
/// first call reads (as `world_map` does) and every later one reuses.
#[tauri::command(async)]
fn distant_land(x: i32, y: i32, inner: i32, radius: i32, state: State) -> Result<tauri::ipc::Response, String> {
    let (w, pal) = map_palette(&state)?;
    let radius = radius.clamp(0, 64);
    Ok(tauri::ipc::Response::new(viewcore::distant::pack(&w, &pal, (x, y), inner.max(0), radius)))
}

/// Every land texture the loaded world knows about, as bytes (`WGTB`): `textures` (`ss`: id,
/// file).
///
/// The id and the file are both needed: an ini section names the LTEX id, but
/// what gets drawn is the filename in that record's DATA field, and the two are
/// rarely the same string. Sending only the id leaves the terrain untextured.
/// The JSON command of the same list was retired 2026-11-17 (Static): the bytes are the one way.
#[tauri::command(async)]
fn textures_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let list = land_textures(w);
    let mut t = Tables::new();
    t.table("textures", "ss", list.len());
    for (id, file) in &list {
        t.s(id).s(file);
    }
    Ok(tauri::ipc::Response::new(t.done()))
}

/// Every land texture's id and the file it draws, the unpainted ground first.
fn land_textures(w: &viewcore::world::World) -> Vec<(String, String)> {
    let mut ids: Vec<String> = Vec::new();
    for m in &w.ltex_by_plugin {
        for id in m.values() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    ids.sort();
    // Unpainted ground has no LTEX record, but it is still a texture a user can
    // target, so it belongs in the list beside the real ones.
    ids.insert(0, viewcore::world::DEFAULT_LTEX.to_string());
    ids.into_iter()
        .map(|id| {
            let file = w.ltex_file.get(&id.to_ascii_lowercase()).cloned().unwrap_or_default();
            (id, file)
        })
        .collect()
}

/// Named cells and regions, for the scope picker, as bytes (`WGTB`): `cells` (`s`) and
/// `regions` (`s`).
/// The JSON command of the same list was retired 2026-11-17 (Static): the bytes are the one way.
#[tauri::command(async)]
fn scopes_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let (cells, regions) = scope_names(w);
    Ok(tauri::ipc::Response::new(pack_names(&[("cells", &cells), ("regions", &regions)])))
}

/// Tables of one string column each, in the order given.
fn pack_names(lists: &[(&str, &[String])]) -> Vec<u8> {
    let mut t = Tables::new();
    for (name, list) in lists {
        t.table(name, "s", list.len());
        for v in list.iter() {
            t.s(v);
        }
    }
    t.done()
}

/// The named cells and the regions, sorted and each once.
fn scope_names(w: &viewcore::world::World) -> (Vec<String>, Vec<String>) {
    let mut cells: Vec<String> = w
        .cells
        .values()
        .filter(|c| !c.name.is_empty())
        .map(|c| c.name.clone())
        .collect();
    cells.sort();
    cells.dedup();
    let mut regions: Vec<String> = w.regions.values().cloned().collect();
    regions.sort();
    regions.dedup();
    (cells, regions)
}

/// Every cell, as bytes (`WGTB`): `grid` (`iissuu`: x, y, name, region, references, has a
/// pathgrid) and `interiors` (`suu`: name, references, has a pathgrid).
///
/// The cell picker needs the whole grid, not just the named cells: an unnamed
/// wilderness cell is just as pickable, and its region still decides which selector
/// governs it.
///
/// The grid is exteriors, and there is nowhere in it for an interior to be: `World.cells`
/// is keyed by grid position and an interior has none. They come back beside it, by name,
/// in `interiors` — nothing places grass in one yet, but the picker has to be able to
/// *name* a place before anything can be done with it. See memory §18 260.
/// The JSON command of the same list was retired 2026-11-17 (Static): the bytes are the one way.
#[tauri::command(async)]
fn cells_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let (grid, ints) = cell_lists(w);
    Ok(tauri::ipc::Response::new(pack_cells(&grid, &ints)))
}

/// One place in the cell lists: an exterior (with its grid square and region) or an
/// interior (`x`, `y` and `region` unused).
struct CellRow {
    x: i32,
    y: i32,
    name: String,
    region: String,
    refs: usize,
    pathgrid: bool,
}

fn pack_cells(grid: &[CellRow], ints: &[CellRow]) -> Vec<u8> {
    let mut t = Tables::new();
    t.table("grid", "iissuu", grid.len());
    for c in grid {
        t.i(c.x as i64).i(c.y as i64).s(&c.name).s(&c.region).u(c.refs as u64).u(u64::from(c.pathgrid));
    }
    t.table("interiors", "suu", ints.len());
    for c in ints {
        t.s(&c.name).u(c.refs as u64).u(u64::from(c.pathgrid));
    }
    t.done()
}

/// The exteriors (in the world's order) and the interiors (by name, case aside).
fn cell_lists(w: &viewcore::world::World) -> (Vec<CellRow>, Vec<CellRow>) {
    let grid = w
        .cells
        .iter()
        .map(|(g, c)| CellRow {
            x: g.0,
            y: g.1,
            name: c.name.clone(),
            region: c.region.clone(),
            refs: c.refs.len(),
            pathgrid: w.pathgrids_ext.contains_key(g),
        })
        .collect();
    let mut ints: Vec<&viewcore::world::Interior> = w.interiors.values().collect();
    ints.sort_by_key(|a| a.name.to_lowercase());
    let ints = ints
        .into_iter()
        .map(|c| CellRow {
            x: 0,
            y: 0,
            name: c.name.clone(),
            region: String::new(),
            refs: c.refs.len(),
            pathgrid: w.pathgrids_int.contains_key(&viewcore::world::room_key(&c.name)),
        })
        .collect();
    (grid, ints)
}

/// Which cells and regions each texture is painted in, for the scope picker.
///
/// Wraithguard: as bytes (`WGTB`): `textures` (`s`: every texture, in the
/// map's order) and `uses` (`sus`: texture, 0 a cell / 1 a region, its name), so a texture
/// that is in the map with nothing under it keeps its entry.
/// The JSON command of the same list was retired 2026-11-17 (Static): the bytes are the one way.
#[tauri::command(async)]
fn texture_usage_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let usage = w.texture_usage();
    let n: usize = usage.values().map(|(c, r)| c.len() + r.len()).sum();
    let mut t = Tables::new();
    t.table("textures", "s", usage.len());
    for tex in usage.keys() {
        t.s(tex);
    }
    t.table("uses", "sus", n);
    for (tex, (cells, regions)) in &usage {
        for c in cells {
            t.s(tex).u(0).s(c);
        }
        for r in regions {
            t.s(tex).u(1).s(r);
        }
    }
    Ok(tauri::ipc::Response::new(t.done()))
}

/// The mesh and texture files the overlay exposes, for the asset browsers. The page used
/// to walk the folders itself through the File System Access API; here the index already
/// exists as a side effect of connecting, so it is just handed over.
///
/// Wraithguard: as bytes (`WGTB`): `meshes` (`s`) and `textures` (`s`) - on a big
/// install hundreds of thousands of names, the list a JSON answer least wants to be.
/// The JSON command of the same list was retired 2026-11-17 (Static): the bytes are the one way.
#[tauri::command(async)]
fn assets_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let v = vfs_of(&state)?;
    let meshes = v.list("meshes/", &[".nif"]);
    let textures = v.list("textures/", viewcore::vfs::Vfs::TEX_EXTS);
    Ok(tauri::ipc::Response::new(pack_names(&[("meshes", &meshes), ("textures", &textures)])))
}

/// Everything the viewport needs to draw one cell, so the page never opens a
/// plugin itself.
///
/// This is the command that matters most for stability. The page used to read every
/// .esm and .esp itself to build an id-to-mesh map; doing that in the page meant
/// marshalling the entire load order — hundreds of megabytes — across the IPC
/// boundary, which simply kills the webview. The world is already parsed on this
/// side, so a cell costs one lookup.
#[tauri::command(async)]
fn cell_data(
    gx: i32,
    gy: i32,
    review: Option<String>,
    without: Option<bool>,
    land_mode: Option<String>,
    state: State,
) -> Result<tauri::ipc::Response, String> {
    // A cell load asks for one of these per cell; done under the lock they would queue
    // behind every scatter. See `snapshot`.
    let w = { state.app().world.clone() }
        .ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    /* Wraithguard: the merged-lands preview - this cell's ground as Merged Lands would
       build it under `land_mode` (`mland.rs`), when two or more plugins edit it. */
    let merged = land_mode
        .as_deref()
        .and_then(viewcore::mland::Strategy::parse)
        .and_then(|s| viewcore::mland::merge(&w, (gx, gy), s));
    let land = merged.as_ref().map(|m| &m.land).or_else(|| w.lands.get(&(gx, gy)));
    let tex_id = |v: u16, plugin: i32| -> Option<String> {
        match &merged {
            Some(m) => Some(if v == 0 { viewcore::world::DEFAULT_LTEX.to_string() } else { m.textures.get(v as usize - 1)?.clone() }),
            None => w.ltex_for(v, plugin).map(|s| s.to_string()),
        }
    };
    let cell = w.cells.get(&(gx, gy));

    /* Wraithguard: the ground's grids cross as bytes (`pack_cell`), not JSON text. Static
       2026-11-17: JSON only where it is needed; these were about 100 KB of text a cell. */
    let mut heights: Vec<f32> = Vec::new();
    let mut vtex: Vec<u16> = Vec::new();
    let mut ltex = String::from("{");
    if let Some(l) = land {
        // Rounded to a tenth (`{:.1}` read back), the same f32 the page made of the text.
        heights = l.heights.iter().map(|h| format!("{h:.1}").parse::<f64>().unwrap_or(0.0) as f32).collect();
        let mut seen: Vec<u16> = Vec::new();
        for v in l.vtex.iter() {
            vtex.push(*v);
            if *v != 0 && !seen.contains(v) {
                seen.push(*v);
                if let Some(id) = tex_id(*v, l.plugin) {
                    if ltex.len() > 1 {
                        ltex.push(',');
                    }
                    // keyed by v-1, which is what the page's resolver looks up
                    ltex.push_str(&format!("\"{}\":", v - 1));
                    ltex.push('"');
                    viewcore::json::escape_into(&id, &mut ltex);
                    ltex.push('"');
                }
            }
        }
    }
    ltex.push('}');

    /* The ground normal at every vertex, as the *engine* resolves it — its own VNML
       where the record has one, the triangle's own normal where it does not.
       Deliberately not "the VNML, and the page derives the rest": which normal a point
       gets is a rule, the engine owns the rules, and a page that re-decided it would be
       a second implementation of exactly the question §11a was measured to answer.

       Signed 16-bit per component (i16 in `pack_cell`, 6 bytes a vertex). One byte would
       do for shading but not for the slope overlay: that is drawn from these normals and
       compared against `fMaximumAngle`, so a coarse encoding would put a visible margin
       of error along the very edge the overlay exists to show. */
    let mut normals: Vec<i16> = Vec::with_capacity(if land.is_some() { VERTS * VERTS * 3 } else { 0 });
    if let Some(l) = land {
        for vy in 0..VERTS {
            for vx in 0..VERTS {
                let n = l.normal_at(vx as f32 * VERT_STEP, vy as f32 * VERT_STEP);
                for c in n {
                    normals.push((c.clamp(-1.0, 1.0) * 32767.0).round() as i32 as i16);
                }
            }
        }
    }

    /* The vertex colors, VCLR as the record has them: 65x65 RGB bytes. None for a
       record without the block, which the page draws white, the way the game does.
       Round 11 item 11. */
    let colors: Vec<u8> = land.and_then(|l| l.colours.as_ref()).map(|c| c.to_vec()).unwrap_or_default();

    /* The alpha maps the ground is drawn from, built here rather than page-side.
       The rule they encode — one texel of ring taken from each neighbouring
       landscape — needs to see neighbours the page does not have open, so deriving
       them from this cell's vtex alone drew a border the engine never scattered
       against. See viewcore::coverage.

       Each mask is 18x18 bits, hex: a texel carries exactly one texture, so a layer
       is 41 bytes on the wire whatever the cell. */
    let layers = match (land, &merged) {
        (Some(_), Some(m)) => viewcore::coverage::layers_json(&viewcore::coverage::layers_with(|gi, gj| {
            if (0..16).contains(&gi) && (0..16).contains(&gj) {
                viewcore::mland::texture_of(m, gi as usize, gj as usize).to_string()
            } else {
                viewcore::coverage::texture_at(&w, &m.land, gi, gj).unwrap_or("").to_string()
            }
        })),
        (Some(l), None) => viewcore::coverage::layers_json(&viewcore::coverage::layers(&w, l)),
        _ => "[]".to_string(),
    };

    // References, each already carrying the mesh it draws.
    let reviewed = cell.and_then(|c| {
        let gone = &w.history.exterior((gx, gy)).gone;
        review_items(&w, &c.refs, gone, review.as_deref(), without.unwrap_or(false), &|p| viewcore::review::in_grid(p, gx, gy))
    });
    let refs = match &reviewed {
        Some(items) => refs_tables_marked(&w, items, false),
        None => refs_tables(&w, cell.map(|c| &c.refs), false),
    };
    /* Wraithguard: who edits this cell's ground, in load order, and - in the merged-lands
       preview - where the edits met. */
    let editors: Vec<String> = w
        .land_edits
        .get(&(gx, gy))
        .map(|v| v.iter().filter_map(|(pi, _, _)| w.plugins.get(*pi).cloned()).collect())
        .unwrap_or_default();
    let land_merged = match &merged {
        Some(m) => {
            let list = |v: &Vec<u32>| v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
            format!(
                "{{\"strategy\":\"{}\",\"contested\":[{}],\"major\":[{}],\"mergeable\":{},\"editors\":{}}}",
                land_mode.as_deref().unwrap_or("").to_ascii_lowercase(),
                list(&m.contested),
                list(&m.major),
                m.mergeable,
                string_array(&m.editors.iter().filter_map(|i| w.plugins.get(*i).cloned()).collect::<Vec<_>>())
            )
        }
        None => "null".to_string(),
    };

    /* Who put what here. Every surviving reference remembers which plugin supplied it,
       and the cell remembers what the rest of the order did on the way — replaced,
       deleted, moved in, moved out. The panel that shows this is how you find out that
       the rock you are looking at came from a mod rather than from Morrowind.esm, and
       why the count in front of you is not the count the base game had. */
    let prov = prov_json(&w, cell.map(|c| (&c.refs, &c.prov)));

    let mut out = J::obj();
    out.str("name", cell.map(|c| c.name.as_str()).unwrap_or(""))
        .str("region", cell.map(|c| c.region.as_str()).unwrap_or(""))
        // Signed: half the world has a negative grid coordinate.
        .num("gx", gx as f64)
        .num("gy", gy as f64)
        .bool("hasLand", land.is_some())
        // The preview draws a water surface to judge height limits against. Only
        // exteriors reach this command, and their sea is always at zero.
        .raw("water", "0")
        .raw("ltex", &ltex)
        .raw("layers", &layers)
        .raw("prov", &prov)
        .raw("landEditors", &string_array(&editors))
        .raw("landMerged", &land_merged)
        .bool("reviewed", reviewed.is_some());
    Ok(tauri::ipc::Response::new(pack_cell(&out.done(), &heights, &normals, &colors, &vtex, &refs)))
}

/* Wraithguard: `cell_data`'s and `interior_data`'s answer, `WGCD` (read by CellWire in
   ui/src/17_cell.js): "WGCD" | u32 version 2 | u32 length + the cell's other fields as JSON
   | u32 count + f32 heights | u32 count + i16 normal components | u32 count + u8 colors
   | u32 count + u16 texture indices | u32 length + the references as WGTB
   (`refs_tables_marked`). Little-endian; a count is 0 with no land (every interior) or no
   VCLR. */
fn pack_cell(json: &str, heights: &[f32], normals: &[i16], colors: &[u8], vtex: &[u16], refs: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(32 + json.len() + heights.len() * 4 + normals.len() * 2 + colors.len() + vtex.len() * 2 + refs.len());
    let n = |o: &mut Vec<u8>, v: usize| o.extend_from_slice(&(v as u32).to_le_bytes());
    o.extend_from_slice(b"WGCD");
    n(&mut o, 2);
    n(&mut o, json.len());
    o.extend_from_slice(json.as_bytes());
    n(&mut o, heights.len());
    for h in heights {
        o.extend_from_slice(&h.to_le_bytes());
    }
    n(&mut o, normals.len());
    for c in normals {
        o.extend_from_slice(&c.to_le_bytes());
    }
    n(&mut o, colors.len());
    o.extend_from_slice(colors);
    n(&mut o, vtex.len());
    for v in vtex {
        o.extend_from_slice(&v.to_le_bytes());
    }
    n(&mut o, refs.len());
    o.extend_from_slice(refs);
    o
}

/// A container's references as bytes, every version standing (`usize::MAX`: all the history
/// has). See [`refs_tables_marked`].
fn refs_tables(
    w: &viewcore::world::World,
    src: Option<&HashMap<viewcore::esp::RefNum, viewcore::esp::CellRef>>,
    indoors: bool,
) -> Vec<u8> {
    let items: Vec<viewcore::review::Item> = src
        .map(|m| m.values().map(|r| (r.clone(), viewcore::review::Mark::None, usize::MAX)).collect())
        .unwrap_or_default();
    refs_tables_marked(w, &items, indoors)
}

/// Wraithguard: which references to send, for the cell viewer's "review a mod" - all of
/// them marked for plugin `p` (`review::with`), or the cell with `p` left out
/// (`review::without`). `None` when no plugin is named, or it is not in the load order.
fn review_items(
    w: &viewcore::world::World,
    refs: &HashMap<viewcore::esp::RefNum, viewcore::esp::CellRef>,
    gone: &[viewcore::world::Gone],
    review: Option<&str>,
    without: bool,
    here: &dyn Fn(&[f32; 3]) -> bool,
) -> Option<Vec<viewcore::review::Item>> {
    let name = review.filter(|s| !s.is_empty())?;
    let p = w.plugins.iter().position(|n| n.eq_ignore_ascii_case(name))? as i32;
    Some(if without {
        viewcore::review::without(refs, &w.history, gone, p, here)
    } else {
        viewcore::review::with(refs, &w.history, p)
    })
}

/// A number as the JSON wrote it - `{:.N}` read back - so the page holds the
/// same value it always did (positions to the hundredth, rotations to 1e-5).
fn rounded(v: f32, places: usize) -> f64 {
    format!("{v:.places$}").parse().unwrap_or(0.0)
}

/* Wraithguard: a cell's references, and the models, lights and actors they draw, as bytes
   (`WGTB`, `Tables` in editor.rs), after the grids in `WGCD` (see `pack_cell`). Static
   2026-11-17: JSON only where it is needed; a cell's references were its biggest text after
   the ground. `CellWire.read` (ui/src/17_cell.js) turns them back into the objects the JSON
   was, so every reader of a reference sees what it saw:

   - `refs` `sdddddddissdudddddds`: id, pos x y z, rot x y z, scale, from (the plugin, an
     index into the page's list; -1 none), key (the mask's name for it), mark ("" none),
     moved, flags (1 still, 2 groundcover, 4 a door), the door's pos x y z and rot x y z,
     and the cell it leads to.
   - `hist` `ui`: a reference (its row in `refs`) and one plugin that supplied a version of
     it, in order - only for references more than one plugin touched.
   - `models` `ss`: object id (lower case), mesh.
   - `lights` `siuuuui`: id, radius, color r g b, negative (0/1), the record's flags.
   - `actors` (`write_actors` in editor.rs): id, kind (npc / crea / lev), model, twin, health,
     bits, AI Hello. */
fn refs_tables_marked(w: &viewcore::world::World, items: &[viewcore::review::Item], indoors: bool) -> Vec<u8> {
    struct RefRow {
        id: String,
        pos: [f64; 3],
        rot: [f64; 3],
        scale: f64,
        from: i32,
        key: String,
        mark: &'static str,
        moved: f64,
        flags: u64,
        door: Option<([f64; 3], [f64; 3], String)>,
    }
    let mut refs: Vec<RefRow> = Vec::new();
    let mut hist: Vec<(usize, i32)> = Vec::new();
    // An object's model, light and actor once each, however many references place it.
    let mut models: Vec<(String, String)> = Vec::new();
    let mut lights: Vec<(String, i32, [u8; 3], bool, i32)> = Vec::new();
    let mut actors: Vec<ActorRow> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut named: Vec<&viewcore::review::Item> = items.iter().collect();
    named.sort_by_key(|(r, _, _)| (r.num.content_file, r.num.index));
    for (r, mark, versions) in named {
        let key = r.id.to_ascii_lowercase();
        let actor = w.actors.get(&key);
        /* Wraithguard: a leveled creature list placed in the cell - a spawn point. Sent
           with the actors, drawn with the model `resolve_leveled` gave it (the list's one
           mesh, or the Construction Set's creature marker). */
        let lev = if actor.is_none() {
            w.objects.get(&key).filter(|o| o.kind == Some(viewcore::objects::ObjKind::LeveledCreature))
        } else {
            None
        };
        /* Gardenfell 18hl: a LIGH record with no mesh is in `lights` and not in `models`;
           it has something to shine, so it goes to the page with an empty model and the
           page places its light at the reference (`bareLights`). */
        if actor.is_none() && lev.is_none() && !w.models.contains_key(&key) && !w.lights.contains_key(&key) {
            continue;
        }
        /* `from` is which plugin in the load order supplied this reference — an index into
           the same plugin list the page already holds, not the name, because a cell can
           carry thousands of references. What people want to know is who put *that* rock
           there, and that is asked by right-clicking it.

           `key` is which reference it *is*, spelled the way the mask spells it: the same
           key `statics_paint` and `paint_stroke` use, so the page can ask "how much paint
           is on this rock" about the rock under the pointer. Empty when the load order
           cannot resolve it. */
        let rkey = viewcore::refkey::RefKey::of(r.num, &w.plugins)
            .map(|k| viewcore::refkey::key_text(&k))
            .unwrap_or_default();
        let row = refs.len();
        let mut flags = 0u64;
        let mut moved = 0.0;
        /* Wraithguard: for reviewing a cell's conflicts - how this reference relates to the
           plugin under review, every plugin that supplied a version of it, and how far the
           load order has moved it from where its first plugin put it. */
        let h = w.history.of(&r.num);
        let n = (*versions).min(h.touched.len());
        if n > 1 {
            hist.extend(h.touched[..n].iter().map(|&p| (row, p)));
            let d = viewcore::review::moved_by(r, &h.poses[..n.min(h.poses.len())]);
            if d > 0.5 {
                moved = rounded(d as f32, 1);
            }
        }
        /* Gardenfell 18ez: a reference whose class the game never animates, on a mesh with
           a `.kf` beside it: the page asks for that mesh `still` (see `mesh_payload`). Said
           only when it changes the picture. Actors and leveled spawns have their own rules. */
        if actor.is_none() && lev.is_none() && !w.anim_ids.contains(&key) {
            let has_kf = w
                .models
                .get(&key)
                .map(|m| w.kf_meshes.contains(&m.replace('\\', "/").to_ascii_lowercase()))
                .unwrap_or(false);
            if has_kf {
                flags |= 1;
            }
        }
        // Wraithguard: grass from a `groundcover=` plugin.
        if usize::try_from(r.plugin).map(|p| w.groundcover.contains(&p)).unwrap_or(false) {
            flags |= 2;
        }
        /* Round 17y: where a door leads, when it teleports. The dialogue offers to open the
           cell on the far side; an interior is named, an exterior is wherever the far
           position falls. */
        let door = r.door.as_ref().map(|d| {
            flags |= 4;
            (
                [rounded(d.pos[0], 2), rounded(d.pos[1], 2), rounded(d.pos[2], 2)],
                [rounded(d.rot[0], 5), rounded(d.rot[1], 5), rounded(d.rot[2], 5)],
                d.cell.clone(),
            )
        });
        refs.push(RefRow {
            id: r.id.clone(),
            pos: [rounded(r.pos[0], 2), rounded(r.pos[1], 2), rounded(r.pos[2], 2)],
            rot: [rounded(r.rot[0], 5), rounded(r.rot[1], 5), rounded(r.rot[2], 5)],
            scale: rounded(r.scale, 4),
            from: r.plugin,
            key: rkey,
            mark: if *mark != viewcore::review::Mark::None { mark.name() } else { "" },
            moved,
            flags,
            door,
        });
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Some(o) = lev {
            actors.push(lev_row(&key, &o.model));
            continue;
        }
        if let Some(a) = actor {
            // The page decides per reference whether an actor is a corpse worth drawing,
            // and needs its health and persist flag to do so.
            actors.push(actor_row(w, &key, a));
            continue;
        }
        /* Round 17m: the LIGH records among the objects in this cell, so the preview can
           light the scene with them after dark - only lights that burn where they stand
           (OffDefault: "does not burn while placed in a cell"). Round 17y: and the record's
           flags whole (Fire 0x010, Flicker 0x008, Pulse 0x080...). */
        if let Some(li) = w.lights.get(&key) {
            if !li.off_default() {
                lights.push((key.clone(), li.radius.max(16), li.colour, li.negative(), li.flags));
            }
        }
        let mesh = w.model_in(&key, indoors).unwrap_or_default().to_string();
        models.push((key, mesh));
    }

    let mut tb = Tables::new();
    tb.table("refs", "sdddddddissdudddddds", refs.len());
    for x in &refs {
        tb.s(&x.id);
        for v in x.pos.iter().chain(x.rot.iter()) {
            tb.d(*v);
        }
        tb.d(x.scale).i(x.from as i64).s(&x.key).s(x.mark).d(x.moved).u(x.flags);
        let (dp, dr, dc) = match &x.door {
            Some((p, r, c)) => (*p, *r, c.as_str()),
            None => ([0.0; 3], [0.0; 3], ""),
        };
        for v in dp.iter().chain(dr.iter()) {
            tb.d(*v);
        }
        tb.s(dc);
    }
    tb.table("hist", "ui", hist.len());
    for (row, p) in &hist {
        tb.u(*row as u64).i(*p as i64);
    }
    tb.table("models", "ss", models.len());
    for (k, m) in &models {
        tb.s(k).s(m);
    }
    tb.table("lights", "siuuuui", lights.len());
    for (k, radius, c, negative, flags) in &lights {
        tb.s(k).i(*radius as i64).u(c[0] as u64).u(c[1] as u64).u(c[2] as u64).u(*negative as u64).i(*flags as i64);
    }
    write_actors(&mut tb, &actors);
    tb.done()
}

/// Who put what here.
///
/// Every surviving reference remembers which plugin supplied it, and the container
/// remembers what the rest of the order did on the way — replaced, deleted, moved in,
/// moved out. The panel that shows this is how you find out that the rock you are looking
/// at came from a mod rather than from Morrowind.esm, and why the count in front of you is
/// not the count the base game had.
fn prov_json(
    w: &viewcore::world::World,
    src: Option<(
        &HashMap<viewcore::esp::RefNum, viewcore::esp::CellRef>,
        &viewcore::world::CellProv,
    )>,
) -> String {
    let mut prov = String::from("{\"sources\":{");
    let Some((list, p)) = src else {
        prov.push_str("},\"replaced\":0,\"deleted\":0,\"movedOut\":0,\"movedIn\":0}");
        return prov;
    };
    let mut by: Vec<(usize, usize)> = Vec::new();
    for r in list.values() {
        if r.plugin < 0 {
            continue;
        }
        let pi = r.plugin as usize;
        match by.iter_mut().find(|(i, _)| *i == pi) {
            Some(e) => e.1 += 1,
            None => by.push((pi, 1)),
        }
    }
    by.sort_by_key(|(i, _)| *i);
    for (n, (pi, count)) in by.iter().enumerate() {
        if n > 0 {
            prov.push(',');
        }
        prov.push('"');
        viewcore::json::escape_into(w.plugins.get(*pi).map(|s| s.as_str()).unwrap_or("?"), &mut prov);
        prov.push_str(&format!("\":{}", count));
    }
    prov.push_str(&format!(
        "}},\"replaced\":{},\"deleted\":{},\"movedOut\":{},\"movedIn\":{}}}",
        p.replaced, p.deleted, p.moved_out, p.moved_in
    ));
    prov
}

/// A plugin's file name by index, or an empty string for "nobody said".
///
/// Deliberately not next to the command it serves: a helper written between a
/// `#[tauri::command]` attribute and its function steals the attribute, and what that
/// looks like is `generate_handler!` failing to find a macro for a command that is
/// plainly there. Cost a build.
fn plugin_name(w: &viewcore::world::World, ix: Option<usize>) -> &str {
    ix.and_then(|i| w.plugins.get(i)).map(|s| s.as_str()).unwrap_or("")
}

/// Everything the viewport needs to draw one interior, by name.
///
/// The same answer `cell_data` gives (`WGCD`), minus everything that is about ground: no
/// heights, no normals, no vtex, no coverage layers, and `hasLand` false. A room is its
/// contents.
///
/// Keyed by name because an interior has no grid — which is the whole of the bug this
/// exists to fix. `CellData.loadCell` asked for `(gx, gy)`, an interior has none, so every
/// one of them arrived as (0, 0) and quietly loaded whichever exterior sits at the origin.
/// Matched without regard to case, like every other name in the tool (§50).
#[tauri::command(async)]
fn interior_data(
    name: String,
    review: Option<String>,
    without: Option<bool>,
    state: State,
) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }
        .ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let room = w.room(&name).ok_or_else(|| viewcore::msg!("eng.no_interior", name = name))?;

    let gone = &w.history.interior(&viewcore::world::room_key(&name)).gone;
    let reviewed = review_items(&w, &room.refs, gone, review.as_deref(), without.unwrap_or(false), &|_| true);
    let refs = match &reviewed {
        Some(items) => refs_tables_marked(&w, items, !room.quasi),
        None => refs_tables(&w, Some(&room.refs), !room.quasi),
    };
    let prov = prov_json(&w, Some((&room.refs, &room.prov)));

    let mut out = J::obj();
    out.str("name", &room.name)
        .str("region", "")
        .bool("interior", true)
        // No grid. Sent as nought rather than left out so the page's reader has the same
        // shape to read either way, and `kind` is what says which this is.
        .num("gx", 0.0)
        .num("gy", 0.0)
        .bool("hasLand", false)
        /* An interior's water is wherever its own record put it, and unlike an exterior's
           sea that is not always zero. Absent means the room has none — a cellar with a
           flooded floor and a cellar with a dry one are different rooms. */
        .raw(
            "water",
            &match room.water {
                Some(z) => format!("{z:.2}"),
                None => "null".to_string(),
            },
        )
        /* And who decided, which is two answers: the last plugin to state the flag says
           whether there is water at all, the last to state a level says how high. They
           are often different plugins — a patch that re-saves a cell header commonly
           carries no level at all — and when a room's water is somewhere nobody expects,
           these two names are the whole of the explanation. */
        .str("waterFlagFrom", plugin_name(&w, room.water_flag_from))
        .str("waterLevelFrom", plugin_name(&w, room.water_from))
        /* Round 17y: whether the room behaves like an exterior (CELL flag 0x80) and, for
           the ones that do not, the AMBI it is lit by - ambient, sunlight, fog colour and
           density, colours as bytes. A room with no AMBI at all is sent null and the page
           falls back to a flat grey. */
        .bool("quasi", room.quasi)
        .raw(
            "ambi",
            &match room.ambi {
                Some(a) => format!(
                    "{{\"ambient\":[{},{},{}],\"sunlight\":[{},{},{}],\"fog\":[{},{},{}],\"fogDensity\":{}}}",
                    a.ambient[0], a.ambient[1], a.ambient[2],
                    a.sunlight[0], a.sunlight[1], a.sunlight[2],
                    a.fog[0], a.fog[1], a.fog[2],
                    a.fog_density
                ),
                None => "null".to_string(),
            },
        )
        // No ground: WGCD's grids are empty (`pack_cell`).
        .raw("ltex", "{}")
        .raw("layers", "[]")
        .raw("prov", &prov)
        .raw("landEditors", "[]")
        .raw("landMerged", "null")
        .bool("reviewed", reviewed.is_some());
    Ok(tauri::ipc::Response::new(pack_cell(&out.done(), &[], &[], &[], &[], &refs)))
}


/* ---- profiles ---------------------------------------------------------------
   A profile is the person's setup — obstacle avoidance, placement, export, the
   viewport — and it applies to whatever grass rules are loaded. Several are worth
   keeping: a quick one for looking around, a careful one for the real export, one
   somebody sent you. So each is a file beside the program and a small state file
   records which is in use.

   Everything below is a thin wrapper over `viewcore::profiles`, which does the work
   against a directory it is handed and is therefore testable without a window. */

/// The program's own folder. Two things live here and nowhere else: the state file that
/// records which profile, which rules and *where the store is*, and the caches nobody
/// keeps by hand — the mesh index and the wording file. Round 9 item 5 moved everything
/// else out from under it.
fn app_dir() -> Result<PathBuf, String> {
    viewcore::profiles::dir_beside_exe()
}

/// Where the `.toml` files are kept: profiles, grass rules, the working mesh setups.
/// Beside the program until Settings says otherwise.
fn store() -> Result<PathBuf, String> {
    Ok(viewcore::profiles::store_dir(&app_dir()?))
}

/* ---- where the .toml files live (round 9 item 5) ---------------------------------

   Robin: "I want to be able to set where I store all my .toml files (except the one
   that keeps tabs of which profiles are loaded etc.). Default should be next to the
   .exe… When I choose where to save the files and I already have existing files they
   are moved to that folder by Gardenfell."

   So: the state file stays beside the program and carries one extra line saying where
   everything else is; choosing a folder moves what is there into it; a name already
   taken in the destination is reported and left alone rather than overwritten, because
   the file already there is somebody's work too.
   ---------------------------------------------------------------------------------- */

/// The game's sky, as the connected install describes it — round 13. The weather colour
/// ramp from the load order's ini, Weather Adjuster's per-region presets from the
/// overlay, MGE.ini's fog. See `viewcore::weather`.
#[tauri::command(async)]
fn atmosphere(state: State) -> Result<String, String> {
    let (order, layout, vfs) = {
        let app = state.app();
        (app.order_file.clone(), app.layout.clone(), app.vfs.clone())
    };
    let Some(order) = order else { return Err(viewcore::msg!("eng.no_install_is")) };
    let Some(vfs) = vfs else { return Err(viewcore::msg!("eng.no_install_is")) };
    let layout = layout.unwrap_or_default();
    // All ten weathers (patch 0004); Gardenfell's own reader keeps Clear only.
    Ok(viewcore::weather::atmosphere_json_with(&order, &vfs, &layout, &viewcore::weather::ALL_WEATHERS))
}








/* ---------------------------------------------------------------------------------
   Favourites.

   Starred entries live beside the exe, in the same state file the active profile does,
   because a favourite is about the mods *this* install has rather than about any one
   set of rules. Keyed by kind, not by picker: star a ground texture once and it is at
   the top of every list that offers ground textures.
   --------------------------------------------------------------------------------- */

#[tauri::command(async)]
fn favourites_list() -> Result<String, String> {
    let dir = app_dir()?;
    let mut out = J::obj();
    for (kind, items) in viewcore::profiles::favourites(&dir) {
        let mut arr = String::from("[");
        for (i, it) in items.iter().enumerate() {
            if i > 0 {
                arr.push(',');
            }
            arr.push('"');
            viewcore::json::escape_into(it, &mut arr);
            arr.push('"');
        }
        arr.push(']');
        out.raw(&kind, &arr);
    }
    Ok(out.done())
}

#[tauri::command(async)]
fn favourite_set(kind: String, item: String, on: bool) -> Result<(), String> {
    viewcore::profiles::set_favourite(&app_dir()?, &kind, &item, on)
}

/* ---- language packs (round 18aq, wording-plan.md phase 2) ------------------------

   The engine owns the files: it finds every `gardenfell.<code>.toml` in `languages\`
   under the store (round 18as — beside the program, or in the folder Settings chose),
   parses and checks it against the embedded catalogue, and says what it found. The page
   does the wording. Nothing here touches the state — a pack is a fact about a folder. */

/// Every pack in the language folder, with its coverage and its refused lines.
#[tauri::command(async)]
fn lang_list() -> Result<String, String> {
    let dir = viewcore::profiles::languages_dir()?;
    let packs: Vec<String> = viewcore::lang::list(std::slice::from_ref(&dir)).iter().map(viewcore::lang::listed_json).collect();
    let mut o = viewcore::json::J::obj();
    o.str("dir", &dir.display().to_string()).raw("packs", &format!("[{}]", packs.join(",")));
    Ok(o.done())
}

/// One pack whole, for the page to apply. An error names the file the profile asked
/// for and the folder looked in, which is the start-up warning's wording material.
#[tauri::command(async)]
fn lang_get(code: String) -> Result<String, String> {
    let dir = viewcore::profiles::languages_dir()?;
    let (path, pack) = viewcore::lang::get(std::slice::from_ref(&dir), &code)?;
    Ok(viewcore::lang::pack_json(&path, &pack))
}

/// Starts a pack: `gardenfell.<code>.toml` in the language folder, the template with
/// its header filled in and every value empty for the person to write. Refuses to write
/// over one that is there — that is somebody's translation. Round 18as, "Create new
/// language file".
#[tauri::command(async)]
fn lang_create(code: String, name: String) -> Result<String, String> {
    let dir = viewcore::profiles::languages_dir()?;
    let code = code.trim();
    let text = viewcore::lang::new_pack(code, &name)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let path = dir.join(viewcore::lang::file_for(code));
    if path.exists() {
        return Err(viewcore::msg!("eng.pack_exists", file = path.display().to_string()));
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut o = viewcore::json::J::obj();
    o.str("file", &path.display().to_string()).str("dir", &dir.display().to_string());
    Ok(o.done())
}

/// Refreshes a pack against the current catalogue in place — every value kept, new keys
/// appended empty with their English above, orphans listed at the end — with the old
/// text kept under `backups\languages\` like every other file's last version (round 18v).
/// Answers what changed.
#[tauri::command(async)]
fn lang_update(code: String) -> Result<String, String> {
    let store = store()?;
    let name = viewcore::lang::file_for(&code);
    let rel = std::path::Path::new(viewcore::lang::LANG_DIR).join(&name).to_string_lossy().into_owned();
    let path = store.join(&rel);
    if !path.is_file() {
        return Err(viewcore::msg!("eng.pack_missing_file", file = name));
    }
    let old = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let (text, added, orphans) = viewcore::lang::update(&old).map_err(|e| format!("{}: {}", name, e))?;
    viewcore::profiles::keep_backup(&store, &rel, text.as_bytes());
    // Round 18bf (I6): beside it and renamed over, like every other write in this file.
    write_file_whole(&path, text.as_bytes())?;
    let mut o = viewcore::json::J::obj();
    o.str("file", &path.display().to_string())
        .str("backup", &viewcore::profiles::backup_path(&store, &rel).map(|p| p.display().to_string()).unwrap_or_default())
        .int("added", added as u64)
        .raw("orphans", &viewcore::json::string_array(&orphans));
    Ok(o.done())
}

/// Opens a folder of this program's in the system's file browser: `what` is "store" —
/// where the .toml files are kept, beside the program or wherever Settings put them — or
/// "languages", the packs' folder under it. Made if it is not there yet: "Open languages
/// folder" on a fresh copy is how a person finds where a pack goes. Answers the path.
/// Round 18as.
///
/// `GARDENFELL_NO_OPEN` in the environment opens nothing and answers the same: a test
/// suite has no desktop to put a window on.
#[tauri::command(async)]
fn open_folder(what: String) -> Result<String, String> {
    let dir = match what.as_str() {
        "languages" => viewcore::profiles::languages_dir()?,
        _ => store()?,
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    if std::env::var_os("GARDENFELL_NO_OPEN").is_none() {
        viewcore::profiles::show_folder(&dir)?;
    }
    let mut o = viewcore::json::J::obj();
    o.str("dir", &dir.display().to_string());
    Ok(o.done())
}


/// Write any file the user picked, as text.
///
/// Round 18bf (I6): beside it and renamed over. This is the command the rules file, the
/// profile and the `.ini` export all go out through, and truncating somebody's rules
/// file with half a rules file is the failure `write_file_whole` exists to prevent.
#[tauri::command(async)]
fn write_text(path: String, text: String) -> Result<(), String> {
    write_path_whole(&path, text.as_bytes())
}

/* Security F5 (the second audit, 2026-11-23): where the page may have the program write.
 *
 * The page names the file for every export, every Save as and every output folder, and the
 * commands used to write wherever it said. A page that is only showing a mod's text cannot
 * name anything, but one that ran a script it should not have could have had this program
 * write over a startup script or a settings file of the person's. So a write to a path the
 * page names is taken only when the person chose that path, in a way the page cannot fake:
 *
 *   * a file or folder they picked in a file dialog this run - the dialog is opened from
 *     Rust (`pick::path_pick`) and its answer is recorded here before the page sees it;
 *   * a path the page names that they then allowed in the program's own message box
 *     (`pick::path_allow`), which says the path in full - typed output folders and the
 *     paths a page keeps from an earlier run come this way;
 *   * the settings file the program was started with (`--extra` `settingsFile`), or the
 *     one beside it.
 *
 * A refused write answers `WG_NOT_GRANTED:` and the path, and the page (`Engine.inv`)
 * asks with the message box and tries once more. Paths are compared after `..` and links
 * are taken out, and without case on Windows. Grants last until the program closes. */
pub mod granted {
    use std::path::{Component, Path, PathBuf};
    use std::sync::Mutex;

    /// What a refused write's message starts with; the path follows.
    pub const REFUSED: &str = "WG_NOT_GRANTED:";

    static FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    static FOLDERS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// The path as it is on disk: absolute, `.` and `..` taken out, the links in the part
    /// that exists followed, and in lower case on Windows.
    pub fn norm(p: &Path) -> PathBuf {
        let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
        let mut flat = PathBuf::new();
        for c in abs.components() {
            match c {
                Component::ParentDir => {
                    flat.pop();
                }
                Component::CurDir => {}
                c => flat.push(c),
            }
        }
        // The deepest part that exists, with its links followed; the rest put back on.
        let mut head = flat.clone();
        let mut tail = Vec::new();
        let real = loop {
            if let Ok(r) = std::fs::canonicalize(&head) {
                let mut r = r;
                for t in tail.iter().rev() {
                    r.push(t);
                }
                break r;
            }
            match (head.file_name().map(|n| n.to_os_string()), head.parent().map(Path::to_path_buf)) {
                (Some(n), Some(up)) => {
                    tail.push(n);
                    head = up;
                }
                _ => break flat,
            }
        };
        if cfg!(windows) {
            PathBuf::from(real.to_string_lossy().to_lowercase())
        } else {
            real
        }
    }

    fn add(list: &Mutex<Vec<PathBuf>>, p: &Path) {
        let n = norm(p);
        let mut l = list.lock().unwrap_or_else(|e| e.into_inner());
        if !l.contains(&n) {
            l.push(n);
        }
    }

    /// A file the person chose: it may be written.
    pub fn file(p: &Path) {
        add(&FILES, p)
    }

    /// A folder the person chose: anything in it may be written.
    pub fn folder(p: &Path) {
        add(&FOLDERS, p)
    }

    /// Whether the person chose this path, or a folder it is in. A picture beside a file
    /// they chose, with its name (`NAME.png`), is theirs too: the cell report's view.
    pub fn may_write(p: &Path) -> bool {
        let n = norm(p);
        let files = FILES.lock().unwrap_or_else(|e| e.into_inner());
        let png = n.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
        let companion = |f: &PathBuf| png && f.parent() == n.parent() && f.file_stem().is_some() && f.file_stem() == n.file_stem();
        files.iter().any(|f| *f == n || companion(f)) || FOLDERS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|f| n.starts_with(f))
    }

    /// The path, when it may be written; otherwise the refusal the page asks about.
    pub fn check(p: &str) -> Result<PathBuf, String> {
        let p = p.trim();
        if p.is_empty() {
            return Err("no path was given".into());
        }
        let path = PathBuf::from(p);
        if may_write(&path) {
            Ok(path)
        } else {
            Err(format!("{REFUSED}{p}"))
        }
    }

    /// A plugin written over in place (Remove / Rename Master, a bulk take, the Editor's
    /// in-place build): one that is there, with a plugin's name. The writers read it as a
    /// plugin before they write it, so nothing else gets this far; any other path the page
    /// names is one the person has to have chosen.
    pub fn plugin_in_place(p: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(p.trim());
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if path.is_file() && matches!(ext.as_str(), "esp" | "esm" | "omwaddon" | "omwgame") {
            Ok(path)
        } else {
            check(p)
        }
    }

    /// What a refused run's message starts with: a program the page names (an external
    /// tes3cmd, typed or kept in the settings file) is run only when the person picked it
    /// in a dialog this run or allowed it in the message box.
    pub const REFUSED_RUN: &str = "WG_NOT_GRANTED_RUN:";

    /// The program, when the person chose it; otherwise the refusal the page asks about.
    pub fn check_run(p: &str) -> Result<PathBuf, String> {
        check(p).map_err(|e| e.replacen(REFUSED, REFUSED_RUN, 1))
    }

    /// For the tests: forget every grant.
    #[cfg(test)]
    pub fn clear() {
        FILES.lock().unwrap_or_else(|e| e.into_inner()).clear();
        FOLDERS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}


/// Round 18r: where the mouse pointer is, and whether it can be seen.
///
/// The one command in this file that touches a `Window`, and the note at the top says
/// none of them does - so here is why this one has to, and why it is still safe.
///
/// **Why.** Turning the head in the WASD scheme needs a pointer with no edge to run into.
/// The web answer is the pointer lock, and every engine that draws this window puts a
/// bubble on the screen when a page takes it - "If you want to see the cursor, press ESC",
/// naming `tauri.localhost` in the built app. It is the browser's own chrome; no page can
/// suppress it. So the page does what a native game does instead: hide the pointer, and
/// put it back in the middle of the viewport whenever it strays. Nothing is locked, the
/// browser has nothing to announce, and the turn still never runs out of desk.
///
/// **Why it is safe off the main thread.** The rule at the top is about *work*: a command
/// without `async` runs on the thread the window is drawn on, and work there freezes the
/// application. This does no work - it hands two numbers to the runtime, which dispatches
/// the window call to the main thread itself. It is `async` like all the rest.
///
/// `x`/`y` are logical pixels in the window's own coordinates, which is what
/// `set_cursor_position` takes; `show` is left out entirely on the calls that only move
/// the pointer, because a visibility flag toggled fifteen times a second is a good way to
/// find out that some platform counts them.
///
/// **Hide before moving, move before showing.** Robin: "The cursor is visible when it
/// teleports to the center and then goes invisible. I want it to go invisible first, then
/// teleport. Same when revealed, but in reverse: Teleport to old position first while
/// invisible, then reveal." Which is one rule rather than two: the pointer is never on
/// screen while it is being moved, whichever direction the call is going.
#[tauri::command(async)]
fn cursor_park(
    show: Option<bool>,
    x: Option<f64>,
    y: Option<f64>,
    window: tauri::Window,
) -> Result<(), String> {
    let hiding = show == Some(false);
    if hiding {
        window.set_cursor_visible(false).map_err(|e| e.to_string())?;
    }
    if let (Some(x), Some(y)) = (x, y) {
        if x.is_finite() && y.is_finite() {
            window
                .set_cursor_position(tauri::LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(v) = show {
        if !hiding {
            window.set_cursor_visible(v).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/* ---- working outside the lock ------------------------------------------------------

   Every command runs off the main thread now (§43), which stopped the window freezing —
   but they all still queued behind one mutex, so nine cells were nine scatters one after
   another. The heavy ones read the world and change nothing in it, so they take a handle
   under the lock, let the lock go, and do the work outside it. Nine cells then scatter on
   nine threads.

   The `Arc`s are what make that cheap: a handle is a pointer, where cloning the world
   would be hundreds of megabytes. The rules and the profile *are* copied, and that is
   deliberate — they are small, a command must not read them half-changed, and
   `preview::enabled` was already copying the selectors it keeps.
   ------------------------------------------------------------------------------------ */

/// The overlay on its own, for the commands that only read files.
fn vfs_of(state: &State) -> Result<std::sync::Arc<Vfs>, String> {
    let app = state.app();
    app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))
}

/// Writes a file without destroying the one already there — round 18bf (I6).
///
/// `fs::write` truncates first and writes after, so a run that fails part way through
/// leaves a **truncated** file where a working one was. For a profile or a rules file
/// that is survivable — those have a ten-deep backup floor under them — but an export
/// has none, and overwriting a good `Grass.esp` with half a `Grass.esp` is the one
/// failure in this program that loses something the person cannot get back by pressing
/// the button again (the run that would rebuild it is the run that just failed).
///
/// So: written beside it under a temporary name, then renamed over. `fs::rename` replaces
/// an existing file on Windows and on Unix, and it is atomic on both — the old file
/// stands whole until the new one is whole. A failure at any point leaves the original
/// exactly as it was and takes the temporary away with it.
fn write_file_whole(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.gfnew"));
    let cleanup = |e: String| {
        let _ = std::fs::remove_file(&tmp);
        e
    };
    std::fs::write(&tmp, bytes).map_err(|e| cleanup(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| cleanup(format!("{}: {e}", path.display())))?;
    Ok(())
}

/// [`write_file_whole`] for a path the caller holds as a string.
/// A path the page names, written whole - when the person chose it (`granted`).
fn write_path_whole(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_file_whole(&granted::check(path)?, bytes)
}




/// Read a file through the overlay, for the meshes and textures the viewport draws.
/// Reads one asset out of the overlay.
///
/// Returned as an ipc Response so the bytes travel the binary channel. A plain
/// `Vec<u8>` would be serialised as a JSON array of numbers — roughly six times
/// the size, and parsed as text at the other end.
#[tauri::command(async)]
fn read_asset(path: String, state: State) -> Result<tauri::ipc::Response, String> {
    // Held only long enough to take the handle: a cell load reads a hundred of these
    // and none of them should queue behind a scatter. See `snapshot`.
    let v = vfs_of(&state)?;
    /* Textures go through the family resolver — the file behind `Tx_AC_rock_01.tga` is
       usually `tx_ac_rock_01.dds` in whichever mod overrode it, and asking for the
       stated spelling across the whole overlay finds vanilla's copy at the bottom of
       the order instead. The page used to walk the extensions itself, one call each,
       which could only ever be extension-major and so could only ever get this wrong. */
    let n = viewcore::vfs::norm(&path);
    let is_tex = viewcore::vfs::Vfs::TEX_EXTS.iter().any(|e| n.ends_with(e));
    let p = if is_tex {
        v.resolve_texture(&n).or_else(|| v.resolve(&n))
    } else {
        v.resolve_mesh(&path).or_else(|| v.resolve(&path))
    }
    .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
    let bytes = v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// One texture, ready for the GPU.
///
/// The page used to read the file over `read_asset` and decode it in JavaScript, on the
/// thread the window is drawn on: **11.7 ms for a 1024² DXT1**, and then uploaded
/// uncompressed at eight times the file's size in video memory. A busy cell touches
/// hundreds of them.
///
/// Now the blocks are handed over as they are whenever the page can upload them, and
/// anything else is decoded here — off that thread, on a command that already runs in
/// parallel with the others (§43, §46). `want_bc` is the page saying whether it has
/// `WEBGL_compressed_texture_s3tc`; without it, compressed files come back as RGBA.
///
/// ```text
/// "GDN2" | u32 headLen | JSON header | levels, largest first | thumbnail
/// ```
///
/// The header names the kind (`bc1`/`bc2`/`bc3`/`rgba`/`file`), the size, each level's
/// size in pixels and bytes, and the thumbnail's size. `file` means the payload is the
/// file's own bytes for `createImageBitmap` — PNG, JPEG and BMP are the browser's job,
/// and it does them off the main thread anyway.
/// The bytes `texture_data` answers with, for one texture. `max_size` (round 16b) drops
/// the mip levels above it - a 4096² replacer arrives as its own 2048² mip, which is
/// the file's own pixels and, at the viewport's size, all of them that can be seen;
/// the two levels dropped were four fifths of the bytes. Textures without a mip chain
/// are sent whole: there is nothing smaller to send.
/// Wraithguard: a texture a NIF carries inside itself (`__nifpx/<record>@<mesh>`): the
/// mesh read again (as `mesh_payload` found it) and the record's pixels decoded.
fn embedded_payload(v: &Vfs, path: &str) -> Option<Result<(viewcore::img::Texture, String), String>> {
    let (ix, owner) = viewcore::nif::embedded_name(path)?;
    let disk = std::path::Path::new(owner.strip_prefix("file:").unwrap_or(owner));
    let bytes = if owner.starts_with("file:") || (disk.is_absolute() && disk.is_file()) {
        std::fs::read(disk).ok()
    } else {
        v.resolve_mesh(owner).or_else(|| v.resolve(owner)).and_then(|p| v.load(&p))
    };
    let Some(bytes) = bytes else {
        return Some(Err(viewcore::msg!("eng.not_in_load_order", path = owner)));
    };
    Some(match viewcore::nif::embedded_texture(&bytes, ix) {
        Some((w, h, px)) => Ok((viewcore::img::from_rgba(w, h, px), format!("inside {owner}"))),
        None => Err(format!("{owner}: its texture record {ix} is not pixel data this viewer reads")),
    })
}

fn texture_payload(v: &Vfs, path: &str, bc: bool, extra: viewcore::img::Extra, max_size: u32) -> Result<Vec<u8>, String> {
    /* Wraithguard: a distant object's texture (`lod_payload` names them `__far/...`): the same
       file at 128 pixels at most, under its own name so the page keeps it apart from the
       full-size copy a near object of the same texture uses. */
    let (path, max_size) = match path.strip_prefix(FAR_TEX).or_else(|| path.strip_prefix("__far\\")) {
        Some(p) => (p, if max_size == 0 { FAR_TEX_SIZE } else { max_size.min(FAR_TEX_SIZE) }),
        None => (path, max_size),
    };
    let n = viewcore::vfs::norm(path);
    /* Wraithguard: what this texture cost and what was done to it, for the Report's
       bundle lines (slow cell loads, 2026-10): the file's own size before the detail cut,
       the bytes read from disk, which cut applied ("dds" a partial read, "levels" mips
       dropped after a whole read, "halved" straight RGBA shrunk here), its format by
       extension, and the engine's time on it. */
    let clock = std::time::Instant::now();
    let mut cuts: Vec<&str> = Vec::new();
    let mut orig: Option<(u32, u32)> = None;
    let mut ext = String::new();
    let (mut t, from, bytes) = if let Some(got) = embedded_payload(v, path) {
        let (t, from) = got?;
        ext.push_str("nif");
        (t, from, Vec::new())
    } else {
        let p = v
            .resolve_texture(&n)
            .or_else(|| v.resolve(&n))
            .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
        ext = match &p {
            viewcore::vfs::Loc::Disk(d) => d.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default(),
            viewcore::vfs::Loc::Packed { key, .. } => key.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default(),
        };
        // Wraithguard: a DDS is read only as far as the Texture detail keeps (its header,
        // then from the first level kept to the end; `img::dds_trim`): a 4096 texture at
        // 512 is a third of a megabyte read, not twenty.
        let is_dds = ext == "dds";
        let mut trimmed = None;
        if is_dds && max_size > 0 {
            if let Some(head) = v.load_range(&p, 0, viewcore::img::DDS_HEAD) {
                if let Some((at, mut cut)) = viewcore::img::dds_trim(&head, max_size) {
                    if let Some(rest) = v.load_range(&p, at, usize::MAX) {
                        // The header's own size, before `dds_trim` rewrote it for the level kept.
                        orig = Some((u32::from_le_bytes([head[16], head[17], head[18], head[19]]),
                                     u32::from_le_bytes([head[12], head[13], head[14], head[15]])));
                        cut.extend(rest);
                        trimmed = Some(cut);
                    }
                }
            }
        }
        let bytes = match trimmed {
            Some(b) => {
                cuts.push("dds");
                b
            }
            None => v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?,
        };
        // Where it came from, for the report: a decode error names the file that failed.
        let from = p.ident(v);
        let t = viewcore::img::read_with(&bytes, &n, bc, extra).map_err(|e| format!("{e} — {from}"))?;
        (t, from, bytes)
    };
    let (src_w, src_h) = orig.unwrap_or((t.w, t.h));
    if max_size > 0 && t.levels.len() > 1 {
        let had = t.levels.len();
        while t.levels.len() > 1 && t.levels[0].0.max(t.levels[0].1) > max_size {
            t.levels.remove(0);
        }
        if t.levels.len() < had {
            cuts.push("levels");
        }
        t.w = t.levels[0].0;
        t.h = t.levels[0].1;
    }
    // Wraithguard: one level of straight RGBA (no mips in the file, or a format decoded
    // here) has none to drop, so it is halved down to the detail instead of sent whole.
    if max_size > 0 && t.levels.len() == 1 && matches!(t.kind, viewcore::img::Kind::Rgba) && t.w.max(t.h) > max_size {
        let (w, h, px) = std::mem::take(&mut t.levels[0]);
        let (w, h, px) = viewcore::img::shrink_rgba(w, h, px, max_size);
        t.levels[0] = (w, h, px);
        t.w = w;
        t.h = h;
        cuts.push("halved");
    }

    let mut head = J::obj();
    head.str("kind", t.kind.name()).int("w", t.w as u64).int("h", t.h as u64).int("depth", t.depth as u64).str("from", &from);
    /* Round 17n: what the alpha channel is for — opaque, a stencil, or a real gradient.
       A NiAlphaProperty says how to draw a shape but not what its texture holds, and the
       two want opposite treatment: cutting a gradient at a threshold turned Vivec's
       waterfalls into hard stencils. Read at full resolution in `img::read`. */
    head.str("alpha", t.alpha.name());
    head.str("ext", &ext)
        .int("srcW", src_w as u64)
        .int("srcH", src_h as u64)
        .int("read", bytes.len() as u64)
        .str("cut", &cuts.join("+"))
        .int("us", clock.elapsed().as_micros().min(u64::MAX as u128) as u64);
    let levels: Vec<String> = t
        .levels
        .iter()
        .map(|(lw, lh, b)| {
            let mut o = J::obj();
            o.int("w", *lw as u64).int("h", *lh as u64).int("bytes", b.len() as u64);
            o.done()
        })
        .collect();
    head.raw("levels", &viewcore::json::raw_array(&levels));
    if let Some((tw, th, tb)) = &t.thumb {
        let mut o = J::obj();
        o.int("w", *tw as u64).int("h", *th as u64).int("bytes", tb.len() as u64);
        head.raw("thumb", &o.done());
    }
    let head = head.done();

    let mut out = Vec::with_capacity(8 + head.len() + bytes.len());
    out.extend_from_slice(b"GDN2");
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(head.as_bytes());
    for (_, _, b) in &t.levels {
        out.extend_from_slice(b);
    }
    if let Some((_, _, tb)) = &t.thumb {
        out.extend_from_slice(tb);
    }
    Ok(out)
}

#[tauri::command(async)]
fn texture_data(
    path: String,
    bc: Option<bool>,
    max_size: Option<u32>,
    bcx: Option<Vec<String>>,
    state: State,
) -> Result<tauri::ipc::Response, String> {
    let v = vfs_of(&state)?;
    Ok(tauri::ipc::Response::new(texture_payload(&v, &path, bc.unwrap_or(false), extra_of(&bcx), max_size.unwrap_or(0))?))
}

/// Wraithguard: the block formats beyond S3TC the page said it can take (`bcx`).
fn extra_of(bcx: &Option<Vec<String>>) -> viewcore::img::Extra {
    let has = |n: &str| bcx.as_ref().is_some_and(|v| v.iter().any(|x| x == n));
    viewcore::img::Extra { rgtc: has("rgtc"), bptc: has("bptc") }
}

/// One mesh, parsed and ready to draw.
///
/// The page used to read the NIF bytes over this same boundary and parse them again
/// in JavaScript — a second reader of a format with no record lengths, which is
/// where a season of avoidance bugs came from. It now draws what the engine read.
/// The bytes `mesh_data` answers with, and the textures the mesh names - the bundle
/// (below) fetches those in the same call.
/// `still`: the reference's class never animates (Gardenfell 18ez; see `refs_tables_marked`),
/// so the `.kf` beside the mesh is not played. False draws the file as it animates - the
/// mesh viewer, and every request from a page that does not send the flag.
fn mesh_payload(v: &Vfs, w: Option<&World>, path: &str, still: bool) -> Result<(Vec<u8>, Vec<String>), String> {
    /* Wraithguard: an NPC, assembled from its skeleton, body parts and what it wears
       (`viewcore::npc`). The page asks for it by `__npc/<id>`. */
    if let Some((id, opts)) = viewcore::npc::npc_request(path) {
        let w = w.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
        let a = viewcore::npc::assemble(&w.npc, v, id, opts)
            .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
        /* A carried torch's light rides where a lamp's does: its point as the attach
           point, its colour and radius as the light under it (the page reads it for an
           NPC as the LIGH record's light, 18_cellpreview.js). */
        let (attach, light) = match a.light {
            Some(l) => (Some(l.at), Some((l.colour, l.radius))),
            None => (None, None),
        };
        return Ok(payload_of_parts(a.parts, None, attach, a.systems, light));
    }
    // Wraithguard: a distant object's model, simplified (`viewcore::lod`): `__lod/<level>/<mode>/<model>`.
    if let Some(rest) = path.strip_prefix("__lod/").or_else(|| path.strip_prefix("__lod\\")) {
        return lod_payload(v, rest);
    }
    // Wraithguard: a Construction Set marker the viewer carries itself (`viewcore::markers`).
    if let Some(bytes) = viewcore::markers::get(path).filter(|_| path.to_ascii_lowercase().starts_with(viewcore::markers::PREFIX)) {
        return Ok(mesh_payload_of(bytes, None, None, None));
    }
    /* The corpse stand-in has no file. An NPC is assembled from body parts at runtime,
       so there is nothing on disk to draw or to keep grass out of, and the engine
       builds a body-shaped slab instead. The viewport asks for it by the same name the
       obstacle walk uses, and gets the same geometry — which is what makes "what you
       see is what blocks grass" true rather than nearly true. */
    if path == viewcore::world::CORPSE_SLAB {
        let g = viewcore::world::corpse_slab();
        let parts = viewcore::nif::draw_from_geom(&g, "npc-corpse");
        return Ok((viewcore::preview::mesh_bytes(&parts, Some(&g)), Vec::new()));
    }
    /* Wraithguard: `file:<path>` (or an absolute path) is one mod's own copy of a mesh (the mesh viewer's
       compare), read straight from disk; its textures still resolve through the setup,
       as the game would draw it. Its `.kf` is the file beside it, when there is one. */
    let disk = std::path::Path::new(path.strip_prefix("file:").unwrap_or(path));
    if path.starts_with("file:") || (disk.is_absolute() && disk.is_file()) {
        let bytes = std::fs::read(disk).map_err(|e| format!("{}: {}", path, e))?;
        let kf = std::fs::read(disk.with_extension("kf")).ok().and_then(|b| viewcore::nif::parse_kf(&b));
        return Ok(mesh_payload_of(&bytes, kf.as_ref(), None, Some(path)));
    }
    /* Wraithguard: the groups a script plays on the reference (`viewcore::anim`), asked for
       as `__grp/<spec>/<model>`: the model's `.kf` laid out with each bone group's group in
       a band of its own, each link timed to start on its group's first frame. A spec the
       file has nothing for draws the model as it always is. */
    if let Some((spec, model)) = viewcore::anim::group_request(path) {
        let p = v
            .resolve_mesh(model)
            .or_else(|| v.resolve(model))
            .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = model))?;
        let bytes = v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = model))?;
        let kf = kf_beside(v, model);
        let laid = kf.as_ref().zip(viewcore::anim::parse_spec(spec)).and_then(|(k, s)| viewcore::anim::layered(k, &s));
        let Some((lk, bands)) = laid else { return Ok(mesh_payload_of(&bytes, kf.as_ref(), None, Some(model))) };
        let Some((mut r, g)) = viewcore::nif::read_for_draw_and_geom(&bytes, Some(&lk)) else {
            return Ok(mesh_payload_of(&bytes, kf.as_ref(), None, Some(model)));
        };
        viewcore::nif::own_embedded(&mut r, model);
        viewcore::anim::retime(&mut r.parts, &bands);
        return Ok(payload_of_parts(r.parts, g, r.attach, r.systems, r.light));
    }
    let p = v
        .resolve_mesh(path)
        .or_else(|| v.resolve(path))
        .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
    let bytes = v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?;
    /* Round 18ag: the animation beside the mesh. Morrowind keeps an actor's keys in a
       `.kf` of the same name - `r\xsiltstrider.nif` walks by `r\xsiltstrider.kf` - and
       so do the banners and flags under `f\`. Looked up the way the mesh was, so a
       replacer's mesh takes the game's own `.kf` from the archive when it ships none of
       its own, which is what the game does too. A mesh with none is the still thing it
       always was. */
    /* Gardenfell 18ey/18ez: **and only when the game would ever open it.** A `.kf` is
       played through the animation system, which belongs to ACTI, CONT, DOOR, LIGH
       (OpenMW's `useAnim()`) and to actors; a STAT never gets one, even when its `model`
       names an `x` file with a `.kf` beside it. `still` is the reference's answer. */
    let kf = if still { None } else { kf_beside(v, path) };
    // Round 17m: and where a light hangs on it, for the LIGH records that name it.
    // Round 18cr: the parts, the particle systems and the light in one read of the file.
    Ok(mesh_payload_of(&bytes, kf.as_ref(), None, Some(path)))

}


/// Wraithguard: a model's drawable parts as the file has them (no `.kf`: a distant copy
/// stands still), or None when it cannot be read.
fn model_parts(v: &Vfs, model: &str) -> Option<Vec<viewcore::nif::DrawPart>> {
    let p = v.resolve_mesh(model).or_else(|| v.resolve(model))?;
    let bytes = v.load(&p)?;
    let (mut r, _) = viewcore::nif::read_for_draw_and_geom(&bytes, None)?;
    viewcore::nif::own_embedded(&mut r, model);
    Some(r.parts)
}

/// Wraithguard: the payload of a distant object's model (`viewcore::lod`): `rest` is
/// `<tier>/<mode>/<model>`, the tier the model's own size puts it in. The parts simplified, and nothing a far copy needs - no
/// collision, no particles, no light. The texture-aware mode reads each part's base
/// texture's small image, from the head of the file only when it is a DDS.
fn lod_payload(v: &Vfs, rest: &str) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut it = rest.splitn(3, ['/', '\\']);
    let tier: u8 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mode = viewcore::lod::Mode::parse(it.next().unwrap_or("plain"));
    let model = it.next().filter(|m| !m.is_empty()).ok_or_else(|| format!("__lod/{rest}: no model"))?;
    let mut parts = model_parts(v, model).ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = model))?;
    let swatch = |tex: &str| -> Option<viewcore::lod::Swatch> {
        let n = viewcore::vfs::norm(tex);
        let p = v.resolve_texture(&n).or_else(|| v.resolve(&n))?;
        let bytes = v
            .load_range(&p, 0, viewcore::img::DDS_HEAD)
            .and_then(|head| viewcore::img::dds_trim(&head, 128))
            .and_then(|(at, mut head)| {
                head.extend(v.load_range(&p, at, usize::MAX)?);
                Some(head)
            })
            .or_else(|| v.load(&p))?;
        viewcore::img::read_with(&bytes, &n, true, viewcore::img::Extra::default()).ok()?.thumb
    };
    viewcore::lod::simplify_parts(&mut parts, tier.min(2), mode, &swatch);
    // Its textures small, under their own names (`texture_payload`).
    let far = |t: &mut String| {
        if !t.is_empty() && !t.starts_with(viewcore::nif::EMBEDDED) && !t.starts_with(FAR_TEX) {
            *t = format!("{FAR_TEX}{t}");
        }
    };
    for p in parts.iter_mut() {
        for t in [&mut p.tex, &mut p.detail_tex, &mut p.glow_tex, &mut p.gloss_tex, &mut p.dark_tex, &mut p.decal_tex, &mut p.env_tex, &mut p.bump_tex] {
            far(t);
        }
        if let Some(f) = p.flip.as_mut() {
            for t in f.frames.iter_mut() {
                far(t);
            }
        }
    }
    Ok(payload_of_parts(parts, None, None, Vec::new(), None))
}

/// Wraithguard: the prefix a distant object's texture is named by, and the most it is.
const FAR_TEX: &str = "__far/";
const FAR_TEX_SIZE: u32 = 128;

/// Wraithguard: the most distant objects one scene draws - instances and models - so a long
/// draw distance on a big install cannot run the page out of memory (Static, 2026-10-13:
/// the viewer closed itself). What is kept is what looks biggest from the centre.
const DISTANT_MAX_REFS: usize = 30_000;
const DISTANT_MAX_MODELS: usize = 1_500;

/// Wraithguard: how far a model reaches from its origin (`viewcore::lod::radius`), read
/// once per model and world and kept.
fn model_radius(v: &Vfs, rev: u64, model: &str) -> f32 {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, f32>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let key = format!("{rev}|{}", model.to_ascii_lowercase());
    if let Some(r) = cache.lock().ok().and_then(|c| c.get(&key).copied()) {
        return r;
    }
    let r = model_parts(v, model).map(|p| viewcore::lod::radius(&p)).unwrap_or(0.0);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, r);
    }
    r
}

/// Wraithguard: distant objects (OpenMW's object paging, for the viewer) - the references
/// placed in every exterior within `radius` cells of (`x`, `y`) and outside the `inner`
/// rings, grouped by model, as bytes (`WGTB`, Static 2026-11-17: JSON only where it is
/// needed; up to 30,000 references of seven numbers each): `meta` `uuu` (references kept,
/// beyond their band, over the budget), `models` `sdu` (mesh, radius, tier) and `refs`
/// `uddddddd` (its row in `models`, x y z, rx ry rz, scale - to 0.1, 1e-5 and 1e-4, as the
/// JSON had them). Read by 58_wg_distant_objects.js. Budgeted as MGE XE budgets its
/// distant statics (`viewcore::lod`): each reference is near, far or very far by its radius
/// times its scale, and drawn only within its tier's band of the cells past the rings;
/// `t` is the model's own tier, which its one simplified copy is made for. Left out too:
/// what is deleted, actors (they move), grass from `groundcover=` plugins (drawn only
/// near), and markers (not drawn in the game).
#[tauri::command(async)]
fn distant_objects(x: i32, y: i32, inner: i32, radius: i32, state: State) -> Result<tauri::ipc::Response, String> {
    let (w, v, rev) = {
        let app = state.app();
        (
            app.world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?,
            app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?,
            app.world_rev,
        )
    };
    let (radius, inner) = (radius.clamp(0, 64), inner.max(0));
    let span = (radius - inner).max(1);
    type Placed = ([f32; 3], [f32; 3], f32, i32);
    let mut by_model: HashMap<String, Vec<Placed>> = HashMap::new();
    for gy in y - radius..=y + radius {
        for gx in x - radius..=x + radius {
            let d = (gx - x).abs().max((gy - y).abs());
            if d <= inner {
                continue;
            }
            let Some(c) = w.cells.get(&(gx, gy)) else { continue };
            for r in c.refs.values() {
                if r.deleted || usize::try_from(r.plugin).map(|p| w.groundcover.contains(&p)).unwrap_or(false) {
                    continue;
                }
                let key = r.id.to_ascii_lowercase();
                if w.actors.contains_key(&key) {
                    continue;
                }
                let Some(m) = w.models.get(&key) else { continue };
                if m.is_empty() || m.to_ascii_lowercase().contains("marker") {
                    continue;
                }
                let low = m.to_ascii_lowercase();
                let m = if low.starts_with("meshes\\") || low.starts_with("meshes/") { m.clone() } else { format!("meshes\\{m}") };
                by_model.entry(m).or_default().push((r.pos, r.rot, r.scale, d - inner));
            }
        }
    }
    let models: Vec<String> = by_model.keys().cloned().collect();
    let radii = viewcore::pool::par_map(&models, |_, m| model_radius(&v, rev, m));
    let (cx, cy) = (x as f32 * 8192.0 + 4096.0, y as f32 * 8192.0 + 4096.0);
    // Every reference within its tier's band, with how big it looks from the centre.
    let mut beyond = 0usize;
    let mut seen: Vec<(f32, usize, usize)> = Vec::new(); // (apparent size, model, ref)
    for (mi, (m, rad)) in models.iter().zip(radii.iter()).enumerate() {
        for (ri, (pos, _, scale, d)) in by_model[m].iter().enumerate() {
            let size = rad * scale;
            if *rad <= 0.0 || *d > viewcore::lod::band_end(viewcore::lod::tier_of(size), span) {
                beyond += 1;
                continue;
            }
            let dist = ((pos[0] - cx).powi(2) + (pos[1] - cy).powi(2)).sqrt().max(1.0);
            seen.push((size / dist, mi, ri));
        }
    }
    // Within the budget, biggest first: the instances, and the models they need.
    seen.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut chosen: Vec<Vec<usize>> = vec![Vec::new(); models.len()];
    let (mut kept, mut used_models, mut over) = (0usize, 0usize, 0usize);
    for (_, mi, ri) in seen {
        if kept >= DISTANT_MAX_REFS || (chosen[mi].is_empty() && used_models >= DISTANT_MAX_MODELS) {
            over += 1;
            continue;
        }
        if chosen[mi].is_empty() {
            used_models += 1;
        }
        chosen[mi].push(ri);
        kept += 1;
    }
    let rows: Vec<usize> = (0..models.len()).filter(|&mi| !chosen[mi].is_empty()).collect();
    let mut tb = Tables::new();
    tb.table("meta", "uuu", 1);
    tb.u(kept as u64).u(beyond as u64).u(over as u64);
    tb.table("models", "sdu", rows.len());
    for &mi in &rows {
        tb.s(&models[mi]).d(rounded(radii[mi], 1)).u(viewcore::lod::tier_of(radii[mi]) as u64);
    }
    tb.table("refs", "uddddddd", kept);
    for (row, &mi) in rows.iter().enumerate() {
        let list = &by_model[&models[mi]];
        for &ri in &chosen[mi] {
            let (pos, rot, scale, _) = &list[ri];
            tb.u(row as u64);
            for v in pos {
                tb.d(rounded(*v, 1));
            }
            for v in rot {
                tb.d(rounded(*v, 5));
            }
            tb.d(rounded(*scale, 4));
        }
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// A mesh's payload from its bytes: the drawable parts, the collision geometry (`geom`
/// when the world has it cached, else parsed here), attach point, particles and light,
/// and every texture it names.
fn mesh_payload_of(
    bytes: &[u8],
    kf: Option<&viewcore::nif::KfSequence>,
    geom: Option<viewcore::nif::MeshGeom>,
    owner: Option<&str>,
) -> (Vec<u8>, Vec<String>) {
    // One read of the file for both the drawing and the collision (the world no longer
    // reads every mesh's collision at startup - that was for grass generation).
    let (parts, attach, systems, light, read_geom) = match viewcore::nif::read_for_draw_and_geom(bytes, kf) {
        Some((mut r, g)) => {
            /* Wraithguard: a texture the mesh carries inside itself is named after the mesh,
               so the page fetches it as it fetches the rest (`texture_payload`). */
            if let Some(o) = owner {
                viewcore::nif::own_embedded(&mut r, o);
            }
            (r.parts, r.attach, r.systems, r.light, g)
        }
        None => (Vec::new(), None, Vec::new(), None, None),
    };
    payload_of_parts(parts, geom.or(read_geom), attach, systems, light)
}

/// The payload from what a mesh read gave: the parts, collision, attach point, particle
/// systems and light, and every texture they name.
fn payload_of_parts(
    parts: Vec<viewcore::nif::DrawPart>,
    geom: Option<viewcore::nif::MeshGeom>,
    attach: Option<[f32; 3]>,
    systems: Vec<viewcore::nif::ParticleSystem>,
    light: Option<([f32; 3], f32)>,
) -> (Vec<u8>, Vec<String>) {
    let mut texs: Vec<String> = Vec::new();
    for part in &parts {
        if !part.tex.is_empty() && !texs.iter().any(|t| t == &part.tex) {
            texs.push(part.tex.clone());
        }
        // The detail map counts as one of the mesh's textures (round 17h): the bundle
        // sends it with the rest, so a scum sheet arrives coloured rather than white.
        if !part.detail_tex.is_empty() && !texs.iter().any(|t| t == &part.detail_tex) {
            texs.push(part.detail_tex.clone());
        }
        // Round 17y: and the dark map, the same way.
        if !part.dark_tex.is_empty() && !texs.iter().any(|t| t == &part.dark_tex) {
            texs.push(part.dark_tex.clone());
        }
        /* Round 18dl: and the glow map, the decal, the environment map and its bump map -
           the page fetched the first two on its own, a trip per file after the bundle
           had answered; now everything a mesh names comes in the one answer. */
        for t in [&part.glow_tex, &part.decal_tex, &part.env_tex, &part.bump_tex, &part.gloss_tex] {
            if !t.is_empty() && !texs.iter().any(|x| x == t) {
                texs.push(t.clone());
            }
        }
        /* Gardenfell 18ey: and every frame of a flipbook. It runs at fifteen frames a second,
           so a frame the page has to come back for is a hole in the water for as long as
           the trip takes (Tamriel Rebuilt's water circle: thirty-two 256-pixel frames). */
        if let Some(f) = &part.flip {
            for t in &f.frames {
                if !t.is_empty() && !texs.iter().any(|x| x == t) {
                    texs.push(t.clone());
                }
            }
        }
    }
    /* Round 17w: the mesh's particle systems ride along, and their textures are bundled
       with the parts' — a waterfall's mist and a candle's flame are textures like any
       other, and the page should not have to come back for them. */
    for sys in &systems {
        if !sys.tex.is_empty() && !texs.iter().any(|t| t == &sys.tex) {
            texs.push(sys.tex.clone());
        }
    }
    let pj = if systems.is_empty() { String::new() } else { viewcore::nif::particles_json_of(&systems) };
    /* Round 17y: and the light under AttachLight, if the file put one there — Glow in the
       Dahrk's windows do, for the light they throw into a room by day. (Read with the
       rest since 18cr, see `nif::read_for_draw`.) */
    (viewcore::preview::mesh_bytes_all(&parts, geom.as_ref(), attach, &pj, light), texs)
}

/// Round 18ag: the `.kf` that goes with a mesh path, parsed, or `None` when there is
/// none where the mesh is (or it is not one).
fn kf_beside(v: &Vfs, path: &str) -> Option<viewcore::nif::KfSequence> {
    // Round 18be (G7): the engine's, so the hull the world builds and the picture this
    // command draws are posed by the same sequence.
    viewcore::world::kf_beside(v, path)
}

#[tauri::command(async)]
fn mesh_data(path: String, still: Option<bool>, state: State) -> Result<tauri::ipc::Response, String> {
    let (v, w) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.world.clone())
    };
    Ok(tauri::ipc::Response::new(mesh_payload(&v, w.as_deref(), &path, still.unwrap_or(false))?.0))
}

/// Wraithguard's mesh viewer: a mesh's collision shape, for drawing over it.
///
/// ```text
/// "GDCL" | u8 shape (0 hull, 1 the visible mesh standing in, 2 none) | u32 n | n × 9 f32
/// ```
/// Object space, three corners a triangle - the soup the engine tests grass against.
#[tauri::command(async)]
fn mesh_collision(path: String, state: State) -> Result<tauri::ipc::Response, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    Ok(tauri::ipc::Response::new(collision_payload(&v, &path)?))
}

/// A mesh's bytes by the mesh viewer's path: a file on disk (`file:` or an absolute path,
/// one mod's own copy) or a load-order path.
fn mesh_bytes(v: &Vfs, path: &str) -> Result<Vec<u8>, String> {
    let disk = std::path::Path::new(path.strip_prefix("file:").unwrap_or(path));
    if path.starts_with("file:") || (disk.is_absolute() && disk.is_file()) {
        return std::fs::read(disk).map_err(|e| format!("{}: {}", path, e));
    }
    let p = v
        .resolve_mesh(path)
        .or_else(|| v.resolve(path))
        .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
    v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))
}

/* ---- the mesh viewer's block panel and Save (4.6, folding native/) ------------------

   These came from the Python host (`wraithguard/nif/inspect.py` and `edit.py` over
   `native/src/nif.rs`), handed to the page in the extra file and a loopback Save. The
   reader and the editor are conflictcore's now (`nifedit`, `nif_order`), so the viewer
   answers both itself, for any mesh it shows. */

/// The block tree, every block's fields (the editable ones marked) and what Optimize
/// would do (`conflictcore::nifedit::blocks`), as JSON text; `{"complete": false}` for a
/// mesh the crate cannot read through.
#[tauri::command(async)]
fn mesh_inspect(path: String, state: State) -> Result<String, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    let bytes = mesh_bytes(&v, &path)?;
    Ok(match conflictcore::nifedit::blocks(&bytes) {
        Ok(x) => x.to_string(),
        Err(e) => serde_json::json!({"tree": [], "blocks": {}, "complete": false, "error": e.to_string()}).to_string(),
    })
}

/// The mesh at `path` with `edits` applied (field sets by block index, `{"op":"optimize"}`
/// for the draw order) written to `out`, the path the user picked; through a `.tmp` so a
/// failed write leaves nothing half done.
#[tauri::command(async)]
fn mesh_save(path: String, edits: String, out: String, state: State) -> Result<String, String> {
    granted::check(&out)?;
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    let bytes = mesh_bytes(&v, &path)?;
    let edits: Vec<serde_json::Value> = serde_json::from_str(&edits).map_err(|e| e.to_string())?;
    let edited = conflictcore::nifedit::edit(&bytes, &edits).map_err(|e| e.to_string())?;
    let tmp = std::path::PathBuf::from(format!("{out}.tmp"));
    std::fs::write(&tmp, &edited).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &out).map_err(|e| format!("{out}: {e}"))?;
    Ok(out)
}

pub(crate) fn collision_payload(v: &Vfs, path: &str) -> Result<Vec<u8>, String> {
    // A group a script plays changes the picture, not what the object is: its file's hull.
    let path = viewcore::anim::model_of(path);
    let disk = std::path::Path::new(path.strip_prefix("file:").unwrap_or(path));
    let (bytes, kf) = if path.starts_with("file:") || (disk.is_absolute() && disk.is_file()) {
        let b = std::fs::read(disk).map_err(|e| format!("{}: {}", path, e))?;
        (b, std::fs::read(disk.with_extension("kf")).ok().and_then(|b| viewcore::nif::parse_kf(&b)))
    } else {
        let p = v
            .resolve_mesh(path)
            .or_else(|| v.resolve(path))
            .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
        let b = v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?;
        (b, kf_beside(v, path))
    };
    let mut out = b"GDCL".to_vec();
    match viewcore::nif::parse_with_kf(&bytes, kf.as_ref()) {
        Some(g) => {
            out.push(match g.shape {
                viewcore::nif::ColShape::Hull => 0,
                viewcore::nif::ColShape::Visible => 1,
                _ => 2,
            });
            out.extend_from_slice(&((g.tris.len() / 3) as u32).to_le_bytes());
            for c in g.tris.iter().take(g.tris.len() / 3 * 3) {
                for f in c {
                    out.extend_from_slice(&f.to_le_bytes());
                }
            }
        }
        None => {
            out.push(2);
            out.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    Ok(out)
}

/// Several meshes and the textures they name, in one answer (round 16b).
///
/// A cell load used to ask for every mesh, and then for every texture each mesh
/// turned out to name: a round trip per file, four hundred of them for nine cells,
/// each carrying its own overhead through the shell's IPC. The engine work behind
/// them is milliseconds (measured: 220 textures decode in 9 ms, 190 meshes parse in
/// 15 ms); the trips were the cost. Now the page sends a batch of mesh paths and the
/// texture keys it already holds, and gets back every mesh, parsed, and every texture
/// those meshes name that it did not have - read, decoded and capped on the engine's
/// threads, in parallel.
///
/// ```text
/// "GDNC" | u32 engineMs | u32 nMesh | per mesh: u16 len, path, u8 ok, u32 len, GDN2 mesh or the error
///        | u32 nTex  | per texture: u16 len, name as the mesh spells it, u8 ok, u32 len, GDN2 texture or the error
/// ```
#[tauri::command(async)]
// Each parameter is a named field of the page's request, not a positional argument
// anybody has to keep in order; the eighth is Gardenfell 18ez's `still`.
#[allow(clippy::too_many_arguments)]
fn assets_bundle(
    meshes: Vec<String>,
    have: Vec<String>,
    bc: Option<bool>,
    max_size: Option<u32>,
    bcx: Option<Vec<String>>,
    maps: Option<Vec<String>>,
    // Gardenfell 18ez: one flag a mesh, aligned with `meshes`; absent means none is still.
    still: Option<Vec<bool>>,
    state: State,
) -> Result<tauri::ipc::Response, String> {
    let extra = extra_of(&bcx);
    let maps = maps.unwrap_or_default();
    let (v, w) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.world.clone())
    };
    let bc = bc.unwrap_or(false);
    let max_size = max_size.unwrap_or(0);
    /* Round 18cr: the engine's own clock on the bundle, for the report's Loading line -
       so a slow "placing objects" can be told apart into the engine's reading, the trip
       through the shell, and the page's decoding. */
    let clock = std::time::Instant::now();
    let got: Vec<Result<(Vec<u8>, Vec<String>), String>> =
        {
            let still = still.unwrap_or_default();
            let asks: Vec<(&String, bool)> =
                meshes.iter().enumerate().map(|(i, m)| (m, still.get(i).copied().unwrap_or(false))).collect();
            viewcore::pool::par_map(&asks, |_, (m, st)| mesh_payload(&v, w.as_deref(), m, *st))
        };
    // Every texture the meshes name that the page does not hold, once each.
    let have: std::collections::HashSet<String> = have.iter().map(|h| viewcore::vfs::norm(h)).collect();
    let mut want: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in got.iter().flatten() {
        for t in &r.1 {
            let k = viewcore::vfs::norm(t);
            if have.contains(&k) || !seen.insert(k) {
                continue;
            }
            want.push(t.clone());
        }
    }
    /* Wraithguard: and the maps beside them - `maps` is the suffixes the page reads
       (`_nh`, `_n`, `_spec`, as the setup's OpenMW names them) - in the same bundle, the
       way the textures themselves come, rather than one trip each after it. Named as the
       page's own index names them: lower case, without `textures/`, the file's own
       extension. */
    if !maps.is_empty() {
        let bases: Vec<String> = want.clone();
        for t in bases {
            let n = viewcore::vfs::norm(&t).to_ascii_lowercase();
            let n = n.strip_prefix("textures/").unwrap_or(&n).to_string();
            let stem = match n.rfind('.') {
                Some(d) if !n[d..].contains('/') => n[..d].to_string(),
                _ => n.clone(),
            };
            for suf in &maps {
                let suf = suf.trim().to_ascii_lowercase();
                if suf.is_empty() || stem.ends_with(&suf) {
                    continue;
                }
                for ext in ["dds", "tga", "bmp", "png"] {
                    let name = format!("{stem}{suf}.{ext}");
                    if v.resolve(&format!("textures/{name}")).is_none() && v.resolve(&name).is_none() {
                        continue;
                    }
                    let k = viewcore::vfs::norm(&name);
                    if !have.contains(&k) && !have.contains(&format!("textures/{k}")) && seen.insert(k) {
                        want.push(name);
                    }
                    break;
                }
            }
        }
    }
    let texs: Vec<Result<Vec<u8>, String>> =
        viewcore::pool::par_map(&want, |_, t| texture_payload(&v, t, bc, extra, max_size));

    let mut out: Vec<u8> = Vec::new();
    let put_str = |out: &mut Vec<u8>, s: &str| {
        let b = s.as_bytes();
        out.extend_from_slice(&(b.len().min(65535) as u16).to_le_bytes());
        out.extend_from_slice(&b[..b.len().min(65535)]);
    };
    let put_res = |out: &mut Vec<u8>, r: Result<&[u8], &str>| {
        let (ok, b) = match r {
            Ok(b) => (1u8, b),
            Err(e) => (0u8, e.as_bytes()),
        };
        out.push(ok);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    };
    // "GDNC": "GDNB" with the engine's milliseconds for the whole bundle after the tag.
    out.extend_from_slice(b"GDNC");
    out.extend_from_slice(&(clock.elapsed().as_millis().min(u32::MAX as u128) as u32).to_le_bytes());
    out.extend_from_slice(&(meshes.len() as u32).to_le_bytes());
    for (m, r) in meshes.iter().zip(got.iter()) {
        put_str(&mut out, m);
        put_res(&mut out, r.as_ref().map(|x| x.0.as_slice()).map_err(|e| e.as_str()));
    }
    out.extend_from_slice(&(want.len() as u32).to_le_bytes());
    for (t, r) in want.iter().zip(texs.iter()) {
        put_str(&mut out, t);
        put_res(&mut out, r.as_ref().map(|b| b.as_slice()).map_err(|e| e.as_str()));
    }
    Ok(tauri::ipc::Response::new(out))
}

#[tauri::command(async)]
fn cpu_threads() -> usize {
    viewcore::pool::threads()
}

/// Wraithguard: Settings' "Read past unknown subrecords" (`editcore::plugin::set_lenient`):
/// set when `on` is given, and answered as it stands. Off until the page sets it from the
/// viewer profile.
#[tauri::command(async)]
fn plugins_lenient(on: Option<bool>) -> bool {
    match on {
        Some(v) => editcore::plugin::set_lenient(v),
        None => editcore::plugin::lenient(),
    }
}

/* ---- ORI: what a picked reference is, and where every part of it comes from ---------

   Wraithguard's object inspector, after the console's `ori`: click a placed object and
   see the reference, its base record, which plugins created and changed it, which
   plugins define the base record, and which data folder or archive the mesh and each
   of its textures resolve from in this setup. */

fn tag_text(t: &viewcore::esp::Tag) -> String {
    String::from_utf8_lossy(t).to_string()
}

/// `key` is the reference key the viewport carries (`plugin:index`); `model` the mesh
/// it drew, when the page has one.
#[tauri::command(async)]
fn ori(key: String, model: Option<String>, state: State) -> Result<String, String> {
    let (v, w) = {
        let app = state.app();
        (app.vfs.clone(), app.world.clone())
    };
    let w = w.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let want = key.to_ascii_lowercase();
    let is_key = |num: &viewcore::esp::RefNum| {
        viewcore::refkey::RefKey::of(*num, &w.plugins)
            .map(|k| viewcore::refkey::key_text(&k).to_ascii_lowercase() == want)
            .unwrap_or(false)
    };
    // The reference, the cell it stands in, which cell that is for a plugin, and the
    // cell's key in the conflict viewer (an interior's name; an exterior's, or its grid).
    use viewcore::inspect::CellSel;
    // Also the cell's key as a plugin's records key it (an interior's name, an exterior's
    // grid), and the plugins with a CELL record for it - the editor reads those.
    type Found<'a> = (&'a viewcore::esp::CellRef, String, CellSel, String, String, &'a [u32]);
    let mut found: Option<Found> = None;
    for (g, c) in &w.cells {
        if let Some(r) = c.refs.iter().find(|(n, _)| is_key(n)).map(|(_, r)| r) {
            let label = if c.name.is_empty() { format!("{}, {}", g.0, g.1) } else { format!("{} ({}, {})", c.name, g.0, g.1) };
            let grid = format!("({}, {})", g.0, g.1);
            let ck = if c.name.is_empty() { grid.clone() } else { c.name.clone() };
            found = Some((r, label, CellSel::Exterior(g.0, g.1), ck, grid, &w.history.exterior(*g).touched_by));
            break;
        }
    }
    if found.is_none() {
        for (k, room) in &w.interiors {
            if let Some(r) = room.refs.iter().find(|(n, _)| is_key(n)).map(|(_, r)| r) {
                let n = room.name.clone();
                found = Some((r, n.clone(), CellSel::Interior(n.clone()), n.clone(), n, &w.history.interior(k).touched_by));
                break;
            }
        }
    }
    let (r, cell, cell_sel, cell_key, ref_cell, cell_by) = found.ok_or_else(|| format!("no reference {key} in the loaded world"))?;
    let name = |i: i32| usize::try_from(i).ok().and_then(|i| w.plugins.get(i)).cloned().unwrap_or_default();
    let lid = r.id.to_ascii_lowercase();

    let mut o = J::obj();
    o.str("key", &key).str("id", &r.id).str("cell", &cell);
    // The plugin that created the reference, spelled as the load order spells it.
    let created = viewcore::refkey::RefKey::of(r.num, &w.plugins).map(|k| k.plugin).unwrap_or_default();
    let created = w.plugins.iter().find(|p| p.eq_ignore_ascii_case(&created)).cloned().unwrap_or(created);
    o.str("createdBy", &created);
    o.str("lastChangedBy", &name(r.plugin));
    let touched: Vec<String> = w.history.of(&r.num).touched.iter().map(|&i| name(i)).collect();
    o.raw("touchedBy", &string_array(&touched));
    o.raw("pos", &format!("[{:.1},{:.1},{:.1}]", r.pos[0], r.pos[1], r.pos[2]))
        .raw("rot", &format!("[{:.4},{:.4},{:.4}]", r.rot[0], r.rot[1], r.rot[2]))
        .num("scale", r.scale as f64);
    if let Some(d) = &r.door {
        o.str("doorTo", if d.cell.is_empty() { "exterior" } else { &d.cell });
    }
    // The base record: its type, name, and every plugin that defines it (the last wins).
    let defs: Vec<String> = w
        .record_where
        .get(&lid)
        .map(|v| {
            v.iter()
                .map(|(p, t)| {
                    let mut d = J::obj();
                    d.str("plugin", w.plugins.get(*p).map(String::as_str).unwrap_or("")).str("tag", &tag_text(t));
                    d.done()
                })
                .collect()
        })
        .unwrap_or_default();
    o.raw("definedIn", &format!("[{}]", defs.join(",")));
    if let Some(def) = w.objects.get(&lid) {
        o.str("name", &def.name);
    }
    o.str("cellKey", &cell_key).str("refCell", &ref_cell).num("refIndex", r.num.index as f64);
    let cell_plugins: Vec<String> = cell_by.iter().map(|&i| name(i as i32)).collect();
    o.raw("cellPlugins", &string_array(&cell_plugins));
    /* Wraithguard: the full help. The reference's own fields from the plugin that last
       changed it, and the base record's from the plugin that last defines it - read from
       the files now, as the world keeps only what drawing needs. */
    let read = |ix: i32| usize::try_from(ix).ok().and_then(|i| w.plugin_paths.get(i)).and_then(|p| std::fs::read(p).ok());
    if let Some(buf) = read(r.plugin) {
        if let Some(d) = viewcore::inspect::ref_details(&buf, &cell_sel, r.num.index, &r.id) {
            let mut j = J::obj();
            let npc_name = |id: &str| w.npc.npcs.get(&id.to_ascii_lowercase()).map(|n| n.name.clone()).unwrap_or_default();
            j.str("owner", &d.owner).str("ownerName", &npc_name(&d.owner)).str("ownerGlobal", &d.owner_global);
            j.str("faction", &d.faction).str("soul", &d.soul).str("key", &d.key).str("trap", &d.trap);
            let opt = |j: &mut J, k: &str, v: Option<f64>| {
                if let Some(v) = v {
                    j.num(k, v);
                }
            };
            opt(&mut j, "rank", d.rank.map(f64::from));
            opt(&mut j, "charge", d.charge.map(f64::from));
            opt(&mut j, "uses", d.uses.map(f64::from));
            opt(&mut j, "count", d.count.map(f64::from));
            opt(&mut j, "lock", d.lock.map(f64::from));
            j.bool("blocked", d.blocked);
            o.raw("refData", &j.done());
        }
    }
    let last_def = w.record_where.get(&lid).and_then(|v| v.last()).copied();
    if let Some((p, tag)) = last_def {
        if let Some(fields) = read(p as i32).and_then(|b| viewcore::inspect::record_fields(&b, tag, &r.id)) {
            let b = viewcore::inspect::base_details(tag, &fields);
            let mut j = J::obj();
            j.str("tag", &tag_text(&tag)).str("name", &b.name).str("script", &b.script);
            j.str("race", &b.race).str("class", &b.class).str("faction", &b.faction).bool("female", b.female);
            if let Some(l) = b.level {
                j.num("level", l as f64);
            }
            if let Some(c) = b.capacity {
                j.num("capacity", c as f64);
            }
            /* The items' names, icons and meshes, and a leveled list's entries - read from
               the plugins that define them, each plugin once (inspect::records_many). */
            let lookup = |ids: &[String]| -> HashMap<String, (viewcore::esp::Tag, Vec<(viewcore::esp::Tag, Vec<u8>)>)> {
                let mut by_plugin: HashMap<usize, std::collections::HashSet<String>> = HashMap::new();
                for id in ids {
                    let k = id.to_ascii_lowercase();
                    if let Some(&(p, _)) = w.record_where.get(&k).and_then(|v| v.last()) {
                        by_plugin.entry(p).or_default().insert(k);
                    }
                }
                let mut out = HashMap::new();
                for (p, want) in by_plugin {
                    if let Some(buf) = read(p as i32) {
                        out.extend(viewcore::inspect::records_many(&buf, &want));
                    }
                }
                out
            };
            let mut ids: Vec<String> = b.items.iter().map(|(_, id)| id.clone()).collect();
            if let Some(l) = &b.list {
                ids.extend(l.entries.iter().map(|(_, id)| id.clone()));
            }
            let mut found = lookup(&ids);
            // A leveled list's own entries, one level down.
            let inner: Vec<String> = found
                .values()
                .filter(|(t, _)| t == b"LEVI" || t == b"LEVC")
                .flat_map(|(_, f)| viewcore::inspect::leveled_list(f).entries.into_iter().map(|(_, id)| id))
                .filter(|id| !found.contains_key(&id.to_ascii_lowercase()))
                .collect();
            found.extend(lookup(&inner));
            let info = |id: &str| -> (String, viewcore::inspect::ItemInfo) {
                match found.get(&id.to_ascii_lowercase()) {
                    Some((t, f)) => (tag_text(t), viewcore::inspect::item_info(*t, f)),
                    None => (String::new(), viewcore::inspect::ItemInfo::default()),
                }
            };
            fn list_json(l: &viewcore::inspect::LeveledList, info: &dyn Fn(&str) -> (String, viewcore::inspect::ItemInfo)) -> String {
                let rows: Vec<String> = l
                    .entries
                    .iter()
                    .map(|(lv, id)| {
                        let (tag, i) = info(id);
                        let mut e = J::obj();
                        e.num("level", *lv as f64).str("id", id).str("tag", &tag).str("name", &i.name).str("icon", &i.icon).str("model", &i.model);
                        e.done()
                    })
                    .collect();
                let mut j = J::obj();
                j.raw("entries", &format!("[{}]", rows.join(","))).num("chanceNone", l.chance_none as f64).raw("flags", &string_array(&l.flags));
                j.done()
            }
            let items: Vec<String> = b
                .items
                .iter()
                .map(|(n, id)| {
                    let (tag, i) = info(id);
                    let mut it = J::obj();
                    it.num("count", *n as f64).str("id", id).str("tag", &tag).str("name", &i.name).str("icon", &i.icon).str("model", &i.model);
                    if let Some(l) = &i.list {
                        it.raw("list", &list_json(l, &info));
                    }
                    it.done()
                })
                .collect();
            j.raw("items", &format!("[{}]", items.join(",")));
            if let Some(l) = &b.list {
                j.raw("list", &list_json(l, &info));
            }
            if let Some(ai) = b.ai {
                j.raw("ai", &format!("[{},{},{},{}]", ai[0], ai[1], ai[2], ai[3]));
            }
            j.raw("services", &string_array(&b.services)).raw("travel", &string_array(&b.travel));
            j.raw("spells", &string_array(&b.spells));
            j.raw("flags", &string_array(&b.flags));
            o.raw("base", &j.done());
        }
    }
    // The mesh and its textures, and where each resolves in the setup.
    let mesh = model.filter(|m| !m.is_empty()).or_else(|| w.models.get(&lid).cloned()).unwrap_or_default();
    o.str("mesh", &mesh);
    if let Some(v) = v.as_ref() {
        if !mesh.is_empty() {
            /* Wraithguard: the meshes the viewer makes itself have no file in the setup -
               saying "not found" for them read as a missing asset. */
            let lm = mesh.to_ascii_lowercase().replace('\\', "/");
            let src = if lm.starts_with(viewcore::markers::PREFIX) {
                "built into Wraithguard (R-Zero's Construction Set markers)".to_string()
            } else if viewcore::npc::npc_request(&mesh).is_some() {
                "assembled from the NPC's skeleton and body parts".to_string()
            } else {
                v.resolve_mesh(&mesh).map(|l| l.ident(v)).unwrap_or_default()
            };
            o.str("meshFrom", &src);
            if let Ok((_, texs)) = mesh_payload(v, Some(&w), &mesh, false) {
                let rows: Vec<String> = texs
                    .iter()
                    .map(|t| {
                        let mut d = J::obj();
                        let from = if t.starts_with(viewcore::nif::EMBEDDED) {
                            "inside the mesh (an embedded texture)".to_string()
                        } else {
                            v.resolve_texture(t).map(|l| l.ident(v)).unwrap_or_default()
                        };
                        d.str("texture", t).str("from", &from);
                        d.done()
                    })
                    .collect();
                o.raw("textures", &format!("[{}]", rows.join(",")));
            }
        }
    }
    Ok(o.done())
}

/// Wraithguard: an actor's dialogue across the load order - the topics it has lines of its
/// own in (with how many, and from which plugins) and the quests those lines touch. Reads
/// every plugin, so the inspector asks for it only when the user does.
#[tauri::command(async)]
fn ori_dialogue(id: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let bufs: Vec<Vec<u8>> = viewcore::pool::par_map(&w.plugin_paths, |_, p| std::fs::read(p).unwrap_or_default());
    let views: Vec<&[u8]> = bufs.iter().map(|b| b.as_slice()).collect();
    let (topics, quests) = viewcore::inspect::dialogue_of(&views, &id);
    let rows: Vec<String> = topics
        .iter()
        .map(|t| {
            let mut j = J::obj();
            let names: Vec<&str> = t.plugins.iter().filter_map(|&i| w.plugins.get(i).map(String::as_str)).collect();
            j.str("topic", &t.name).num("kind", t.kind as f64).num("lines", t.lines as f64).raw("plugins", &string_array(&names));
            j.done()
        })
        .collect();
    let mut o = J::obj();
    o.raw("topics", &format!("[{}]", rows.join(","))).raw("quests", &string_array(&quests));
    Ok(o.done())
}

/// Wraithguard: where a record can be seen - for the conflict viewer's "Show in Cell
/// Preview". A CELL by its conflict key (an interior's name, an exterior's name or
/// "(x, y)"); anything else by the first reference to it (interiors by name, then the
/// exterior by grid, so the answer is the same every time), with how many there are.
/// `{kind:"int"|"ext", name, x, y, key, pos, count}`.
#[tauri::command(async)]
fn find_record(tag: String, id: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let want = id.trim().to_ascii_lowercase();
    let mut o = J::obj();
    if tag.eq_ignore_ascii_case("CELL") {
        if let Some(room) = w.interiors.get(&viewcore::world::room_key(&id)) {
            o.str("kind", "int").str("name", &room.name);
            return Ok(o.done());
        }
        let grid = want
            .strip_prefix('(')
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|r| r.split_once(','))
            .and_then(|(a, b)| Some((a.trim().parse::<i32>().ok()?, b.trim().parse::<i32>().ok()?)));
        let mut cells: Vec<&(i32, i32)> = w.cells.keys().collect();
        cells.sort();
        let hit = match grid {
            Some(g) => w.cells.contains_key(&g).then_some(g),
            None => cells.into_iter().find(|g| w.cells[g].name.eq_ignore_ascii_case(id.trim())).copied(),
        };
        let g = hit.ok_or_else(|| format!("no cell {id} in the loaded world"))?;
        o.str("kind", "ext").str("name", &w.cells[&g].name).num("x", g.0 as f64).num("y", g.1 as f64);
        return Ok(o.done());
    }
    let key_of = |r: &viewcore::esp::CellRef| {
        viewcore::refkey::RefKey::of(r.num, &w.plugins).map(|k| viewcore::refkey::key_text(&k)).unwrap_or_default()
    };
    let mut count = 0usize;
    let mut first: Option<(String, &viewcore::esp::CellRef, Option<(i32, i32)>)> = None;
    let mut rooms: Vec<&viewcore::world::Interior> = w.interiors.values().collect();
    rooms.sort_by_key(|a| a.name.to_ascii_lowercase());
    for room in rooms {
        let mut refs: Vec<&viewcore::esp::CellRef> = room.refs.values().filter(|r| !r.deleted && r.id.eq_ignore_ascii_case(&want)).collect();
        refs.sort_by_key(|r| (r.num.content_file, r.num.index));
        count += refs.len();
        if first.is_none() {
            if let Some(r) = refs.first() {
                first = Some((room.name.clone(), r, None));
            }
        }
    }
    let mut grids: Vec<&(i32, i32)> = w.cells.keys().collect();
    grids.sort();
    for g in grids {
        let c = &w.cells[g];
        let mut refs: Vec<&viewcore::esp::CellRef> = c.refs.values().filter(|r| !r.deleted && r.id.eq_ignore_ascii_case(&want)).collect();
        refs.sort_by_key(|r| (r.num.content_file, r.num.index));
        count += refs.len();
        if first.is_none() {
            if let Some(r) = refs.first() {
                first = Some((c.name.clone(), r, Some(*g)));
            }
        }
    }
    let (name, r, grid) = first.ok_or_else(|| format!("{id} is not placed in any cell of the loaded world"))?;
    match grid {
        Some(g) => o.str("kind", "ext").num("x", g.0 as f64).num("y", g.1 as f64),
        None => o.str("kind", "int"),
    };
    o.str("name", &name).str("key", &key_of(r)).num("count", count as f64);
    o.raw("pos", &format!("[{:.1},{:.1},{:.1}]", r.pos[0], r.pos[1], r.pos[2])).num("rot", r.rot[2] as f64);
    Ok(o.done())
}

/* ---- the settings file and the Setup page (port step 4.5) ----------------------------

   Wraithguard's settings file (`toolscore::settings`) is where every page's paths and options
   live: the openmw.cfg, the customizations TOML, the rule files, MOMW's file, tes3cmd and the
   rest. Launched by the Tk app it is the file the Tk app names (`extra.settingsFile`); on its
   own the program keeps it beside itself, where the Tk app kept it too.

   The Setup page shows and checks every one of them (`toolscore::setup`) and saves them. It
   opens by itself on a first launch (no cfg yet) and after a start that never finished
   loading its install (`setup_mark`: the page marks the start before it connects and clears
   it after), and is one button away the rest of the time, the connect's card included, so a
   malformed install can be put right without the load that hangs on it. */

/// The marker of a start that is loading its install (`setup_mark`), beside the program.
const LAST_START: &str = "wraithguard_last_start.json";

/// The settings file: the one named, else the one beside the program.
pub(crate) fn settings_path(given: &str) -> Result<PathBuf, String> {
    if given.trim().is_empty() { Ok(app_dir()?.join(toolscore::settings::FILE)) } else { Ok(PathBuf::from(given.trim())) }
}

/// The openmw.cfg the settings file names, when it is there: what the program opens on when
/// it is started on its own (no `--openmw-cfg`). (`mod.rs` calls it; check-commands, which
/// compiles this file without `mod.rs`, does not.)
#[allow(dead_code)]
pub(crate) fn settings_cfg() -> Option<String> {
    let s = toolscore::settings::read(&settings_path("").ok()?);
    let cfg = s.get("cfg")?.as_str()?.trim().to_string();
    // An openmw.cfg, an MO2 install (`mo2:...`) or a Data Files folder (`setup_spec`).
    let (_, path, _) = setup_spec(&cfg);
    std::path::Path::new(&path).exists().then_some(cfg)
}

/// Wraithguard: the pages' fields from the settings file, so each page opens on the setup the
/// person last used: `{loadorder, tools, conflicts, theme}`, each holding only the fields the
/// file sets. `settingsFile` empty: the one beside the program.
#[tauri::command(async)]
fn settings_seed(settings_file: String) -> Result<String, String> {
    let s = toolscore::settings::read(&settings_path(&settings_file)?);
    Ok(toolscore::settings::pages(&s).to_string())
}

/// Wraithguard: page fields (`toolscore::settings::all_fields`' names, a JSON object) written to
/// the settings file, every other key kept. Answers the file's fields after.
#[tauri::command(async)]
fn settings_write(settings_file: String, fields: String) -> Result<String, String> {
    let f: serde_json::Value = serde_json::from_str(&fields).map_err(|e| format!("the settings did not read: {e}"))?;
    let f = f.as_object().ok_or("the settings are not an object")?;
    let path = settings_path(&settings_file)?;
    // Security F5: the one beside the program, or the one it was started with (granted at
    // start-up, `mod.rs`); any other the person has to have chosen.
    if path != settings_path("")? {
        granted::check(&path.to_string_lossy())?;
    }
    let s = toolscore::settings::write_fields(&path, f).map_err(|e| format!("{} could not be written: {e}", path.display()))?;
    Ok(serde_json::Value::Object(toolscore::settings::all_fields(&s)).to_string())
}

/// Wraithguard: the Setup page's state - `{settingsFile, exists, fields, lastStart}`:
/// `lastStart` the marker of a start still loading (`{state, cfg, at}`), or null.
#[tauri::command(async)]
fn setup_state(settings_file: String) -> Result<String, String> {
    let path = settings_path(&settings_file)?;
    let s = toolscore::settings::read(&path);
    let last: serde_json::Value = std::fs::read_to_string(app_dir()?.join(LAST_START))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);
    Ok(serde_json::json!({
        "settingsFile": path.to_string_lossy(),
        "exists": path.is_file(),
        "fields": toolscore::settings::all_fields(&s),
        "lastStart": last,
    })
    .to_string())
}

/// Wraithguard: every field checked as the program will read it (`toolscore::setup`):
/// `{field: {level: empty|ok|warn|error, lines}}`.
#[tauri::command(async)]
fn setup_check(fields: String) -> Result<String, String> {
    let f: serde_json::Value = serde_json::from_str(&fields).map_err(|e| format!("the fields did not read: {e}"))?;
    Ok(toolscore::setup::check_all(f.as_object().ok_or("the fields are not an object")?).to_string())
}

/// Wraithguard: the openmw.cfg files OpenMW keeps on this machine (`toolscore::setup::find_cfgs`).
#[tauri::command(async)]
fn setup_find_cfgs() -> Result<String, String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    // Windows' Documents can be OneDrive's; the profile's own folder is looked in as well
    let onedrive = std::env::var_os("OneDrive").map(|o| PathBuf::from(o).join("Documents"));
    let mut found = toolscore::setup::find_cfgs(home.as_deref(), onedrive.as_deref());
    found.dedup();
    Ok(serde_json::json!(found.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>()).to_string())
}

/// Wraithguard: marks a start (`state` `connecting`, with the `cfg` it is loading) or clears
/// the mark (`state` `ok`). A mark still there at the next start means that start never
/// finished loading, and the Setup page opens first.
#[tauri::command(async)]
fn setup_mark(state: String, cfg: String) -> Result<(), String> {
    let path = app_dir()?.join(LAST_START);
    if state == "ok" {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    std::fs::write(&path, serde_json::json!({"state": state, "cfg": cfg, "at": at}).to_string()).map_err(|e| e.to_string())
}

/// Wraithguard: starts the program again, as it was started (the Setup page's "Restart",
/// for when the install it is loading will not finish), and ends this one.
#[tauri::command(async)]
fn app_restart() -> Result<(), String> {
    let exe = self_exe()?;
    std::process::Command::new(exe).args(std::env::args().skip(1)).spawn().map_err(|e| format!("the program could not be started again: {e}"))?;
    std::process::exit(0);
}

/// Wraithguard: each rule file's age in days (`null`: missing), for the page's rule list.
#[tauri::command(async)]
fn loadorder_rule_ages(rules: Vec<String>) -> Result<String, String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let paths: Vec<PathBuf> = rules.iter().map(PathBuf::from).collect();
    let ages = viewcore::ordercore::updates::rule_file_ages(&paths, now);
    Ok(serde_json::Value::from(ages.into_iter().map(|(n, a)| serde_json::json!([n, a])).collect::<Vec<_>>()).to_string())
}

/// Wraithguard: a mods folder scanned for the subset, `{lines, folders, plugins, start}`; the
/// lines go straight into the sort (`subsetLines`).
#[tauri::command(async)]
fn loadorder_scan(dir: String) -> Result<tauri::ipc::Response, String> {
    let scan = viewcore::ordercore::inputs::scan_mod_directories(&dir);
    let mut tb = Tables::new();
    tb.table("scan", "uus", 1);
    tb.u(scan.folders as u64).u(scan.plugins as u64).s(&scan.start);
    tb.table("lines", "s", scan.lines.len());
    for l in &scan.lines {
        tb.s(l);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/* ---- the cell map's coverage (Wraithguard) -------------------------------------------

   Which plugins touch which cells - Wraithguard's modmapper-style cell map, answered
   from the world the viewer already loaded rather than a second scan. Each exterior grid
   and each interior lists the plugins with a CELL record for it, by index into
   `plugins`, so a large load order does not repeat every name on every cell. */

/// Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed): `plugins`
/// `s`; `ext` `iiu` (x, y, a plugin with a CELL record for it - one row per plugin, in order);
/// `int` `su` (the room's name, a plugin, likewise); `extc` `iiuu` (x, y, contested
/// references, plugins editing the ground). Cells in the order the JSON had them. Read by
/// 39_wg_coverage.js.
#[tauri::command(async)]
fn cell_coverage(state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    // Sorted as the JSON's text was (`[x,y,...]` as a string), so the page's maps fill in
    // the same order.
    let mut ext: Vec<(String, (i32, i32), &Vec<u32>)> = w
        .cells
        .values()
        .map(|c| (c, &w.history.exterior(c.grid).touched_by))
        .filter(|(_, by)| !by.is_empty())
        .map(|(c, by)| {
            let ixs = by.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
            (format!("[{},{},[{}]]", c.grid.0, c.grid.1, ixs), c.grid, by)
        })
        .collect();
    ext.sort_by(|a, b| a.0.cmp(&b.0));
    /* Wraithguard: and per exterior, how contested its references are and how many
       plugins edit its ground - the map's other two heat modes. */
    let mut grids: std::collections::BTreeSet<(i32, i32)> = w.cells.keys().copied().collect();
    grids.extend(w.land_edits.keys().copied());
    let extc: Vec<((i32, i32), usize, usize)> = grids
        .into_iter()
        .filter_map(|g| {
            let conf = w.cells.get(&g).map(|c| viewcore::usage::conflicts(&c.refs, &w.history, w.history.exterior(g))).unwrap_or(0);
            let land = w.land_edits.get(&g).map(|v| v.len()).unwrap_or(0);
            (conf > 0 || land > 1).then_some((g, conf, land))
        })
        .collect();
    let mut int: Vec<(String, &str, &Vec<u32>)> = w
        .interiors
        .iter()
        .map(|(k, r)| (r, &w.history.interior(k).touched_by))
        .filter(|(_, by)| !by.is_empty())
        .map(|(r, by)| {
            let mut name = String::new();
            viewcore::json::escape_into(&r.name, &mut name);
            let ixs = by.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
            (format!("[\"{}\",[{}]]", name, ixs), r.name.as_str(), by)
        })
        .collect();
    int.sort_by(|a, b| a.0.cmp(&b.0));
    let mut tb = Tables::new();
    tb.table("plugins", "s", w.plugins.len());
    for p in &w.plugins {
        tb.s(p);
    }
    tb.table("ext", "iiu", ext.iter().map(|e| e.2.len()).sum());
    for (_, g, by) in &ext {
        for p in by.iter() {
            tb.i(g.0 as i64).i(g.1 as i64).u(*p as u64);
        }
    }
    tb.table("int", "su", int.iter().map(|e| e.2.len()).sum());
    for (_, name, by) in &int {
        for p in by.iter() {
            tb.s(name).u(*p as u64);
        }
    }
    tb.table("extc", "iiuu", extc.len());
    for (g, conf, land) in &extc {
        tb.i(g.0 as i64).i(g.1 as i64).u(*conf as u64).u(*land as u64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: the path grids of the cells asked for - `"x,y"` or `"int:<name>"` - with
/// their points in world units: `[{spec, from, points:[[x,y,z]..], edges:[[a,b]..]}]`.
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed): `grids`
   `ss` (the cell's spec, the plugin it is from), `points` `uddd` (its row in `grids`, x y z
   in world units) and `edges` `uuu` (its row in `grids`, the two points' indices within
   that grid). Read by 48_wg_tools.js. */
#[tauri::command(async)]
fn pathgrid(cells: Vec<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut grids: Vec<(&str, &str)> = Vec::new();
    let mut points: Vec<(usize, [f64; 3])> = Vec::new();
    let mut edges: Vec<(usize, u32, u32)> = Vec::new();
    for spec in &cells {
        let (pg, origin) = match viewcore::cells::CellKey::parse(spec) {
            Some(viewcore::cells::CellKey::Interior(name)) => (w.pathgrids_int.get(&viewcore::world::room_key(&name)), (0.0, 0.0)),
            Some(viewcore::cells::CellKey::Exterior(x, y)) => (w.pathgrids_ext.get(&(x, y)), (x as f32 * 8192.0, y as f32 * 8192.0)),
            None => (None, (0.0, 0.0)),
        };
        let Some(pg) = pg else { continue };
        let row = grids.len();
        grids.push((spec, plugin_name(&w, usize::try_from(pg.plugin).ok())));
        points.extend(pg.points.iter().map(|p| (row, [(p[0] as f32 + origin.0) as f64, (p[1] as f32 + origin.1) as f64, p[2] as f64])));
        edges.extend(pg.edges.iter().map(|&(a, b)| (row, a, b)));
    }
    let mut tb = Tables::new();
    tb.table("grids", "ss", grids.len());
    for (spec, from) in &grids {
        tb.s(spec).s(from);
    }
    tb.table("points", "uddd", points.len());
    for (row, p) in &points {
        tb.u(*row as u64).d(p[0]).d(p[1]).d(p[2]);
    }
    tb.table("edges", "uuu", edges.len());
    for (row, a, b) in &edges {
        tb.u(*row as u64).u(*a as u64).u(*b as u64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: OpenMW's navigation mesh over the cells asked for (as `pathgrid` takes
/// them), read from `navmesh.db` (`viewcore::navmesh`; its `encode` gives the layout).
/// `db` is a file or folder the user chose; without one the file is looked for beside
/// the setup's openmw.cfg, then in OpenMW's user data folder.
#[tauri::command(async)]
fn navmesh(cells: Vec<String>, db: Option<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let order = { state.app().order_file.clone() };
    let chosen = db.filter(|d| !d.trim().is_empty()).map(PathBuf::from);
    let cands = viewcore::navmesh::candidates(chosen.as_deref(), order.as_deref());
    let nav = viewcore::navmesh::NavDb::find(&cands)?;
    let found = nav.cells(&cells)?;
    Ok(tauri::ipc::Response::new(viewcore::navmesh::encode(&nav.path, &found)))
}

/// Wraithguard: several meshes' collision, in one answer (as `mesh_collision` gives one):
///
/// ```text
/// "GDCB" | u32 n | per mesh: u16 len, path, then mesh_collision's "GDCL" answer's length
///        (u32) and bytes
/// ```
#[tauri::command(async)]
fn collision_bundle(meshes: Vec<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    let got = viewcore::pool::par_map(&meshes, |_, m| collision_payload(&v, m).unwrap_or_else(|_| {
        let mut e = b"GDCL".to_vec();
        e.push(2);
        e.extend_from_slice(&0u32.to_le_bytes());
        e
    }));
    let mut out = b"GDCB".to_vec();
    out.extend_from_slice(&(meshes.len() as u32).to_le_bytes());
    for (m, b) in meshes.iter().zip(got) {
        let pb = m.as_bytes();
        out.extend_from_slice(&(pb.len().min(65535) as u16).to_le_bytes());
        out.extend_from_slice(&pb[..pb.len().min(65535)]);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&b);
    }
    Ok(tauri::ipc::Response::new(out))
}

/// Wraithguard: every cell where something is used - `kind` "mesh" (a mesh path), "id"
/// (a base record) or "texture" (on the meshes that name it, and on the ground):
/// `{ids, meshes, places:[{spec,label,count,pos,rot,ground}]}`.
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed): `ids`
   `s` (the records that use it), `meshes` `s` (the meshes that name it) and `places`
   `ssuddddu` (spec, label, count, x y z to 0.1, rot, 1 when it is the ground's). Read by
   48_wg_tools.js. */
#[tauri::command(async)]
fn where_used(kind: String, name: String, state: State) -> Result<tauri::ipc::Response, String> {
    let (v, w) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?)
    };
    let mut meshes: Vec<String> = Vec::new();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ground: Vec<viewcore::usage::Place> = Vec::new();
    match kind.as_str() {
        "id" => {
            ids.insert(name.to_ascii_lowercase());
        }
        "mesh" => {
            meshes.push(viewcore::usage::mesh_key(&name));
            ids = viewcore::usage::ids_using_mesh(&w, &name);
        }
        "texture" => {
            meshes = viewcore::usage::meshes_naming_texture(&w, &v, &name);
            for m in &meshes {
                ids.extend(viewcore::usage::ids_using_mesh(&w, m));
            }
            ground = viewcore::usage::land_places(&w, &name);
        }
        _ => return Err(format!("unknown kind {kind}")),
    }
    let places = viewcore::usage::places_of(&w, &ids);
    let mut idv: Vec<String> = ids.into_iter().collect();
    idv.sort();
    let mut tb = Tables::new();
    tb.table("ids", "s", idv.len());
    for id in &idv {
        tb.s(id);
    }
    tb.table("meshes", "s", meshes.len());
    for m in &meshes {
        tb.s(m);
    }
    tb.table("places", "ssuddddu", places.len() + ground.len());
    for (p, on_ground) in places.iter().map(|p| (p, false)).chain(ground.iter().map(|p| (p, true))) {
        tb.s(&p.spec).s(&p.label).u(p.count as u64);
        for v in p.pos {
            tb.d(rounded(v, 1));
        }
        tb.d(p.rot as f64).u(on_ground as u64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: every place in the setup that supplies an asset, winner first -
/// `[{label, from, disk, winner}]` (`disk` the file's path when it is loose, for the mesh
/// viewer's side-by-side compare).
#[tauri::command(async)]
fn asset_providers(path: String, kind: String, state: State) -> Result<String, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    /* The places in the order the resolver looks (patch 0003), so the first row - the
       winner - is the file the viewer draws: for a texture `.dds` before the spelled name,
       in every folder and then every archive, with and without `textures/`. */
    let list = match kind.as_str() {
        "texture" => v.texture_providers(&path),
        "mesh" => v.mesh_providers(&path),
        _ => v.providers(&path, &[]),
    };
    let rows: Vec<String> = list
        .iter()
        .enumerate()
        .map(|(i, (label, loc))| {
            let mut o = J::obj();
            o.str("label", label)
                .str("from", &loc.ident(&v))
                .str(
                    "disk",
                    &match loc {
                        viewcore::vfs::Loc::Disk(p) => p.to_string_lossy().to_string(),
                        _ => String::new(),
                    },
                )
                .bool("winner", i == 0);
            o.done()
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

/// Wraithguard: the game settings the overlays use, from the load order's GMSTs (the
/// game's defaults where no plugin sets one).
#[tauri::command(async)]
fn game_settings(state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let g = |k: &str, d: f32| w.gmst.get(k).copied().unwrap_or(d);
    // The first-person body's movement (06_gl.js, 71_wg_walk.js): OpenMW's formulas over
    // these, each the game's default where the load order sets none.
    let movement: serde_json::Map<String, serde_json::Value> = MOVEMENT_GMSTS.iter().map(|(k, d)| (k.to_string(), serde_json::json!(g(&k.to_ascii_lowercase(), *d)))).collect();
    Ok(serde_json::json!({
        "activate": g("imaxactivatedist", 192.0),
        "greetMult": g("igreetdistancemultiplier", 6.0),
        // The greeting's length and how far the player goes before another (77_wg_ai.js `greet`).
        "greetDuration": g("igreetduration", 4.0),
        "greetReset": g("fgreetdistancereset", 512.0),
        "move": movement,
    })
    .to_string())
}

/// The game settings the first-person body moves by, with Morrowind's defaults.
const MOVEMENT_GMSTS: [(&str, f32); 20] = [
    ("fMinWalkSpeed", 100.0),
    ("fMaxWalkSpeed", 200.0),
    // A creature's walk (77_wg_ai.js): the same formula over these.
    ("fMinWalkSpeedCreature", 5.0),
    ("fMaxWalkSpeedCreature", 300.0),
    ("fBaseRunMultiplier", 1.75),
    ("fAthleticsRunBonus", 1.0),
    ("fEncumberedMoveEffect", 0.3),
    ("fSneakSpeedMultiplier", 0.75),
    ("fSwimRunBase", 0.5),
    ("fSwimRunAthleticsMult", 0.1),
    ("fSwimHeightScale", 0.9),
    ("fMinFlySpeed", 5.0),
    ("fMaxFlySpeed", 50.0),
    ("fJumpAcrobaticsBase", 128.0),
    ("fJumpAcroMultiplier", 4.0),
    ("fJumpRunMultiplier", 1.0),
    ("fJumpEncumbranceBase", 0.5),
    ("fJumpEncumbranceMultiplier", 1.0),
    ("fFatigueBase", 1.25),
    ("fFatigueMult", 0.5),
];

/// Wraithguard: what the scene's script interpreter (`ui/src/73_wg_scripts.js`) starts from:
/// for the object ids asked about, the script each runs (`objects: {id: script}`); those
/// scripts' and the start scripts' source (`scripts: {lowercased id: {id, text}}`); the start
/// scripts (`start`); every global variable (`globals: [{name, kind, value}]`); and the actors
/// and containers among the ids (`actors: {id: {tag, items: [[item, count]], stats}}`).
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed; the scene's
   scripts whole): `objects` `ss` (object id, its script), `scripts` `sss` (name, id, text - the
   scripts of those objects and the start scripts), `start` `s`, `globals` `ssd` (name, kind,
   value), `actors` `ssu` (id, NPC_ / CREA / CONT, has stats 0/1), with
   `actor_items` `usi` (its row, item, count) and `actor_stats` `ui` (its row, a stat, in
   order). Read by `WgScripts.readScene` (73_wg_scripts.js). */
#[tauri::command(async)]
fn scene_scripts(ids: Vec<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut objects: Vec<(String, &String)> = Vec::new();
    let mut want: std::collections::BTreeSet<String> = w.start_scripts.iter().map(|s| s.to_ascii_lowercase()).collect();
    // The actors and containers among them: their type, what they carry, their health,
    // magicka and fatigue (the scripts' GetItemCount, GetHealth, AI and combat).
    let mut actors: Vec<(String, String, Option<&[i32; 10]>, &[(String, i32)])> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        let k = id.to_ascii_lowercase();
        if !seen.insert(k.clone()) {
            continue;
        }
        if let Some(sc) = w.object_scripts.get(&k) {
            want.insert(sc.clone());
            objects.push((k.clone(), sc));
        }
        let tag = w.records.placed_type(&k);
        if let Some(t @ (b"NPC_" | b"CREA" | b"CONT")) = tag.as_ref() {
            let items = w.inventories.get(&k).map(|v| v.as_slice()).unwrap_or(&[]);
            actors.push((k.clone(), String::from_utf8_lossy(t).into_owned(), w.actor_stats.get(&k), items));
        }
    }
    let scripts: Vec<(&String, &(String, String))> = want.iter().filter_map(|k| w.script_texts.get(k).map(|s| (k, s))).collect();
    let mut tb = Tables::new();
    tb.table("objects", "ss", objects.len());
    for (k, s) in &objects {
        tb.s(k).s(s);
    }
    tb.table("scripts", "sss", scripts.len());
    for (k, (id, text)) in &scripts {
        tb.s(k).s(id).s(text);
    }
    tb.table("start", "s", w.start_scripts.len());
    for s in &w.start_scripts {
        tb.s(s);
    }
    tb.table("globals", "ssd", w.globals.len());
    for (name, kind, value) in w.globals.values() {
        // The f32's own value, widened (as serde_json wrote it).
        tb.s(name).s(&kind.to_string()).d(*value as f64);
    }
    tb.table("actors", "ssu", actors.len());
    for (k, tag, stats, _) in &actors {
        tb.s(k).s(tag).u(stats.is_some() as u64);
    }
    tb.table("actor_items", "usi", actors.iter().map(|a| a.3.len()).sum());
    for (row, (_, _, _, items)) in actors.iter().enumerate() {
        for (item, n) in items.iter() {
            tb.u(row as u64).s(item).i(*n as i64);
        }
    }
    tb.table("actor_stats", "ui", actors.iter().map(|a| a.2.map(|s| s.len()).unwrap_or(0)).sum());
    for (row, (_, _, stats, _)) in actors.iter().enumerate() {
        for v in stats.iter().flat_map(|s| s.iter()) {
            tb.u(row as u64).i(*v as i64);
        }
    }
    /* Wraithguard: the AI ratings, `actor_rates` `uuuuu` (row; hello, fight, flee, alarm), for
       the Preview's AI (77_wg_ai.js): an auto-calculated NPC has them too. */
    let rated: Vec<(usize, &[u8; 4])> = actors.iter().enumerate().filter_map(|(i, a)| w.ai_rates.get(&a.0).map(|r| (i, r))).collect();
    tb.table("actor_rates", "uuuuu", rated.len());
    for (row, r) in &rated {
        tb.u(*row as u64).u(r[0] as u64).u(r[1] as u64).u(r[2] as u64).u(r[3] as u64);
    }
    /* Wraithguard: each actor's own AI packages, in order, `actor_packages` `uss` (row; kind;
       args as MWScript's Ai* take them, `|` between, the Reset flag last), started when the Scripts run. */
    let packs: Vec<(usize, &(String, String))> =
        actors.iter().enumerate().flat_map(|(i, a)| w.ai_packages.get(&a.0).into_iter().flatten().map(move |p| (i, p))).collect();
    tb.table("actor_packages", "uss", packs.len());
    for (row, (kind, args)) in &packs {
        tb.u(*row as u64).s(kind).s(args);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: scripts' source by id (a script another starts, `StartScript`):
/// `{lowercased id: {id, text}}`, the ones the load order has.
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed; whole
   script sources): `scripts` `sss` (the name asked, lower case; the record's id; its text),
   for those the load order has. Read by 73_wg_scripts.js. */
#[tauri::command(async)]
fn script_texts(names: Vec<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<(String, &String, &String)> = names
        .iter()
        .filter_map(|n| {
            let k = n.to_ascii_lowercase();
            let (id, text) = w.script_texts.get(&k)?;
            seen.insert(k.clone()).then_some((k, id, text))
        })
        .collect();
    let mut tb = Tables::new();
    tb.table("scripts", "sss", rows.len());
    for (k, id, text) in &rows {
        tb.s(k).s(id).s(text);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: the animation groups each model's `.kf` marks (`viewcore::anim::groups`),
/// for the scripts' PlayGroup, LoopGroup and `openmw.animation`: `{model: [[group, start,
/// stop, velocity], ...]}`, the window in seconds and how fast the group carries the actor
/// across the ground (`viewcore::anim::velocity`, units a second; 0 for one that does not). An NPC (`__npc/<id>`) answers the shared
/// skeleton's, `xbase_anim`; a model with no `.kf` answers an empty list.
#[tauri::command(async)]
fn anim_groups(models: Vec<String>, state: State) -> Result<String, String> {
    let v = vfs_of(&state)?;
    let mut out = serde_json::Map::new();
    for m in models {
        let file = if viewcore::npc::npc_id_of(&m).is_some() { "xbase_anim.nif".to_string() } else { viewcore::anim::model_of(&m).to_string() };
        let list: Vec<serde_json::Value> = kf_beside(&v, &file)
            // Each group's window and, fourth, how fast it carries the actor across the ground
            // (units a second: the walk is played at the actor's pace against it).
            .map(|k| viewcore::anim::groups(&k).into_iter().map(|(g, a, b)| { let v = viewcore::anim::velocity(&k, a, b); serde_json::json!([g, a, b, (v * 100.0).round() / 100.0]) }).collect())
            .unwrap_or_default();
        out.insert(m, serde_json::Value::Array(list));
    }
    Ok(serde_json::Value::Object(out).to_string())
}

/// Wraithguard: OpenMW's animation blending rules (the Preview's transitions between an actor's
/// groups, `ui/src/79_wg_animblend.js`), as text for the page to read: `global`, the load
/// order's `animations/animation-config.yaml` (or `.json`) - a mod's, else OpenMW's own in its
/// resources folder - and `files`, per model asked about, the `.yaml` or `.json` beside the
/// `.kf` it animates with, which OpenMW reads for that file's animations
/// (docs/source/reference/modding/animation-blending.rst). Null where there is none.
#[tauri::command(async)]
fn anim_blend_config(models: Vec<String>, state: State) -> Result<String, String> {
    let v = vfs_of(&state)?;
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    let read = |p: &str| v.resolve(p).and_then(|l| v.load(&l)).map(text);
    let global = read("animations/animation-config.yaml")
        .or_else(|| read("animations/animation-config.json"))
        .or_else(|| {
            let r = LUA_RESOURCES.lock().ok()?.clone()?;
            let d = r.join("vfs").join("animations");
            std::fs::read(d.join("animation-config.yaml")).or_else(|_| std::fs::read(d.join("animation-config.json"))).ok().map(text)
        });
    let mut files = serde_json::Map::new();
    for m in models {
        let file = if viewcore::npc::npc_id_of(&m).is_some() { "xbase_anim.nif".to_string() } else { viewcore::anim::model_of(&m).to_string() };
        let Some(dot) = file.rfind('.') else { continue };
        let stem = &file[..dot];
        let beside = |ext: &str| {
            let p = format!("{stem}.{ext}");
            v.resolve_mesh(&p).or_else(|| v.resolve(&p)).and_then(|l| v.load(&l)).map(text)
        };
        let t = beside("yaml").or_else(|| beside("json"));
        files.insert(m, t.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
    }
    Ok(serde_json::json!({"global": global, "files": files}).to_string())
}

/// Wraithguard: what the scene's OpenMW Lua (`ui/src/74_wg_lua.js`) starts from: the load
/// order's registered scripts, `scripts: [{path, flags}]` in load order (none, and a `note`,
/// for a Morrowind.ini setup), and the record type of each object id asked about,
/// `types: {id: tag}`, which decides the scripts an object gets (`NPC`, `LIGHT`, ...).
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed; a type per
   object of the scene): `head` `s` (a note: why there are no scripts), `scripts` `s` (each
   registered script's path), `script_flags` `us` (its row, a flag), `script_files` `us` (its
   row, a content file that registers it), `types` `ss` (an id asked,
   lower case; its record type). Read by 74_wg_lua.js. */
#[tauri::command(async)]
fn lua_scene(ids: Vec<String>, state: State) -> Result<tauri::ipc::Response, String> {
    let (vfs, order, w) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone(), app.world.clone())
    };
    let reg = viewcore::luascan::registered_by_file(&vfs, order.as_deref())?;
    let note = if reg.is_none() { "Not an OpenMW setup: Morrowind.ini load orders have no Lua scripts." } else { "" };
    let scripts = reg.unwrap_or_default();
    let mut types: Vec<(String, String)> = Vec::new();
    if let Some(w) = &w {
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            let k = id.to_ascii_lowercase();
            if let Some(t) = w.records.placed_type(&k) {
                if seen.insert(k.clone()) {
                    types.push((k, String::from_utf8_lossy(&t).into_owned()));
                }
            }
        }
    }
    let mut tb = Tables::new();
    tb.table("head", "s", 1);
    tb.s(note);
    tb.table("scripts", "s", scripts.len());
    for (path, _, _) in &scripts {
        tb.s(path);
    }
    tb.table("script_flags", "us", scripts.iter().map(|(_, f, _)| f.len()).sum());
    for (row, (_, flags, _)) in scripts.iter().enumerate() {
        for f in flags {
            tb.u(row as u64).s(f);
        }
    }
    // The content files that register each script (`.omwscripts`, `.omwaddon`): the panel's by-file mode.
    tb.table("script_files", "us", scripts.iter().map(|(_, _, n)| n.len()).sum());
    for (row, (_, _, names)) in scripts.iter().enumerate() {
        for n in names {
            tb.u(row as u64).s(n);
        }
    }
    tb.table("types", "ss", types.len());
    for (k, t) in &types {
        tb.s(k).s(t);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: the Preview's OpenMW Lua (`viewcore::luarun`), one at a time - in a worker
/// process (`viewcore::luaproc`, this program started again with `--lua-worker`) that is
/// ended when a call runs past its time, wherever in the Lua it is (security, 2026-11-23:
/// the Lua's own count hook does not reach into a native call).
static LUA_RUN: std::sync::Mutex<Option<viewcore::luaproc::Proc>> = std::sync::Mutex::new(None);

/// How long the Lua may take to start (every script read and run once), and each frame.
const LUA_START_LIMIT: std::time::Duration = std::time::Duration::from_secs(90);
const LUA_STEP_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);

/// A worker with the Lua started in it.
fn lua_worker(jit: Option<&std::path::Path>) -> Result<viewcore::luaproc::Proc, String> {
    // The program's own file, not the AppImage around it (that would be unpacked again).
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    viewcore::luaproc::Proc::start(&exe, &["--lua-worker"], &[], jit, false, LUA_START_LIMIT)
}

/// Wraithguard: starts the Preview's OpenMW Lua on the install's own LuaJIT (Lua 5.4 where
/// none is found): OpenMW's built-ins and the load order's scripts read, the global and
/// player scripts started. `weathers` are the Preview's weather names, in order.
/// `{backend, resources, note, answer: {effects, log, status}}`; a Morrowind.ini setup has
/// no Lua and gets only a `note`.
/// `off`: the scripts turned off in the Scripts panel (paths, and `@builtins` for all of
/// OpenMW's own), which are not started (`luarun::start_json`).
#[tauri::command(async)]
fn lua_run_start(weathers: Vec<String>, openmw: Option<String>, off: Option<Vec<String>>, extra: Option<serde_json::Value>, state: State) -> Result<String, String> {
    let (vfs, order) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone())
    };
    *LUA_RUN.lock().map_err(|e| e.to_string())? = None;
    let Some(reg) = viewcore::luascan::registered(&vfs, order.as_deref())? else {
        return Ok(serde_json::json!({"note": "Not an OpenMW setup: Morrowind.ini load orders have no Lua scripts."}).to_string());
    };
    let cfg = order.as_deref().filter(|p| viewcore::luascan::is_openmw_cfg(p));
    let resources = lua_resources(openmw, cfg);
    let jit = resources.as_deref().and_then(viewcore::luarun::find_luajit);
    let mut run = lua_worker(jit.as_deref())?;
    /* Any record of the load order the scripts ask for by id (`luarec`, Static 2026-11-23: "look
       up any record from the load order on demand, not just placed ones"). */
    /* And the files a script reads (`luafiles`, Static 2026-11-23): a text file of the data
       folders, YAML and TOML as data (kinds that begin with `@`). */
    {
        let mut recs = state.app().world.clone().map(viewcore::luarec::Records::of_world);
        let files = vfs.clone();
        run.records = Some(Box::new(move |kinds: &str, id: &str| {
            if kinds.starts_with('@') {
                return viewcore::luafiles::answer(&files, kinds, id);
            }
            let kinds: Vec<&str> = kinds.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
            recs.as_mut()?.lookup(&kinds, id)
        }));
    }
    let content = viewcore::luascan::content(order.as_deref());
    // The load order's numeric game settings, for `core.getGMST`.
    let gmst: Vec<(String, f32)> = state.app().world.as_ref().map(|w| w.gmst.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
    let gmst_text: Vec<(String, String)> = state.app().world.as_ref().map(|w| w.gmst_text.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    let start = viewcore::luarun::start_json(&vfs, resources.as_deref(), &reg, &weathers, &content, &off.unwrap_or_default(), &gmst, &gmst_text);
    /* A mod the page made, run with the load order's (Tools → HTN AI's run on FluadHTN, Static
       2026-11-23): `{files: [[path, text]...], scripts: [[path, [flags]]...], tableExt}`. */
    let start = match extra.as_ref().filter(|e| e.is_object()) {
        Some(e) => {
            let files: Vec<(String, String)> = e["files"].as_array().map(|a| a.iter().filter_map(|f| Some((f.get(0)?.as_str()?.to_string(), f.get(1)?.as_str()?.to_string()))).collect()).unwrap_or_default();
            let scripts: Vec<(String, Vec<String>)> = e["scripts"]
                .as_array()
                .map(|a| a.iter().filter_map(|f| Some((f.get(0)?.as_str()?.to_string(), f.get(1)?.as_array()?.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()))).collect())
                .unwrap_or_default();
            viewcore::luarun::start_with_extra(&start, &vfs, &files, &scripts, e["tableExt"].as_bool().unwrap_or(false))
        }
        None => start,
    };
    /* The settings the player saved in OpenMW (`player_storage.bin`, `global_storage.bin` beside
       openmw.cfg): in the storage sections before any script runs (`luafiles::storage_json`). */
    let start = {
        let dir = cfg.and_then(|c| c.parent());
        let read = |n: &str| dir.and_then(|d| std::fs::read(d.join(n)).ok()).and_then(|b| viewcore::luafiles::storage_json(&b));
        match (read("player_storage.bin"), read("global_storage.bin")) {
            (None, None) => start,
            (p, g) => format!("{{\"storage\":{},{}", serde_json::json!({"player": p, "global": g}), &start[1..]),
        }
    };
    let answer = run.call("__wg_start", &start, LUA_START_LIMIT)?;
    let backend = run.backend.clone();
    *LUA_RUN.lock().map_err(|e| e.to_string())? = Some(run);
    *LUA_RESOURCES.lock().map_err(|e| e.to_string())? = resources.clone();
    // nearby.castRay's world: the land now, the objects' shapes as the frames bring them.
    let world = { state.app().world.clone() };
    *viewcore::luarun::COLLIDER.lock().map_err(|e| e.to_string())? = Some(viewcore::luarun::Collider { world, ..Default::default() });
    *LUA_SHAPES.lock().map_err(|e| e.to_string())? = None;
    // A new worker has none of the collision yet.
    *LUA_SENT.lock().map_err(|e| e.to_string())? = LuaSent::default();
    let note = if resources.is_none() { "OpenMW's resources folder was not found: its built-in scripts (I.AI, activation, music, combat) are not running. Choose the OpenMW folder (the one with openmw.exe) to run them." } else { "" };
    let answer: serde_json::Value = serde_json::from_str(&answer).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"backend": backend, "resources": resources.map(|p| p.display().to_string()), "note": note, "answer": answer}).to_string())
}

/// Wraithguard: OpenMW's resources folder for the Preview's Lua: the OpenMW folder the person
/// chose (or its resources), before the cfg and the usual places; else Setup's OpenMW folder
/// (the settings file's `openmw_install`).
fn lua_resources(openmw: Option<String>, cfg: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let chosen = openmw
        .filter(|p| !p.trim().is_empty())
        .or_else(|| {
            let s = toolscore::settings::read(&settings_path("").ok()?);
            s.get("openmw_install").and_then(|v| v.as_str()).map(str::to_string).filter(|p| !p.trim().is_empty())
        })
        .map(|p| std::path::PathBuf::from(p.trim().trim_matches('"')));
    viewcore::luacore::findings::find_resources(cfg, chosen.as_deref())
}

/// Wraithguard: "Check my Lua mods" (Tools; Static 2026-11-23). Each Lua mod of the load order
/// run on its own - OpenMW's built-ins, the mod's scripts, a small scene (the player and an NPC,
/// a container), records and files answered as in the Preview - for a start and three frames,
/// and what its scripts said sorted (`viewcore::luacheck::report`): a dependency missing, a
/// plugin not loaded, the mod's own fault, other errors, and what the Preview does not run yet,
/// ranked. MWSE mods each `main.lua` on its own. With the load-order lint (`luacheck::lint`).
/// Three workers at a time, one record index between them.
/// Wraithguard: the load order's Lua, read once for the Tools page and the Scripts panel
/// (`lua_check`, `lua_deps`): the registered scripts and their files, every source with OpenMW's
/// own (the start message `start_json` makes, everything on), and OpenMW's own registered scripts.
struct LuaAll {
    vfs: std::sync::Arc<viewcore::vfs::Vfs>,
    world: Option<std::sync::Arc<viewcore::world::World>>,
    reg_files: Vec<(String, Vec<String>, Vec<String>)>,
    reg: Vec<(String, Vec<String>)>,
    content: Vec<String>,
    all: serde_json::Value,
    sources: std::collections::HashMap<String, String>,
    builtins: Vec<serde_json::Value>,
    resources: Option<std::path::PathBuf>,
}
fn lua_all(openmw: Option<String>, state: &State) -> Result<LuaAll, String> {
    let (vfs, order, world) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone(), app.world.clone())
    };
    let Some(reg_files) = viewcore::luascan::registered_by_file(&vfs, order.as_deref())? else {
        return Err("Not an OpenMW setup: Morrowind.ini load orders have no Lua scripts.".into());
    };
    let cfg = order.as_deref().filter(|p| viewcore::luascan::is_openmw_cfg(p));
    let resources = lua_resources(openmw, cfg);
    let content = viewcore::luascan::content(order.as_deref());
    let gmst: Vec<(String, f32)> = world.as_ref().map(|w| w.gmst.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
    let gmst_text: Vec<(String, String)> = world.as_ref().map(|w| w.gmst_text.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    let reg: Vec<(String, Vec<String>)> = reg_files.iter().map(|(p, f, _)| (p.clone(), f.clone())).collect();
    let all = viewcore::luarun::start_json(&vfs, resources.as_deref(), &reg, &["Clear".to_string()], &content, &[], &gmst, &gmst_text);
    let all: serde_json::Value = serde_json::from_str(&all).map_err(|e| e.to_string())?;
    let sources = all["sources"].as_object().map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
    let builtins: Vec<serde_json::Value> = all["scripts"].as_array().cloned().unwrap_or_default().into_iter()
        .filter(|s| s["path"].as_str().is_some_and(|p| !reg.iter().any(|(q, _)| q.eq_ignore_ascii_case(p)))).collect();
    Ok(LuaAll { vfs, world, reg_files, reg, content, all, sources, builtins, resources })
}

/// Wraithguard: what each registered Lua script needs on with it (`luacheck::deps`): the scripts
/// that provide the interfaces its mod uses, `@builtins` for OpenMW's own - the Scripts panel turns
/// them on with it (Static 2026-11-23).
#[tauri::command(async)]
fn lua_deps(openmw: Option<String>, state: State) -> Result<String, String> {
    let a = lua_all(openmw, &state)?;
    let reg: Vec<String> = a.reg.iter().map(|(p, _)| p.clone()).collect();
    let builtins: Vec<String> = a.builtins.iter().filter_map(|s| s["path"].as_str().map(str::to_string)).collect();
    Ok(viewcore::luacheck::deps(&a.sources, &reg, &builtins).to_string())
}

/// Wraithguard: Tools → HTN AI (Static 2026-11-23): what an exported mod needs and whether the
/// setup has it - FluadHTN's OpenMW and MWSE builds in the data folders, and Rubic0n's LuaJIT in
/// the OpenMW install (`viewcore::htn::deps`).
#[tauri::command(async)]
fn htn_deps(openmw: Option<String>, state: State) -> Result<String, String> {
    let (vfs, order) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone())
    };
    let cfg = order.as_deref().filter(|p| viewcore::luascan::is_openmw_cfg(p));
    let jit = lua_resources(openmw, cfg).as_deref().and_then(viewcore::luarun::find_luajit);
    let mut v = viewcore::htn::deps(&vfs, jit.as_deref());
    v["openmw_setup"] = serde_json::Value::Bool(cfg.is_some());
    /* Static 2026-11-23: "its in the customizations toml not the cfg". The data folders the
       customizations TOML (the Load order page's, from Wraithguard's settings) adds, which the
       MOMW configurator writes into openmw.cfg: FluadHTN found there is said to be on its way. */
    let toml = settings_path("")
        .ok()
        .map(|p| toolscore::settings::read(&p))
        .and_then(|s| s.get("customizations").and_then(|x| x.as_str()).map(str::to_string))
        .filter(|p| !p.trim().is_empty());
    if let Some(path) = toml {
        let data: Vec<String> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| viewcore::ordercore::inputs::customizations(&t).ok())
            .map(|c| c.data.iter().map(|d| d.value.clone()).collect())
            .unwrap_or_default();
        let mut t = viewcore::htn::in_toml(&data);
        t["path"] = serde_json::Value::String(path);
        let omw = v["omw"].as_bool().unwrap_or(false);
        if !omw && t["omw"].is_string() {
            v["hint"] = serde_json::Value::String(format!(
                "FluadHTN is in your customizations TOML ({}) but not in openmw.cfg yet: run the MOMW configurator (or Export on the Load order page) so openmw.cfg gets its data= line, then reload.",
                t["omw"].as_str().unwrap_or("")
            ));
        }
        v["toml"] = t;
    }
    Ok(v.to_string())
}

/// Wraithguard: an HTN domain the person picked (`*.wghtn.json`), read back.
#[tauri::command(async)]
fn htn_read(path: String) -> Result<String, String> {
    let p = granted::check(&path)?;
    Ok(viewcore::htn::read_domain(&p)?.to_string())
}

/// Wraithguard: the mod Tools → HTN AI made, written into the folder the person picked:
/// `files` is `[[relative path, text]...]`, each below that folder (`viewcore::htn::write_mod`).
#[tauri::command(async)]
fn htn_export(folder: String, files: serde_json::Value) -> Result<String, String> {
    let dir = granted::check(&folder)?;
    let list: Vec<(String, String)> = files
        .as_array()
        .ok_or("no files were given")?
        .iter()
        .filter_map(|f| Some((f.get(0)?.as_str()?.to_string(), f.get(1)?.as_str()?.to_string())))
        .collect();
    let out = viewcore::htn::write_mod(&dir, &list)?;
    Ok(serde_json::json!(out.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()).to_string())
}

#[tauri::command(async)]
fn lua_check(openmw: Option<String>, state: State) -> Result<String, String> {
    use std::collections::BTreeMap;
    let LuaAll { vfs, world, reg_files, reg: _, content, all, sources, builtins, resources } = lua_all(openmw, &state)?;
    let jit = resources.as_deref().and_then(viewcore::luarun::find_luajit);
    // The mods: a content file's scripts together (its first content file), as the by-file mode.
    let mut mods: BTreeMap<String, Vec<(String, Vec<String>)>> = BTreeMap::new();
    for (p, f, files) in &reg_files {
        let k = files.first().cloned().unwrap_or_else(|| p.clone());
        mods.entry(k).or_default().push((p.clone(), f.clone()));
    }
    let scene = serde_json::json!({"player": {"pos": [100, 200, 0], "cell": {"name": "Balmora", "ext": false}},
        "objects": {"k1": {"id": "wg_check_box", "tag": "CONT", "pos": [0, 0, 0]}, "k2": {"id": "wg_check_npc", "tag": "NPC_", "pos": [300, 0, 0]}}});
    // (name, kind, scripts, the start message)
    let mut jobs: Vec<(String, String, Vec<String>, String)> = Vec::new();
    for (name, scripts) in &mods {
        let mut start = all.clone();
        let mut list = builtins.clone();
        list.extend(scripts.iter().map(|(p, f)| serde_json::json!({"path": p, "flags": f.iter().filter(|x| *x != "LOAD").collect::<Vec<_>>()})));
        start["scripts"] = serde_json::Value::Array(list);
        start["mwse"] = serde_json::json!([]);
        start["state"] = scene.clone();
        jobs.push((name.clone(), "openmw".into(), scripts.iter().map(|(p, _)| p.clone()).collect(), start.to_string()));
    }
    for main in all["mwse"].as_array().cloned().unwrap_or_default() {
        let Some(main) = main.as_str() else { continue };
        let mut start = all.clone();
        start["scripts"] = serde_json::json!([]);
        start["mwse"] = serde_json::json!([main]);
        start["state"] = scene.clone();
        let name = main.trim_end_matches("/main.lua").to_string();
        jobs.push((name, "mwse".into(), vec![main.to_string()], start.to_string()));
    }
    let records = std::sync::Arc::new(std::sync::Mutex::new(world.clone().map(viewcore::luarec::Records::of_world)));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let runs: std::sync::Mutex<Vec<(usize, viewcore::luacheck::ModRun)>> = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..3 {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some((name, kind, scripts, start)) = jobs.get(i) else { break };
                let mut run = viewcore::luacheck::ModRun { name: name.clone(), kind: kind.clone(), scripts: scripts.clone(), ..Default::default() };
                let collect = |run: &mut viewcore::luacheck::ModRun, a: &str| {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(a) {
                        for l in v["log"].as_array().into_iter().flatten() {
                            let g = |k: &str| l[k].as_str().unwrap_or("").to_string();
                            if matches!(g("kind").as_str(), "error" | "skip") {
                                run.log.push((g("path"), g("kind"), g("text")));
                            }
                        }
                    }
                };
                match lua_worker(jit.as_deref()) {
                    Ok(mut p) => {
                        let recs = records.clone();
                        let files = vfs.clone();
                        p.records = Some(Box::new(move |kinds: &str, id: &str| {
                            if kinds.starts_with('@') {
                                return viewcore::luafiles::answer(&files, kinds, id);
                            }
                            let kinds: Vec<&str> = kinds.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
                            recs.lock().ok()?.as_mut()?.lookup(&kinds, id)
                        }));
                        match p.call("__wg_start", start, std::time::Duration::from_secs(30)) {
                            Ok(a) => {
                                collect(&mut run, &a);
                                for _ in 0..3 {
                                    match p.call("__wg_step", r#"{"dt":0.2}"#, std::time::Duration::from_secs(15)) {
                                        Ok(a) => collect(&mut run, &a),
                                        Err(e) => {
                                            run.log.push((String::new(), "error".into(), e.trim_start_matches(viewcore::luaproc::STOPPED).trim().to_string()));
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => run.failed = Some(e.trim_start_matches(viewcore::luaproc::STOPPED).trim().to_string()),
                        }
                    }
                    Err(e) => run.failed = Some(e),
                }
                if let Ok(mut g) = runs.lock() {
                    g.push((i, run));
                }
            });
        }
    });
    let mut runs = runs.into_inner().map_err(|e| e.to_string())?;
    runs.sort_by_key(|(i, _)| *i);
    let runs: Vec<viewcore::luacheck::ModRun> = runs.into_iter().map(|(_, r)| r).collect();
    let line = |p: &str, n: usize| sources.get(&p.to_ascii_lowercase()).and_then(|t| t.lines().nth(n.saturating_sub(1))).map(str::to_string);
    let mut report = viewcore::luacheck::report(&runs, &line);
    // The content files each data folder holds a copy of.
    let copies: Vec<(String, Vec<String>)> = content
        .iter()
        .filter(|c| c.to_ascii_lowercase().ends_with(".omwscripts"))
        .map(|c| (c.clone(), vfs.providers(c, &[]).into_iter().map(|(label, _)| label).collect::<Vec<_>>()))
        .collect();
    report["lint"] = viewcore::luacheck::lint(&sources, &copies);
    report["resources"] = serde_json::json!(resources.map(|p| p.display().to_string()));
    Ok(report.to_string())
}

/// Wraithguard: OpenMW's Lua storage file `which` (`player` / `global`): beside openmw.cfg.
fn lua_storage_path(which: &str, state: &State) -> Result<std::path::PathBuf, String> {
    let order = state.app().order_file.clone();
    let cfg = order.as_deref().filter(|p| viewcore::luascan::is_openmw_cfg(p)).ok_or("Not an OpenMW setup: there is no Lua storage to read.")?;
    let name = match which {
        "player" => "player_storage.bin",
        "global" => "global_storage.bin",
        _ => return Err(format!("no such storage: {which}")),
    };
    Ok(cfg.parent().ok_or("openmw.cfg has no folder")?.join(name))
}

/// Wraithguard: the settings OpenMW keeps for Lua mods (Tools → Lua settings; Static
/// 2026-11-23): `{player: {path, sections}, global: {path, sections}}`, each file's sections as
/// `luafiles::storage_json` reads them (null where there is no file).
#[tauri::command(async)]
fn lua_settings_read(state: State) -> Result<String, String> {
    let mut out = serde_json::Map::new();
    for which in ["player", "global"] {
        let p = lua_storage_path(which, &state)?;
        let sections = std::fs::read(&p).ok().and_then(|b| viewcore::luafiles::storage_json(&b));
        out.insert(which.into(), serde_json::json!({"path": p.display().to_string(), "sections": sections, "backup": p.with_extension("bin.wg-backup").exists()}));
    }
    Ok(serde_json::Value::Object(out).to_string())
}

/// Wraithguard: one Lua setting changed in OpenMW's storage file (`value` null: taken out). The
/// file written only when it reads back as exactly the settings meant (`luafiles::storage_set`);
/// the original kept once as `<file>.wg-backup` before the first change; written to a new file
/// and moved over the old, so a failure leaves the old one whole.
#[tauri::command(async)]
fn lua_settings_write(which: String, section: String, key: String, value: serde_json::Value, state: State) -> Result<String, String> {
    let p = lua_storage_path(&which, &state)?;
    let old = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let new = viewcore::luafiles::storage_set(&old, &section, &key, value)?;
    let backup = p.with_extension("bin.wg-backup");
    if !backup.exists() {
        std::fs::write(&backup, &old).map_err(|e| format!("the backup {}: {e}", backup.display()))?;
    }
    let tmp = p.with_extension("bin.wg-new");
    std::fs::write(&tmp, &new).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", p.display())
    })?;
    Ok(serde_json::json!({"path": p.display().to_string(), "backup": backup.display().to_string()}).to_string())
}

/// Wraithguard: the record index `lua_record` keeps for the world it was made for.
type LuaRecordIndex = Option<(usize, viewcore::luarec::Records)>;
static LUA_RECORDS: std::sync::Mutex<LuaRecordIndex> = std::sync::Mutex::new(None);

/// Wraithguard: a record as OpenMW's Lua gives it to a script (`types.X.records[id]`,
/// `viewcore::luarec`) - the record dialog's "As Lua sees it" (Static 2026-11-23). Null for a
/// kind the scripts have no records of. The index is kept while the same world is loaded.
#[tauri::command(async)]
fn lua_record(tag: String, id: String, state: State) -> Result<String, String> {
    let w = state.app().world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let key = std::sync::Arc::as_ptr(&w) as usize;
    let mut g = LUA_RECORDS.lock().map_err(|e| e.to_string())?;
    if g.as_ref().map(|(k, _)| *k) != Some(key) {
        *g = Some((key, viewcore::luarec::Records::of_world(w)));
    }
    let r = g.as_mut().and_then(|(_, r)| r.lookup(&[tag.as_str()], &id));
    Ok(r.unwrap_or_else(|| "null".into()))
}

/// Wraithguard: the OpenMW resources folder the Preview's Lua runs on (its built-ins and fonts).
static LUA_RESOURCES: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Wraithguard: OpenMW's own font for the windows Lua scripts make (`ui.create`): its
/// resources' MysticCards, as the game draws them. Empty when no resources are known.
#[tauri::command(async)]
fn lua_ui_font() -> Result<tauri::ipc::Response, String> {
    let r = LUA_RESOURCES.lock().map_err(|e| e.to_string())?.clone();
    let bytes = r.and_then(|r| std::fs::read(r.join("vfs").join("fonts").join("MysticCards.ttf")).ok()).unwrap_or_default();
    Ok(tauri::ipc::Response::new(bytes))
}

/// Wraithguard: one frame of the Preview's OpenMW Lua: what changed in the scene in (JSON,
/// `luarun.lua`'s `__wg_step`), `{effects, log, status}` out.
#[tauri::command(async)]
fn lua_run_step(frame: String, state: State) -> Result<String, String> {
    let delta = lua_collision(&frame, &state);
    let mut g = LUA_RUN.lock().map_err(|e| e.to_string())?;
    let run = g.as_mut().ok_or("OpenMW Lua is not running")?;
    // The worker's copy of the collision, brought up to date before the frame (its rays are its own).
    if let Some(d) = delta {
        run.send_col(&d);
    }
    let got = run.call("__wg_step", &frame, LUA_STEP_LIMIT);
    // A worker that was stopped is let go: the next start makes a new one.
    if !run.alive() {
        *g = None;
    }
    got.map_err(|e| e.replacen(viewcore::luaproc::STOPPED, "", 1).trim().to_string())
}

/// Wraithguard: each mesh's collision triangles, read once while the Lua runs (`None`: the
/// mesh has no collision, or could not be read).
static LUA_SHAPES: std::sync::Mutex<Option<std::collections::HashMap<String, Option<std::sync::Arc<Vec<[f32; 3]>>>>>> = std::sync::Mutex::new(None);

/// Wraithguard: a mesh's collision triangles in its own units (three vertices each), read
/// once: `None` for a mesh with no collision (or one that cannot be read). The collision
/// the first-person body walks on, `nearby.castRay` casts against and the Physics drop
/// falls onto.
fn collision_tris(vfs: &Vfs, model: &str) -> Option<std::sync::Arc<Vec<[f32; 3]>>> {
    let mut cache = LUA_SHAPES.lock().ok()?;
    let cache = cache.get_or_insert_with(Default::default);
    cache
        .entry(model.to_ascii_lowercase())
        .or_insert_with(|| {
            let b = collision_payload(vfs, model).ok()?;
            // GDCL, the shape (0 hull, 1 the visible mesh, 2 none), the count, the floats.
            if b.len() < 9 || b[4] == 2 {
                return None;
            }
            let n = u32::from_le_bytes([b[5], b[6], b[7], b[8]]) as usize;
            let f: Vec<f32> = b[9..].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
            Some(std::sync::Arc::new(f.as_chunks::<3>().0.iter().take(n * 3).copied().collect()))
        })
        .clone()
}

/// Wraithguard: the collision `nearby.castRay` casts against, kept to the frame the page
/// sent: each object with a `model` and the renderer's matrix `m` placed (plus the scene's
/// `origin`), one sent as `false` taken out, `enabled` followed; the land counted while
/// the player is outside.
/// Static 2026-11-23: also what changed, for the Lua worker's own copy (`luarun::Collider::apply`'s
/// form): the models' triangles once each (`LUA_SENT`), the objects' placings, the land of the
/// squares around the player. None when nothing did.
fn lua_collision(frame: &str, state: &State) -> Option<serde_json::Value> {
    use serde_json::{json, Map, Value};
    let v = serde_json::from_str::<Value>(frame).ok()?;
    let mut col = viewcore::luarun::COLLIDER.lock().ok()?;
    let col = col.as_mut()?;
    let mut sent = LUA_SENT.lock().ok()?;
    let mut out = Map::new();
    if let Some(ext) = v.pointer("/player/cell/ext").and_then(Value::as_bool) {
        col.exterior = ext;
        // The sea outside; an interior's water when the page says it has some.
        col.water = if ext { Some(0.0) } else { v.pointer("/player/cell/water").and_then(Value::as_f64) };
        out.insert("exterior".into(), json!(ext));
        out.insert("water".into(), json!(col.water));
        // The land around the player, each square once.
        if let (true, Some(gx), Some(gy), Some(w)) = (ext, v.pointer("/player/cell/gx").and_then(Value::as_i64), v.pointer("/player/cell/gy").and_then(Value::as_i64), col.world.clone()) {
            let mut lands = Vec::new();
            for dx in -2..=2 {
                for dy in -2..=2 {
                    let g = (gx as i32 + dx, gy as i32 + dy);
                    if sent.lands.insert(g) {
                        if let Some(l) = w.lands.get(&g) {
                            lands.push(json!([g.0, g.1, viewcore::luarun::f32s_b64(&l.heights[..])]));
                        }
                    }
                }
            }
            if !lands.is_empty() {
                out.insert("lands".into(), Value::Array(lands));
            }
        }
    }
    let mut models = Map::new();
    let mut placed = Map::new();
    let finish = |out: &mut Map<String, Value>, models: Map<String, Value>, placed: Map<String, Value>| {
        if !models.is_empty() {
            out.insert("models".into(), Value::Object(models));
        }
        if !placed.is_empty() {
            out.insert("objects".into(), Value::Object(placed));
        }
    };
    let Some(objs) = v.get("objects").and_then(Value::as_object) else {
        finish(&mut out, models, placed);
        return (!out.is_empty()).then_some(Value::Object(out));
    };
    let origin = v.get("origin").and_then(serde_json::Value::as_array).map(|o| [o.first().and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32, o.get(1).and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32]).unwrap_or([0.0, 0.0]);
    let vfs = state.app().vfs.clone();
    let Some(vfs) = vfs else {
        finish(&mut out, models, placed);
        return (!out.is_empty()).then_some(Value::Object(out));
    };
    for (k, o) in objs {
        if o.is_boolean() {
            col.shapes.remove(k);
            placed.insert(k.clone(), json!(false));
            continue;
        }
        let enabled = o.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let (Some(model), Some(m)) = (o.get("model").and_then(Value::as_str), o.get("m").and_then(Value::as_array)) else {
            if let Some(sh) = col.shapes.get_mut(k) {
                sh.enabled = enabled;
            }
            placed.insert(k.clone(), json!({"enabled": enabled}));
            continue;
        };
        let mut mm = [0f32; 12];
        for (i, x) in m.iter().take(12).enumerate() {
            mm[i] = x.as_f64().unwrap_or(0.0) as f32;
        }
        match collision_tris(&vfs, model) {
            Some(t) => {
                let mut sh = viewcore::luarun::Shape::placed(&t, &mm, origin, enabled);
                sh.kind = viewcore::luarun::collision_type::of_tag(o.get("tag").and_then(Value::as_str).unwrap_or(""));
                if sent.models.insert(model.to_ascii_lowercase()) {
                    let flat: Vec<f32> = t.iter().flatten().copied().collect();
                    models.insert(model.to_ascii_lowercase(), json!(viewcore::luarun::f32s_b64(&flat)));
                }
                placed.insert(k.clone(), json!({"model": model.to_ascii_lowercase(), "m": mm, "origin": origin, "enabled": enabled, "kind": sh.kind}));
                col.shapes.insert(k.clone(), sh);
            }
            None => {
                col.shapes.remove(k);
                placed.insert(k.clone(), json!(false));
            }
        }
    }
    finish(&mut out, models, placed);
    (!out.is_empty()).then_some(Value::Object(out))
}

/// What the current Lua worker has been sent of the collision (`lua_collision`): each model's
/// triangles and each land square once. Emptied when a worker starts.
#[derive(Default)]
struct LuaSent {
    models: std::collections::HashSet<String>,
    lands: std::collections::HashSet<(i32, i32)>,
}
static LUA_SENT: std::sync::LazyLock<std::sync::Mutex<LuaSent>> = std::sync::LazyLock::new(Default::default);

/// Wraithguard: `phys_drop`'s request, read from bytes (`WGTB`, `WgPhysics.pack` in
/// 76_wg_physics.js; Static 2026-11-17: JSON only where it is needed - a body's triangles and
/// the ground's 65 x 65 heights per square were JSON text):
///
/// - `settings` `dddduuudd`: gravity, friction, bounce, seconds, drop only, hollow, exterior,
///   the scene's origin x y
/// - `bodies` `ssFF`: key, model, its matrix (12), its drawn triangles (nine numbers each)
/// - `statics` `sF`: model, matrix
/// - `land` `iiuF`: gx, gy, the page has its ground 0/1 (0: none, the square is bare), the
///   65 x 65 heights
struct PhysRequest {
    gravity: f32,
    friction: f32,
    bounce: f32,
    seconds: f32,
    drop_only: bool,
    hollow: bool,
    exterior: bool,
    origin: [f32; 2],
    bodies: Vec<(String, String, [f32; 12], Vec<f32>)>,
    statics: Vec<(String, [f32; 12])>,
    land: Vec<(i32, i32, Option<Vec<f32>>)>,
    /// The kinetic event (none: a plain drop): a blast at a point, the bodies' starting speed
    /// (a throw), and whether the objects around move too.
    blast: Option<([f32; 3], f32, f32)>,
    throw: [f32; 3],
    /// The objects around that move when struck (`key`, `id`, `model`, matrix), unless the
    /// blacklist holds them; those it holds are fixed, as statics are.
    near: Vec<(String, String, String, [f32; 12])>,
    blacklist: viewcore::physcore::Blacklist,
}

impl PhysRequest {
    fn read(b: &[u8]) -> Result<PhysRequest, String> {
        use viewcore::wgtb::{read, rows};
        let t = read(b).map_err(|e| format!("the drop's request: {e}"))?;
        // Each table with the columns this reads, or none: a different layout is an error,
        // not a guess.
        for (name, cols) in [("settings", "dddduuudd"), ("bodies", "ssFF"), ("statics", "sF"), ("land", "iiuF"), ("kinetic", "udddddddd"), ("near", "sssF"), ("blacklist", "ss")] {
            if let Some(x) = t.iter().find(|x| x.name == name).filter(|x| x.cols != cols) {
                return Err(format!("the drop's request: {name} has columns {}, not {cols}", x.cols));
            }
        }
        let mat = |v: &viewcore::wgtb::Val| {
            let mut m = [0f32; 12];
            for (i, x) in v.floats().iter().take(12).enumerate() {
                m[i] = *x;
            }
            m
        };
        let s = rows(&t, "settings").first().ok_or("the drop's request has no settings")?;
        let n = |i: usize| s.get(i).map_or(0.0, |v| v.num());
        Ok(PhysRequest {
            gravity: n(0) as f32,
            friction: n(1) as f32,
            bounce: n(2) as f32,
            seconds: n(3) as f32,
            drop_only: n(4) != 0.0,
            hollow: n(5) != 0.0,
            exterior: n(6) != 0.0,
            origin: [n(7) as f32, n(8) as f32],
            bodies: rows(&t, "bodies").iter().map(|r| (r[0].str().to_string(), r[1].str().to_string(), mat(&r[2]), r[3].floats().to_vec())).collect(),
            statics: rows(&t, "statics").iter().map(|r| (r[0].str().to_string(), mat(&r[1]))).collect(),
            land: rows(&t, "land")
                .iter()
                .map(|r| (r[0].num() as i32, r[1].num() as i32, (r[2].num() != 0.0).then(|| r[3].floats().to_vec())))
                .collect(),
            blast: rows(&t, "kinetic").first().filter(|k| k[0].num() != 0.0).map(|k| {
                ([k[1].num() as f32, k[2].num() as f32, k[3].num() as f32], k[4].num() as f32, k[5].num() as f32)
            }),
            throw: rows(&t, "kinetic").first().map_or([0.0; 3], |k| [k[6].num() as f32, k[7].num() as f32, k[8].num() as f32]),
            near: rows(&t, "near").iter().map(|r| (r[0].str().to_string(), r[1].str().to_string(), r[2].str().to_string(), mat(&r[3]))).collect(),
            blacklist: {
                let mut b = viewcore::physcore::Blacklist::default();
                for r in rows(&t, "blacklist") {
                    let v = r[1].str().trim().to_string();
                    match r[0].str() {
                        "pattern" if !v.is_empty() => b.patterns.push(v),
                        "tag" if !v.is_empty() => b.tags.push(v.to_uppercase()),
                        "size" => b.max_size = v.parse().unwrap_or(0.0),
                        _ => {}
                    }
                }
                b
            },
        })
    }
}

/// Wraithguard: the Editor's Physics drop (`physcore`): the `bodies` fall onto the
/// `statics` and, outside, the land, until they rest or `seconds` runs out. The request
/// is bytes (`PhysRequest`, read from `WgPhysics.pack`): the bodies, statics, origin,
/// exterior, land and settings, each `m` the renderer's matrix (3x4, rows) in the scene's frame. A body's
/// `tris` (its drawn triangles, nine numbers each) stand in for a mesh with no collision;
/// `land` is the ground as the page has it (the Land view's strokes not yet kept), a
/// square's 65 x 65 heights, `null` for none - the load order's for squares not given. Answers `{frames: [[[x, y, z, rx,
/// ry, rz]]], seconds, settled, skipped}`, positions in the world's units - as bytes (`WGTB`,
/// Static 2026-11-17: JSON only where it is needed; every frame of every body): `head` `duu`
/// (seconds, settled 0/1, frame count), `poses` `udddddd` (its frame, x y z, rx ry
/// rz; a frame's bodies in order) and `skipped` `s`. Read by `WgPhysics.readDrop`.
///
/// Kinetic events (2026-11-18): `kinetic` `udddddddd` (a blast 0/1, its point x y z in the
/// world, its reach and strength; the bodies' starting velocity x y z, a throw), `near` `sssF`
/// (key, id, model, matrix: the objects around that move when struck) and `blacklist` `ss`
/// (`pattern`, `tag` or `size`, and its value; `physcore::Blacklist`). A near object the
/// blacklist holds, by its mesh, its record's type (`record_where`) or its collision's widest
/// side, or one with no collision, is ground; the rest are bodies after the ones asked for.
/// The answer adds `keys` `s` (every body's key, in the poses' order) and `fixed` `ss` (a near
/// object held, and why).
#[tauri::command(async)]
fn phys_drop(request: tauri::ipc::Request<'_>, state: State) -> Result<tauri::ipc::Response, String> {
    use viewcore::physcore::{self as pc, glam_mat3_from_rows};
    let req = PhysRequest::read(raw_body(&request)?)?;
    let (vfs, world) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.world.clone())
    };
    let origin = req.origin;
    let settings = pc::Settings {
        gravity: req.gravity,
        friction: req.friction.clamp(0.0, 2.0),
        restitution: req.bounce.clamp(0.0, 1.0),
        max_seconds: req.seconds.clamp(0.1, 30.0),
        drop_only: req.drop_only,
        fps: 30.0,
        hollow: req.hollow,
        blast: req.blast.map(|(at, radius, strength)| pc::Blast { at, radius, strength: strength.clamp(0.0, 20000.0) }),
    };
    // The bodies: their collision's points, where they stand in the world, turned and scaled.
    let mut bodies = Vec::new();
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (key, model, m, tris) in &req.bodies {
        let (key, model, m) = (key.as_str(), model.as_str(), *m);
        let scale = (m[0] * m[0] + m[4] * m[4] + m[8] * m[8]).sqrt().max(1e-6);
        let rot = pc::angles(glam_mat3_from_rows([[m[0] / scale, m[1] / scale, m[2] / scale], [m[4] / scale, m[5] / scale, m[6] / scale], [m[8] / scale, m[9] / scale, m[10] / scale]]));
        let pos = [m[3] + origin[0], m[7] + origin[1], m[11]];
        // Its collision; for a mesh with none (marked so, or nothing in it collides), what is drawn.
        let points = collision_tris(&vfs, model).map(|t| t.to_vec()).unwrap_or_else(|| tris.as_chunks::<3>().0.to_vec());
        for k in 0..2 {
            lo[k] = lo[k].min(pos[k]);
            hi[k] = hi[k].max(pos[k]);
        }
        bodies.push(pc::Body { key: key.to_string(), points, pos, rot, scale, vel: req.throw });
    }
    // The objects around that join in, unless the blacklist holds them (`fixed`: why); held,
    // or with no collision to move by, they stay as ground.
    let mut fixed: Vec<(String, String)> = Vec::new();
    let mut held: Vec<(String, [f32; 12])> = Vec::new();
    for (key, id, model, m) in &req.near {
        let Some(t) = collision_tris(&vfs, model) else {
            held.push((model.clone(), *m));
            fixed.push((key.clone(), "it has no collision to move by".into()));
            continue;
        };
        let scale = (m[0] * m[0] + m[4] * m[4] + m[8] * m[8]).sqrt().max(1e-6);
        let (lo3, hi3) = t.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(mut l, mut h), p| {
            for k in 0..3 {
                l[k] = l[k].min(p[k]);
                h[k] = h[k].max(p[k]);
            }
            (l, h)
        });
        let size = (0..3).map(|k| hi3[k] - lo3[k]).fold(0.0f32, f32::max) * scale;
        let tag = world
            .as_ref()
            .and_then(|w| w.record_where.get(&id.to_ascii_lowercase()).and_then(|d| d.last().map(|(_, t)| tag_text(t))))
            .unwrap_or_default();
        if let Some(why) = req.blacklist.holds(model, &tag, size) {
            held.push((model.clone(), *m));
            fixed.push((key.clone(), why));
            continue;
        }
        let rot = pc::angles(glam_mat3_from_rows([[m[0] / scale, m[1] / scale, m[2] / scale], [m[4] / scale, m[5] / scale, m[6] / scale], [m[8] / scale, m[9] / scale, m[10] / scale]]));
        let pos = [m[3] + origin[0], m[7] + origin[1], m[11]];
        for k in 0..2 {
            lo[k] = lo[k].min(pos[k]);
            hi[k] = hi[k].max(pos[k]);
        }
        bodies.push(pc::Body { key: key.clone(), points: t.to_vec(), pos, rot, scale, vel: [0.0; 3] });
    }
    if bodies.is_empty() {
        return Err(if req.near.is_empty() {
            "Nothing to drop: select objects in the render window".into()
        } else {
            format!("Nothing can move: the {} object(s) in reach are all held by the blacklist or have no collision", req.near.len())
        });
    }
    // What they fall onto: the other objects' collision, placed, and the land around them.
    let mut statics = Vec::new();
    for (model, m) in req.statics.iter().chain(&held) {
        if let Some(t) = collision_tris(&vfs, model) {
            statics.push(pc::Static { tris: viewcore::luarun::Shape::placed(&t, m, origin, true).tris });
        }
    }
    if req.exterior {
        // The page's ground first: the Land view's strokes are in it before they are kept.
        let given: std::collections::HashMap<(i32, i32), Option<Vec<f32>>> =
            req.land.iter().map(|(gx, gy, h)| ((*gx, *gy), h.clone().filter(|h| h.len() == 65 * 65))).collect();
        let reach = 8192.0;
        let cell = |v: f32| (v / 8192.0).floor() as i32;
        for gx in cell(lo[0] - reach)..=cell(hi[0] + reach) {
            for gy in cell(lo[1] - reach)..=cell(hi[1] + reach) {
                let tris = match given.get(&(gx, gy)) {
                    Some(Some(h)) => pc::land_tris((gx, gy), h),
                    Some(None) => pc::flat_square((gx, gy)),
                    None => match world.as_ref().and_then(|w| w.lands.get(&(gx, gy))) {
                        Some(l) => pc::land_tris((gx, gy), &l.heights[..]),
                        None => pc::flat_square((gx, gy)),
                    },
                };
                statics.push(pc::Static { tris });
            }
        }
    }
    let out = pc::drop(&statics, &bodies, &settings);
    let mut tb = Tables::new();
    tb.table("head", "duu", 1);
    tb.d(out.seconds as f64).u(out.settled as u64).u(out.frames.len() as u64);
    tb.table("poses", "udddddd", out.frames.iter().map(|fr| fr.len()).sum());
    for (i, fr) in out.frames.iter().enumerate() {
        for (p, r) in fr {
            // Each f32 widened, as serde_json wrote it.
            tb.u(i as u64);
            for v in p.iter().chain(r.iter()) {
                tb.d(*v as f64);
            }
        }
    }
    tb.table("skipped", "s", out.skipped.len());
    for k in &out.skipped {
        tb.s(k);
    }
    // The bodies in the order of each frame's poses (the ones asked for, then the objects
    // around that joined in), and the objects around the blacklist held, with why.
    tb.table("keys", "s", bodies.len());
    for b in &bodies {
        tb.s(&b.key);
    }
    tb.table("fixed", "ss", fixed.len());
    for (k, why) in &fixed {
        tb.s(k).s(why);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: the Preview's OpenMW Lua stopped and its state dropped.
#[tauri::command(async)]
fn lua_run_stop() -> Result<String, String> {
    *LUA_RUN.lock().map_err(|e| e.to_string())? = None;
    if let Ok(mut c) = viewcore::luarun::COLLIDER.lock() {
        *c = None;
    }
    if let Ok(mut c) = LUA_SHAPES.lock() {
        *c = None;
    }
    Ok("{}".into())
}

/// Wraithguard: the game state an OpenMW save holds, for the Dialogue window's In game tab
/// (`toolscore::save::game_state`): `{name, sex, race, class, level, factions: [[id, rank]],
/// globals: [{name, kind, value}], journal: [[quest, index]]}`.
/* Wraithguard: as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed): `save` `ssssi`
   (name, sex, race, class, level), `factions` `si`, `globals` `ssd` (name, kind, value),
   `journal` `si` and `inventory` `si`. Read by `WgScripts.readSave` (73_wg_scripts.js). */
#[tauri::command(async)]
fn dialogue_state_from_save(path: String) -> Result<tauri::ipc::Response, String> {
    let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let g = toolscore::save::game_state(&data)?;
    let mut tb = Tables::new();
    tb.table("save", "ssssi", 1);
    tb.s(&g.name).s(&g.sex).s(&g.race).s(&g.class).i(g.level as i64);
    for (name, list) in [("factions", &g.factions), ("journal", &g.journal), ("inventory", &g.inventory)] {
        tb.table(name, "si", list.len());
        for (k, v) in list {
            tb.s(k).i(*v as i64);
        }
    }
    tb.table("globals", "ssd", g.globals.len());
    for (n, k, v) in &g.globals {
        tb.s(n).s(&k.to_string()).d(*v as f64);
    }
    Ok(tauri::ipc::Response::new(tb.done()))
}

/// Wraithguard: the player's stats from an OpenMW save, for the first-person body
/// (`toolscore::save::player_stats`): `{name, skills: [27], attributes: [8], fatigue: [cur, max]}`.
#[tauri::command(async)]
fn walk_player_from_save(path: String) -> Result<String, String> {
    let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let p = toolscore::save::player_stats(&data)?;
    Ok(serde_json::json!({"name": p.name, "skills": p.skills, "attributes": p.attributes, "fatigue": [p.fatigue.0, p.fatigue.1]}).to_string())
}

/* Wraithguard: requests whose body is raw bytes (the page's `Engine.send`, Static 2026-11-17:
   JSON only where it is needed), and the headers that say what the bytes are for -
   percent-encoded by the page, so a path in any script survives a header. */

/// A raw request's bytes; a JSON one is an error naming the way to call it.
fn raw_body<'r>(request: &'r tauri::ipc::Request<'_>) -> Result<&'r [u8], String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(b) => Ok(b),
        _ => Err("this command takes bytes (Engine.send), not JSON".into()),
    }
}

/// A raw request's header, percent-decoded (`encodeURIComponent` on the page).
fn header(request: &tauri::ipc::Request<'_>, name: &str) -> Option<String> {
    let v = request.headers().get(name)?.to_str().ok()?;
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).copied().and_then(hex), b.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Wraithguard: writes the request's bytes (a screenshot for the report bundle) to the path
/// in its `x-path` header. It was `write_b64`, the bytes as base64 inside JSON.
#[tauri::command(async)]
fn write_bytes(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let path = header(&request, "x-path").filter(|p| !p.is_empty()).ok_or("no path to write to (x-path)")?;
    write_path_whole(&path, raw_body(&request)?)
}

/* ---- the viewer profile (Wraithguard) ---------------------------------------------

   The cell viewer has one profile: Wraithguard's. The setup itself (data paths, load
   order) always comes from Wraithguard as the openmw.cfg it writes for each launch;
   this file keeps only the viewer's own preferences (lighting, sky, effects, camera),
   in the profile TOML the preview settings already speak. Wraithguard names the file
   with `--prefs`; without it, it sits beside the program. */

fn viewer_prefs_path() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WRAITHGUARD_VIEWER_PREFS") {
        return Ok(PathBuf::from(p));
    }
    Ok(app_dir()?.join("wraithguard_cellviewer.profile.json"))
}

/// The viewer profile's text; a fresh one when there is no file yet.
#[tauri::command(async)]
fn viewer_profile_read() -> Result<String, String> {
    let p = viewer_prefs_path()?;
    match std::fs::read_to_string(&p) {
        Ok(t) => Ok(t),
        // A new profile: the defaults, with ambient occlusion off (as Gardenfell's starter
        // profile had it - a missing `ssao` would hand the switch to the install's chain).
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(r#"{"viewport":[{"key":"ssao","value":false}]}"#.to_string())
        }
        Err(e) => Err(format!("{}: {}", p.display(), e)),
    }
}

/// Writes the viewer profile (through a temporary file, so a crash never leaves half of one).
/// Answers `{"path": ...}`: the page reads every answer but `TEXT`'s as JSON, and a bare
/// Windows path ("C:\\Users\\...") is not JSON, so every save said "profile not saved"
/// although the file was written (Static's report, 2026-10-19).
#[tauri::command(async)]
fn viewer_profile_write(text: String) -> Result<String, String> {
    let p = viewer_prefs_path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {}", p.display(), e))?;
    Ok(J::obj().str("path", &p.display().to_string()).done())
}

/// Where the viewer's window was when it last settled (Gardenfell 18el/18fi, brought in
/// 2026-10-12): the outer rectangle in physical pixels, and whether it was maximised.
pub struct WindowBox {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub max: bool,
}

/// Beside the viewer profile: the window is a fact about this machine, not a setting.
fn window_box_path() -> Result<PathBuf, String> {
    Ok(viewer_prefs_path()?.with_file_name("wraithguard_cellviewer.window.json"))
}

/// The remembered window, when there is one and it is a window at all. Read by the shell
/// before the window is shown (`cellviewer::restore_window`); check-commands, which has no
/// window, never asks.
#[allow(dead_code)]
pub(crate) fn window_box() -> Option<WindowBox> {
    window_box_of(&std::fs::read_to_string(window_box_path().ok()?).ok()?)
}

/// [`window_box`] from the file's text: `None` for anything that is not a window.
fn window_box_of(text: &str) -> Option<WindowBox> {
    let v = viewcore::json::parse(text).ok()?;
    let i = |k: &str| v.get(k).and_then(|x| x.as_i64());
    let b = WindowBox {
        x: i32::try_from(i("x")?).ok()?,
        y: i32::try_from(i("y")?).ok()?,
        w: u32::try_from(i("w")?).ok()?,
        h: u32::try_from(i("h")?).ok()?,
        max: v.get("max").and_then(|x| x.as_bool()).unwrap_or(false),
    };
    (b.w >= 200 && b.h >= 150).then_some(b)
}

/// The file's text for a window.
fn window_box_text(b: &WindowBox) -> String {
    format!("{{\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"max\":{}}}", b.x, b.y, b.w, b.h, b.max)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod granted_tests {
    use super::*;

    /* One test, so the grants (which are the program's, not a test's) are not cleared
       under another test running beside it. */
    #[test]
    fn only_what_the_person_chose_is_written() {
        granted::clear();
        let root = std::env::temp_dir().join(format!("wg-granted-{}", std::process::id()));
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let s = |p: &std::path::Path| p.to_string_lossy().into_owned();

        // Nothing chosen: refused, with the path for the page to ask about.
        let report = root.join("report.md");
        let e = write_text(s(&report), "x".into()).unwrap_err();
        assert_eq!(e, format!("{}{}", granted::REFUSED, s(&report)));
        assert!(!report.exists());

        // Chosen: written; its picture beside it too, but nothing else of that name.
        granted::file(&report);
        write_text(s(&report), "x".into()).unwrap();
        assert!(granted::may_write(&root.join("report.png")));
        assert!(!granted::may_write(&root.join("report.bat")));
        assert!(!granted::may_write(&root.join("other.md")));

        // A folder: what is in it, not what `..` climbs out to.
        granted::folder(&out);
        assert!(granted::may_write(&out.join("a").join("b.nif")));
        assert!(!granted::may_write(&out.join("..").join("escape.txt")));
        assert!(granted::check(" ").is_err());

        // A plugin written over in place is one that is there; any other name is asked about.
        let esp = root.join("Mod.esp");
        std::fs::write(&esp, b"TES3").unwrap();
        assert!(granted::plugin_in_place(&s(&esp)).is_ok());
        assert!(granted::plugin_in_place(&s(&root.join("New.esp"))).is_err());
        assert!(granted::plugin_in_place(&s(&root.join("run.cmd"))).is_err());

        // Backups: only what is named as one is deleted.
        let bak = root.join("Mod.esp.preclean.bak");
        std::fs::write(&bak, b"x").unwrap();
        assert!(tools_backups_delete(vec![s(&esp), s(&bak)]).is_err());
        assert!(esp.exists() && bak.exists());
        assert_eq!(tools_backups_delete(vec![s(&bak)]), Ok(1));

        granted::clear();
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
// Items follow (the command); the module sits beside what it tests.
#[allow(clippy::items_after_test_module)]
mod window_box_tests {
    use super::*;

    #[test]
    fn a_window_comes_back_as_written() {
        let b = window_box_of(&window_box_text(&WindowBox { x: -1920, y: 40, w: 1600, h: 1000, max: true })).unwrap();
        assert_eq!((b.x, b.y, b.w, b.h, b.max), (-1920, 40, 1600, 1000, true));
    }

    #[test]
    fn what_is_not_a_window_is_refused() {
        assert!(window_box_of("").is_none());
        assert!(window_box_of("not json").is_none());
        assert!(window_box_of(r#"{"x":0,"y":0,"w":10,"h":10}"#).is_none(), "too small to be a window");
        assert!(window_box_of(r#"{"x":0,"y":0,"w":-5,"h":800}"#).is_none());
        assert!(window_box_of(r#"{"x":0,"y":0,"w":800,"h":600}"#).is_some_and(|b| !b.max), "max defaults to off");
    }
}

/// Remembers the window's outer rectangle and whether it is maximised, for the next start
/// (the page sends it when the window settles; while maximised, the last loose size).
#[tauri::command(async)]
fn window_set(x: i32, y: i32, w: u32, h: u32, max: bool) -> Result<String, String> {
    let p = window_box_path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
    }
    let tmp = p.with_extension("json.tmp");
    let text = window_box_text(&WindowBox { x, y, w, h, max });
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {}", p.display(), e))?;
    Ok("{\"saved\":true}".to_string())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod wgor_tests {
    use super::{pack_rows, ObjRow};

    /// `pack_rows` makes the bytes the page's `unpackRows` is tested on: one fixture,
    /// `ui/tests/editor/wgor_rows.json`, for both sides (found from either crate).
    #[test]
    fn the_rows_pack_as_the_page_reads_them() {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let file = loop {
            let f = dir.join("ui/tests/editor/wgor_rows.json");
            if f.is_file() {
                break f;
            }
            assert!(dir.pop(), "wgor_rows.json not found above the crate");
        };
        let text = std::fs::read_to_string(file).unwrap();
        let at = text.find("\"hex\": \"").expect("no hex in the fixture") + 8;
        let want = &text[at..at + text[at..].find('"').unwrap()];
        let row = |id: &str, name: &str, model: &str, plugins: Vec<usize>, placed: u32| ObjRow {
            key: id.to_ascii_lowercase(),
            id: id.into(),
            name: name.into(),
            model: model.into(),
            plugins,
            placed,
        };
        let rows = [row("Furn_Ä", "Bench \"old\"", "f\\b.nif", vec![0, 1], 7), row("x", "", "", vec![], 0)];
        let got: String = pack_rows(&["Morrowind.esm".into(), "Grass – Ä.esp".into()], &rows).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(got, want);
    }
}

#[cfg(test)]
mod wgtb_tests {
    use super::{pack_cell_refs, pack_cells, pack_tags, CellRefs, CellRow, RefRow};

    /// The cases `ui/tests/editor/wgtb_tables.json` holds, packed: the page's
    /// `unpackTables` is tested on the same file's bytes (found from either crate).
    fn cases() -> [(&'static str, Vec<u8>); 3] {
        let tags = pack_tags(&[("ACTI".into(), 12), ("NPC_".into(), 0)]);
        let cell = |x, y, name: &str, region: &str, refs, pathgrid| CellRow {
            x,
            y,
            name: name.into(),
            region: region.into(),
            refs,
            pathgrid,
        };
        let cells = pack_cells(
            &[cell(-3, 7, "Ald-ruhn", "West Gash Region", 40, true), cell(0, -12, "", "", 0, false)],
            &[cell(0, 0, "Vivec, Ä \"Arena\"", "", 5, false)],
        );
        let refs = pack_cell_refs(&CellRefs {
            name: "Seyda Neen".into(),
            ref_cell: "(-2, -9)".into(),
            plugins: vec!["Morrowind.esm".into(), "Grass – Ä.esp".into()],
            rows: vec![RefRow {
                key: "1:00000a".into(),
                id: "furn_ä \\".into(),
                tag: "STAT".into(),
                pos: [-12345.67, 0.04, 3.25],
                plugin: "Morrowind.esm".into(),
                owner: "fargoth (rank 2)".into(),
            }],
        });
        [("tags", tags), ("cells", cells), ("cellRefs", refs)]
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_lists_pack_as_the_page_reads_them() {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let file = loop {
            let f = dir.join("ui/tests/editor/wgtb_tables.json");
            if f.is_file() {
                break f;
            }
            assert!(dir.pop(), "wgtb_tables.json not found above the crate");
        };
        let text = std::fs::read_to_string(file).unwrap();
        for (name, bytes) in cases() {
            let at = text.find(&format!("\"{name}Hex\": \"")).unwrap_or_else(|| panic!("no {name}Hex in the fixture"));
            let at = at + name.len() + 8;
            let want = &text[at..at + text[at..].find('"').unwrap()];
            assert_eq!(hex(&bytes), want, "{name}");
        }
    }

    #[test]
    #[ignore]
    fn print_the_fixture_hex() {
        for (name, bytes) in cases() {
            println!("{name}Hex {}", hex(&bytes));
        }
    }
}

pub fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(App::default()))
        .invoke_handler(tauri::generate_handler![
            // Security F5: the file dialogs, opened from Rust (`pick.rs`).
            super::pick::path_pick,
            super::pick::path_allow,
            window_set,
            connect_stage,
            editor_tags_bytes,
            cells_bytes,
            editor_cell_refs_bytes,
            textures_bytes,
            scopes_bytes,
            texture_usage_bytes,
            assets_bytes,
            pathgrid,
            navmesh,
            collision_bundle,
            where_used,
            asset_providers,
            game_settings,
            walk_player_from_save,
            scene_scripts,
            script_texts,
            lua_scene,
            anim_groups,
            lua_run_start,
            lua_check,
            lua_deps,
            lua_settings_read,
            lua_settings_write,
            htn_deps,
            htn_read,
            htn_export,
            lua_record,
            lua_run_step,
            lua_run_stop,
            dialogue_state_from_save,
            phys_drop,
            lua_ui_font,
            anim_blend_config,
            write_bytes,
            editor_open,
            editor_link,
            editor_land_link,
            editor_setup,
            loadorder_compute,
            loadorder_export,
            loadorder_plain_sort,
            loadorder_plain_write,
            loadorder_plain_restore_dates,
            setup_runs,
            loadorder_rule_ages,
            settings_seed,
            settings_write,
            setup_state,
            setup_check,
            setup_find_cfgs,
            setup_mark,
            app_restart,
            loadorder_scan,
            loadorder_update_rules,
            loadorder_update_yml,
            loadorder_fetch_data_paths,
            loadorder_author,
            conflicts_scan,
            conflicts_fields,
            conflicts_resources,
            loadorder_drop_probe,
            meshopt_analyse,
            meshopt_write,
            meshopt_progress,
            meshcheck_scan,
            meshcheck_write,
            conflicts_texture,
            conflicts_report,
            conflicts_csv,
            conflicts_patch,
            conflicts_land_surfaces,
            editor_preview_patch,
            conflicts_by_plugin,
            conflicts_plugin_group,
            conflicts_verdicts,
            scan_cache_warm,
            conflicts_plugin_bulk,
            conflicts_plugin_bulk_apply,
            help_docs,
            help_doc,
            help_open_link,
            land_meta_read,
            land_meta_preview,
            land_meta_write,
            land_merge_home,
            land_merge_fork_config,
            land_merge,
            land_merge_progress,
            land_merge_cancel,
            tools_lint,
            tools_backups,
            tools_backups_restore,
            tools_backups_delete,
            tools_masters,
            tools_remove_master,
            tools_rename_master,
            tools_merge_master,
            tools_save_check,
            tools_journal,
            tools_tes3cmd_find,
            tools_locate,
            tools_tes3cmd,
            tools_tes3cmd_undo,
            tools_merge_objects,
            find_record,
            editor_records,
            editor_rows,
            editor_ref_names,
            editor_actors,
            lua_scan,
            lua_chart,
            editor_script_flowchart,
            editor_record_tag,
            ori_dialogue,
            mesh_collision,
            mesh_inspect,
            mesh_save,
            open_install,
            cell_data,
            interior_data,
            startup_install,
            theme_set,
            picker_set,
            world_map,
            world_map_tiles,
            distant_land,
            distant_objects,
            atmosphere,
            favourites_list,
            favourite_set,
            lang_list,
            lang_get,
            lang_create,
            lang_update,
            open_folder,
            write_text,
            cursor_park,
            read_asset,
            sound_info,
            sound_emitters,
            sound_records,
            sound_fallbacks,
            regions_list,
            music_list,
            region_sounds,
            sound_pcm,
            mesh_data,
            assets_bundle,
            texture_data,
            cpu_threads,
            plugins_lenient,
            viewer_profile_read,
            cell_coverage,
            ori,
            viewer_profile_write,
        ])
}
