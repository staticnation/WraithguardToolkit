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
#[tauri::command(async)]
fn world_map(state: State) -> Result<String, String> {
    let (w, _) = map_palette(&state)?;
    let cells = viewcore::worldmap::cells(&w);
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let mut list = String::from("[");
    for (i, (x, y)) in cells.iter().enumerate() {
        if i > 0 {
            list.push(',');
        }
        list.push_str(&format!("[{x},{y}]"));
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    list.push(']');
    let mut out = J::obj();
    out.int("tile", viewcore::worldmap::TILE as u64)
        // Gardenfell 18hp: the open sea's colour, so the page needs no literal of its own.
        .str("sea", &viewcore::worldmap::open_sea_css())
        .raw("cells", &list);
    if cells.is_empty() {
        out.raw("min", "null").raw("max", "null");
    } else {
        out.raw("min", &format!("[{x0},{y0}]")).raw("max", &format!("[{x1},{y1}]"));
    }
    Ok(out.done())
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

/// Every land texture the loaded world knows about.
///
/// The id and the file are both needed: an ini section names the LTEX id, but
/// what gets drawn is the filename in that record's DATA field, and the two are
/// rarely the same string. Sending only the id leaves the terrain untextured.
#[tauri::command(async)]
fn textures(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut out = String::from("[");
    for (i, (id, file)) in land_textures(w).iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        viewcore::json::escape_into(id, &mut out);
        out.push_str("\",\"file\":\"");
        viewcore::json::escape_into(file, &mut out);
        out.push_str("\"}");
    }
    out.push(']');
    Ok(out)
}

/// Wraithguard: `textures` as bytes (`WGTB`): `textures` (`ss`: id, file).
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

/// Named cells and regions, for the scope picker.
#[tauri::command(async)]
fn scopes(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let (cells, regions) = scope_names(w);
    let mut out = J::obj();
    out.raw("cells", &string_array(&cells))
        .raw("regions", &string_array(&regions));
    Ok(out.done())
}

/// Wraithguard: `scopes` as bytes (`WGTB`): `cells` (`s`) and `regions` (`s`).
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

/// Every exterior cell in the merged world.
///
/// The cell picker needs the whole grid, not just the named cells: an unnamed
/// wilderness cell is just as pickable, and its region still decides which selector
/// governs it.
///
/// The grid is exteriors, and there is nowhere in it for an interior to be: `World.cells`
/// is keyed by grid position and an interior has none. They come back beside it, by name,
/// in `interiors` — nothing places grass in one yet, but the picker has to be able to
/// *name* a place before anything can be done with it. See memory §18 260.
#[tauri::command(async)]
fn cells(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let (grid_rows, ints) = cell_lists(w);

    // [x, y, name, region] per cell, flat, because a few thousand small objects
    // is measurably slower to hand across than one array.
    let mut grid = String::from("[");
    for (i, c) in grid_rows.iter().enumerate() {
        if i > 0 {
            grid.push(',');
        }
        grid.push_str(&format!("[{},{},", c.x, c.y));
        grid.push('"');
        viewcore::json::escape_into(&c.name, &mut grid);
        grid.push_str("\",\"");
        viewcore::json::escape_into(&c.region, &mut grid);
        // Wraithguard: the Editor's Cell View columns - references, and whether it has a
        // pathgrid (appended, so readers of the first four are unchanged).
        grid.push_str(&format!("\",{},{}]", c.refs, u8::from(c.pathgrid)));
    }
    grid.push(']');

    // [name, references] per interior, sorted, so the picker's list is stable between
    // scans of the same install rather than in hash order.
    let mut inter = String::from("[");
    for (i, c) in ints.iter().enumerate() {
        if i > 0 {
            inter.push(',');
        }
        inter.push_str("[\"");
        viewcore::json::escape_into(&c.name, &mut inter);
        inter.push_str(&format!("\",{},{}]", c.refs, u8::from(c.pathgrid)));
    }
    inter.push(']');

    let mut out = J::obj();
    out.raw("grid", &grid)
        .int("count", grid_rows.len() as u64)
        .raw("interiors", &inter)
        .int("interiorCount", ints.len() as u64);
    Ok(out.done())
}

/// Wraithguard: `cells` as bytes (`WGTB`): `grid` (`iissuu`: x, y, name, region,
/// references, has a pathgrid) and `interiors` (`suu`: name, references, has a pathgrid).
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
#[tauri::command(async)]
fn texture_usage(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let usage = w.texture_usage();
    let mut s = String::from("{");
    let mut first = true;
    for (tex, (cells, regions)) in &usage {
        if !first {
            s.push(',');
        }
        first = false;
        s.push('"');
        viewcore::json::escape_into(tex, &mut s);
        s.push_str("\":{\"cells\":");
        s.push_str(&string_array(cells));
        s.push_str(",\"regions\":");
        s.push_str(&string_array(regions));
        s.push('}');
    }
    s.push('}');
    Ok(s)
}

/// Wraithguard: `texture_usage` as bytes (`WGTB`): `textures` (`s`: every texture, in the
/// map's order) and `uses` (`sus`: texture, 0 a cell / 1 a region, its name), so a texture
/// that is in the map with nothing under it keeps its entry.
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

/// Wraithguard: `assets` as bytes (`WGTB`): `meshes` (`s`) and `textures` (`s`) - on a big
/// install hundreds of thousands of names, the list a JSON answer least wants to be.
#[tauri::command(async)]
fn assets_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let v = vfs_of(&state)?;
    let meshes = v.list("meshes/", &[".nif"]);
    let textures = v.list("textures/", viewcore::vfs::Vfs::TEX_EXTS);
    Ok(tauri::ipc::Response::new(pack_names(&[("meshes", &meshes), ("textures", &textures)])))
}

/// The mesh and texture files the overlay exposes, for the asset browsers.
///
/// The page used to walk the folders itself through the File System Access API;
/// here the index already exists as a side effect of connecting, so it is just
/// handed over.
#[tauri::command(async)]
fn assets(state: State) -> Result<String, String> {
    let v = vfs_of(&state)?;
    let meshes = v.list("meshes/", &[".nif"]);
    let textures = v.list("textures/", viewcore::vfs::Vfs::TEX_EXTS);
    let mut out = J::obj();
    out.raw("meshes", &string_array(&meshes)).raw("textures", &string_array(&textures));
    Ok(out.done())
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
) -> Result<String, String> {
    // A cell load asks for one of these per cell and each writes a hundred kilobytes of
    // heights; done under the lock they queue behind every scatter. See `snapshot`.
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

    let mut heights = String::from("[");
    let mut vtex = String::from("[");
    let mut ltex = String::from("{");
    if let Some(l) = land {
        for (i, h) in l.heights.iter().enumerate() {
            if i > 0 {
                heights.push(',');
            }
            heights.push_str(&format!("{:.1}", h));
        }
        let mut seen: Vec<u16> = Vec::new();
        for (i, v) in l.vtex.iter().enumerate() {
            if i > 0 {
                vtex.push(',');
            }
            vtex.push_str(&v.to_string());
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
    heights.push(']');
    vtex.push(']');
    ltex.push('}');

    /* The ground normal at every vertex, as the *engine* resolves it — its own VNML
       where the record has one, the triangle's own normal where it does not.
       Deliberately not "the VNML, and the page derives the rest": which normal a point
       gets is a rule, the engine owns the rules, and a page that re-decided it would be
       a second implementation of exactly the question §11a was measured to answer.

       Signed 16-bit per component, hex, 4 chars each. Bytes would do for shading but
       not for the slope overlay: that is drawn from these normals and compared against
       `fMaximumAngle`, so a coarse encoding would put a visible margin of error along
       the very edge the overlay exists to show. 65x65x3 -> about 50 KB a cell, in line
       with the height grid the page already carries. */
    let mut normals = String::with_capacity(if land.is_some() { VERTS * VERTS * 12 + 2 } else { 2 });
    normals.push('"');
    if let Some(l) = land {
        for vy in 0..VERTS {
            for vx in 0..VERTS {
                let n = l.normal_at(vx as f32 * VERT_STEP, vy as f32 * VERT_STEP);
                for c in n {
                    let q = (c.clamp(-1.0, 1.0) * 32767.0).round() as i32 as u16;
                    normals.push_str(&format!("{q:04x}"));
                }
            }
        }
    }
    normals.push('"');

    /* The vertex colours, VCLR as the record has them: 65x65 RGB bytes, hex, 2 chars
       each — about 25 KB a cell. An empty string for a record without the block, which
       the page draws white, the way the game does. Round 11 item 11. */
    let mut colours = String::with_capacity(VERTS * VERTS * 6 + 2);
    colours.push('"');
    if let Some(c) = land.and_then(|l| l.colours.as_ref()) {
        for b in c.iter() {
            colours.push_str(&format!("{b:02x}"));
        }
    }
    colours.push('"');

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
    let (refs, models, actors, lights) = match &reviewed {
        Some(items) => refs_json_marked(&w, items, false),
        None => refs_json(&w, cell.map(|c| &c.refs), false),
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
        .raw("heights", &heights)
        .raw("normals", &normals)
        .raw("colours", &colours)
        .raw("vtex", &vtex)
        .raw("ltex", &ltex)
        .raw("layers", &layers)
        .raw("refs", &refs)
        .raw("models", &models)
        .raw("actors", &actors)
        .raw("lights", &lights)
        .raw("prov", &prov)
        .raw("landEditors", &string_array(&editors))
        .raw("landMerged", &land_merged)
        .bool("reviewed", reviewed.is_some());
    Ok(out.done())
}

/// Every reference in one place, with the meshes and actors it names.
///
/// Written once and used by both `cell_data` and `interior_data`: a room's contents and a
/// cell's contents are the same question asked of two different containers, and the day
/// they were two copies is the day one of them would stop reporting which plugin supplied
/// a reference.
///
/// Round 18al: `indoors` is a room the sky never reaches - an interior not flagged to
/// behave like an exterior - where a banner whose twin only its script earned draws its
/// plain, still file (`World::model_in`). Robin: "no interior banners wave in the wind";
/// "Interiors that are tagged to work as exteriors should of course still have their
/// flags work as in exteriors too."
fn refs_json(
    w: &viewcore::world::World,
    src: Option<&HashMap<viewcore::esp::RefNum, viewcore::esp::CellRef>>,
    indoors: bool,
) -> (String, String, String, String) {
    // Every version stands (`usize::MAX`: all the history has).
    let items: Vec<viewcore::review::Item> = src
        .map(|m| m.values().map(|r| (r.clone(), viewcore::review::Mark::None, usize::MAX)).collect())
        .unwrap_or_default();
    refs_json_marked(w, &items, indoors)
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

/// [`refs_json`] over references already chosen, each with its review mark.
fn refs_json_marked(
    w: &viewcore::world::World,
    items: &[viewcore::review::Item],
    indoors: bool,
) -> (String, String, String, String) {
    let mut refs = String::from("[");
    let mut models = String::from("{");
    /* Round 17m: the LIGH records among the objects in this cell, so the preview can
       light the scene with them after dark. Keyed the same way as `models`, and only
       for lights that actually burn where they stand — the OffDefault flag means "does
       not burn while placed in a cell". */
    let mut lights = String::from("{");
    let mut first_light = true;
    // Actors travel beside the models rather than among them: the page decides per
    // reference whether an actor is a corpse worth drawing, and needs its health and
    // persist flag to do so.
    let mut actors = String::from("{");
    let mut first = true;
    let mut first_model = true;
    let mut first_actor = true;
    {
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
            if !first {
                refs.push(',');
            }
            first = false;
            refs.push_str("{\"id\":\"");
            viewcore::json::escape_into(&r.id, &mut refs);
            /* `from` is which plugin in the load order supplied this reference — an
               index into the same plugin list the page already holds, not the name,
               because a cell can carry thousands of references and the name would be
               repeated on every one of them. The left column used to show these totalled
               up per plugin, which answered a question nobody had; what people want to
               know is who put *that* rock there, and that is asked by right-clicking it. */
            /* And which reference it *is*, spelled the way the mask spells it. The page
               could name a mesh and a plugin but never an instance, so it could not ask
               "how much paint is on this rock" about the rock under the pointer. This is
               the same key `statics_paint` and `paint_stroke` already use — the same
               naming, carried one place further, rather than a second way of naming a
               reference. Empty when the load order cannot resolve it. */
            let rkey = viewcore::refkey::RefKey::of(r.num, &w.plugins)
                .map(|k| viewcore::refkey::key_text(&k))
                .unwrap_or_default();
            refs.push_str(&format!(
                "\",\"pos\":[{:.2},{:.2},{:.2}],\"rot\":[{:.5},{:.5},{:.5}],\"scale\":{:.4},\"from\":{},\"key\":\"",
                r.pos[0], r.pos[1], r.pos[2], r.rot[0], r.rot[1], r.rot[2], r.scale,
                r.plugin
            ));
            viewcore::json::escape_into(&rkey, &mut refs);
            refs.push('"');
            /* Wraithguard: for reviewing a cell's conflicts - how this reference relates to
               the plugin under review, every plugin that supplied a version of it, and how
               far the load order has moved it from where its first plugin put it. */
            if *mark != viewcore::review::Mark::None {
                refs.push_str(",\"mark\":\"");
                refs.push_str(mark.name());
                refs.push('"');
            }
            let h = w.history.of(&r.num);
            let n = (*versions).min(h.touched.len());
            if n > 1 {
                refs.push_str(&format!(
                    ",\"hist\":[{}]",
                    h.touched[..n].iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",")
                ));
                let d = viewcore::review::moved_by(r, &h.poses[..n.min(h.poses.len())]);
                if d > 0.5 {
                    refs.push_str(&format!(",\"moved\":{:.1}", d));
                }
            }
            /* Gardenfell 18ez: a reference whose class the game never animates, on a mesh
               with a `.kf` beside it: the page asks for that mesh `still` (see
               `mesh_payload`). Said only when it changes the picture, so every reader of a
               reference sees what it saw. Actors and leveled spawns have their own rules. */
            if actor.is_none() && lev.is_none() && !w.anim_ids.contains(&key) {
                let has_kf = w
                    .models
                    .get(&key)
                    .map(|m| w.kf_meshes.contains(&m.replace('\\', "/").to_ascii_lowercase()))
                    .unwrap_or(false);
                if has_kf {
                    refs.push_str(",\"still\":true");
                }
            }
            // Wraithguard: grass from a `groundcover=` plugin.
            if usize::try_from(r.plugin).map(|p| w.groundcover.contains(&p)).unwrap_or(false) {
                refs.push_str(",\"gc\":1");
            }
            /* Round 17y: where a door leads, when it teleports. The dialogue offers to open
               the cell on the far side; an interior is named, an exterior is wherever the
               far position falls. */
            if let Some(d) = &r.door {
                refs.push_str(&format!(
                    ",\"door\":{{\"pos\":[{:.2},{:.2},{:.2}],\"rot\":[{:.5},{:.5},{:.5}],\"cell\":\"",
                    d.pos[0], d.pos[1], d.pos[2], d.rot[0], d.rot[1], d.rot[2]
                ));
                viewcore::json::escape_into(&d.cell, &mut refs);
                refs.push_str("\"}");
            }
            refs.push('}');
            if let Some(o) = lev {
                if !actors.contains(&format!("\"{}\":", key)) {
                    if !first_actor {
                        actors.push(',');
                    }
                    first_actor = false;
                    push_lev(&key, &o.model, &mut actors);
                }
                continue;
            }
            if let Some(a) = actor {
                if !first_actor {
                    actors.push(',');
                }
                first_actor = false;
                push_actor(w, &key, a, &mut actors);
                continue;
            }
            if let Some(li) = w.lights.get(&key) {
                if !li.off_default() && !lights.contains(&format!("\"{}\":", key)) {
                    if !first_light {
                        lights.push(',');
                    }
                    first_light = false;
                    lights.push('"');
                    viewcore::json::escape_into(&key, &mut lights);
                    // Round 17y: and the record's flags whole, for rules the page may want
                    // (Fire 0x010, Flicker 0x008, Pulse 0x080...).
                    lights.push_str(&format!(
                        "\":{{\"radius\":{},\"colour\":[{},{},{}],\"negative\":{},\"flags\":{}}}",
                        li.radius.max(16),
                        li.colour[0],
                        li.colour[1],
                        li.colour[2],
                        li.negative(),
                        li.flags
                    ));
                }
            }
            let mesh = w.model_in(&key, indoors).unwrap_or_default();
            if !first_model {
                models.push(',');
            }
            first_model = false;
            models.push('"');
            viewcore::json::escape_into(&key, &mut models);
            models.push_str("\":\"");
            viewcore::json::escape_into(mesh, &mut models);
            models.push('"');
        }
    }
    refs.push(']');
    models.push('}');
    actors.push('}');
    lights.push('}');
    (refs, models, actors, lights)
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
/// The same answer `cell_data` gives, minus everything that is about ground: no heights,
/// no normals, no vtex, no coverage layers, and `hasLand` false. A room is its contents.
///
/// Keyed by name because an interior has no grid — which is the whole of the bug this
/// exists to fix. `CellData.loadCell` asked for `(gx, gy)`, an interior has none, so every
/// one of them arrived as (0, 0) and quietly loaded whichever exterior sits at the origin.
/// Matched without regard to case, like every other name in the tool (§50).
#[tauri::command(async)]
fn interior_data(name: String, review: Option<String>, without: Option<bool>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }
        .ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let room = w.room(&name).ok_or_else(|| viewcore::msg!("eng.no_interior", name = name))?;

    let gone = &w.history.interior(&viewcore::world::room_key(&name)).gone;
    let reviewed = review_items(&w, &room.refs, gone, review.as_deref(), without.unwrap_or(false), &|_| true);
    let (refs, models, actors, lights) = match &reviewed {
        Some(items) => refs_json_marked(&w, items, !room.quasi),
        None => refs_json(&w, Some(&room.refs), !room.quasi),
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
        .raw("heights", "[]")
        .str("normals", "")
        .str("colours", "")
        .raw("vtex", "[]")
        .raw("ltex", "{}")
        .raw("layers", "[]")
        .raw("refs", &refs)
        .raw("models", &models)
        .raw("actors", &actors)
        .raw("lights", &lights)
        .raw("prov", &prov)
        .raw("landEditors", "[]")
        .raw("landMerged", "null")
        .bool("reviewed", reviewed.is_some());
    Ok(out.done())
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
fn write_path_whole(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_file_whole(std::path::Path::new(path), bytes)
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
/// `still`: the reference's class never animates (Gardenfell 18ez; see `refs_json_marked`),
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
/// rings, grouped by model, as `{"models":[{"m":model,"r":radius,"t":tier,"refs":[[x,y,z,
/// rx,ry,rz,scale],...]}],"refs":n,"beyond":n,"over":n}`. Budgeted as MGE XE budgets its
/// distant statics (`viewcore::lod`): each reference is near, far or very far by its radius
/// times its scale, and drawn only within its tier's band of the cells past the rings;
/// `t` is the model's own tier, which its one simplified copy is made for. Left out too:
/// what is deleted, actors (they move), grass from `groundcover=` plugins (drawn only
/// near), and markers (not drawn in the game).
#[tauri::command(async)]
fn distant_objects(x: i32, y: i32, inner: i32, radius: i32, state: State) -> Result<String, String> {
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
    let mut out = String::from("{\"models\":[");
    let mut first_model = true;
    for (mi, m) in models.iter().enumerate() {
        if chosen[mi].is_empty() {
            continue;
        }
        let list = &by_model[m];
        let mut refs = String::new();
        for &ri in &chosen[mi] {
            let (pos, rot, scale, _) = &list[ri];
            if !refs.is_empty() {
                refs.push(',');
            }
            refs.push_str(&format!(
                "[{:.1},{:.1},{:.1},{:.5},{:.5},{:.5},{:.4}]",
                pos[0], pos[1], pos[2], rot[0], rot[1], rot[2], scale
            ));
        }
        if !first_model {
            out.push(',');
        }
        first_model = false;
        out.push_str("{\"m\":\"");
        viewcore::json::escape_into(m, &mut out);
        out.push_str(&format!("\",\"r\":{:.1},\"t\":{},\"refs\":[{refs}]}}", radii[mi], viewcore::lod::tier_of(radii[mi])));
    }
    out.push_str(&format!("],\"refs\":{kept},\"beyond\":{beyond},\"over\":{over}}}"));
    Ok(out)
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

pub(crate) fn collision_payload(v: &Vfs, path: &str) -> Result<Vec<u8>, String> {
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

/* ---- the Editor mode (Wraithguard, 50_wg_editor.js) ------------------------------------

   The Construction Set's Object Window and Cell View, from the world the viewer already
   loaded: what records each tag has and who defines them, and what references a cell
   holds without drawing it. A record's contents and every edit are Wraithguard's (the
   page asks it over the `links` the launch handed over); this only lists. */

/// Every reference in the world by base id (lower case) -> how many are placed. Counted
/// once per world (`rev`), the cells split across the pool, and kept: every Object Window
/// tab reads it, and it holds every reference the load order places, grass included.
fn placed_counts(w: &World, rev: u64) -> std::sync::Arc<HashMap<String, u32>> {
    type Kept = (u64, std::sync::Arc<HashMap<String, u32>>);
    static KEPT: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
    if let Some((r, n)) = KEPT.lock().ok().and_then(|k| k.clone()) {
        if r == rev {
            return n;
        }
    }
    let cells: Vec<_> = w.cells.values().map(|c| &c.refs).collect();
    let rooms: Vec<_> = w.interiors.values().map(|c| &c.refs).collect();
    let mut parts = viewcore::pool::par_map(&cells, |_, refs| {
        let mut n: HashMap<String, u32> = HashMap::new();
        for r in refs.values().filter(|r| !r.deleted) {
            *n.entry(r.id.to_ascii_lowercase()).or_default() += 1;
        }
        n
    });
    parts.extend(viewcore::pool::par_map(&rooms, |_, refs| {
        let mut n: HashMap<String, u32> = HashMap::new();
        for r in refs.values().filter(|r| !r.deleted) {
            *n.entry(r.id.to_ascii_lowercase()).or_default() += 1;
        }
        n
    }));
    let mut n: HashMap<String, u32> = HashMap::new();
    for p in parts {
        for (k, v) in p {
            *n.entry(k).or_default() += v;
        }
    }
    let n = std::sync::Arc::new(n);
    if let Ok(mut k) = KEPT.lock() {
        *k = Some((rev, n.clone()));
    }
    n
}

/// The Object Window's tabs: `[[tag, count]...]`, every tag the load order has records of.
#[tauri::command(async)]
fn editor_tags(state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let rows: Vec<String> = tag_counts(&w)
        .iter()
        .map(|(t, c)| {
            let mut s = String::from("[\"");
            viewcore::json::escape_into(t, &mut s);
            s.push_str(&format!("\",{c}]"));
            s
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

/// Wraithguard: `editor_tags` as bytes (`WGTB`): one table, `tags` (`su`: tag, count).
#[tauri::command(async)]
fn editor_tags_bytes(state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok(tauri::ipc::Response::new(pack_tags(&tag_counts(&w))))
}

fn pack_tags(tags: &[(String, u64)]) -> Vec<u8> {
    let mut t = Tables::new();
    t.table("tags", "su", tags.len());
    for (tag, c) in tags {
        t.s(tag).u(*c);
    }
    t.done()
}

/// Every tag the load order has records of, with how many, in tag order.
fn tag_counts(w: &viewcore::world::World) -> Vec<(String, u64)> {
    // Wraithguard: every live record in its own type's tab - an id defined as a weapon
    // and a misc item is in both, as OpenMW keeps both (`ordercore::records`).
    let mut n: HashMap<viewcore::esp::Tag, u64> = HashMap::new();
    for (tag, _id, _v) in w.records.iter() {
        *n.entry(*tag).or_default() += 1;
    }
    let mut tags: Vec<(viewcore::esp::Tag, u64)> = n.into_iter().collect();
    tags.sort();
    tags.into_iter().map(|(t, c)| (tag_text(&t), c)).collect()
}

/// One Object Window row: the id as shown, name, model, the plugins defining it (load-order
/// indices, the last winning) and how many are placed.
struct ObjRow {
    key: String,
    id: String,
    name: String,
    model: String,
    plugins: Vec<usize>,
    placed: u32,
}

fn tag_of(tag: &str) -> viewcore::esp::Tag {
    let mut t = [b'_'; 4];
    for (i, b) in tag.bytes().take(4).enumerate() {
        t[i] = b.to_ascii_uppercase();
    }
    t
}

/// The Object Window's rows for one tag, sorted by id: straight from the record index the
/// world was loaded with (`ordercore::records`) - nothing the scene has drawn. The id is as
/// the record spells it where the world kept it (objects), else as the winning plugin does.
fn object_rows(w: &World, rev: u64, want: viewcore::esp::Tag) -> Vec<ObjRow> {
    let placed = placed_counts(w, rev);
    let mut rows: Vec<ObjRow> = Vec::new();
    // Wraithguard: this type's live records (`ordercore::records`), whatever else the id is.
    for (tag, id, v) in w.records.iter() {
        if *tag != want {
            continue;
        }
        // The world's object, model and name are the placed type's: another type's row
        // shows its id alone.
        let is_placed = w.records.is_placed(tag, id);
        let obj = w.objects.get(id).filter(|_| is_placed);
        let shown = obj.map(|o| o.id.as_str()).filter(|s| !s.is_empty()).unwrap_or(&v.id);
        // Lights, statics and the like keep their mesh in `models` rather than `objects`.
        let model = obj
            .map(|o| o.model.as_str())
            .filter(|m| !m.is_empty())
            .or_else(|| w.models.get(id).filter(|_| is_placed).map(String::as_str));
        // Wraithguard: a land texture has no mesh; its texture file stands in the column
        // (the Editor's preview shows the texture itself).
        let model = model.or_else(|| if want == *b"LTEX" { w.ltex_file.get(id).map(String::as_str) } else { None });
        // A body part is never placed, so it has no object or model above; its mesh is in
        // the NPC tables (the parts an NPC is assembled from), and the preview needs it.
        let model = model.or_else(|| {
            if want == *b"BODY" {
                w.npc.bodies.get(&id.to_ascii_lowercase()).map(|b| b.model.as_str()).filter(|m| !m.is_empty())
            } else {
                None
            }
        });
        rows.push(ObjRow {
            key: id.to_string(),
            id: shown.to_string(),
            name: obj.map(|o| o.name.clone()).unwrap_or_default(),
            model: model.unwrap_or("").to_string(),
            plugins: v.plugins.clone(),
            // The references placed are of the type the id is placed as.
            placed: if is_placed { placed.get(id).copied().unwrap_or(0) } else { 0 },
        });
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

fn world_and_rev(state: &State) -> Result<(std::sync::Arc<World>, u64), String> {
    let app = state.app();
    let w = app.world.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok((w, app.world_rev))
}

/// The Object Window's rows for one tag as JSON: `{plugins:[names], rows:[[id, name,
/// model, [plugin ix...], placed]...]}` - for the Editor's other lists (dialogue, place,
/// record), and the Object Window's fallback when `editor_rows` fails.
#[tauri::command(async)]
fn editor_records(tag: String, state: State) -> Result<String, String> {
    let (w, rev) = world_and_rev(&state)?;
    let rows: Vec<String> = object_rows(&w, rev, tag_of(&tag))
        .into_iter()
        .map(|r| {
            let mut row = String::from("[\"");
            viewcore::json::escape_into(&r.id, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.name, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.model, &mut row);
            let ixs: Vec<String> = r.plugins.iter().map(usize::to_string).collect();
            row.push_str(&format!("\",[{}],{}]", ixs.join(","), r.placed));
            row
        })
        .collect();
    let mut o = J::obj();
    o.raw("plugins", &string_array(&w.plugins));
    o.raw("rows", &format!("[{}]", rows.join(",")));
    Ok(o.done())
}

/// Wraithguard: the Object Window's rows for one tag, as bytes (no JSON to go wrong):
/// `"WGOR"`, u32 version 1, u32 plugin count, each plugin a string; u32 row count, each row
/// id, name, model (strings), u32 plugin count, that many u32 indices, u32 placed. A string
/// is u32 byte length + UTF-8. All little-endian.
#[tauri::command(async)]
fn editor_rows(tag: String, state: State) -> Result<tauri::ipc::Response, String> {
    let (w, rev) = world_and_rev(&state)?;
    Ok(tauri::ipc::Response::new(pack_rows(&w.plugins, &object_rows(&w, rev, tag_of(&tag)))))
}

fn pack_rows(plugins: &[String], rows: &[ObjRow]) -> Vec<u8> {
    fn u32_(o: &mut Vec<u8>, v: usize) {
        o.extend_from_slice(&(v.min(u32::MAX as usize) as u32).to_le_bytes());
    }
    fn str_(o: &mut Vec<u8>, s: &str) {
        u32_(o, s.len());
        o.extend_from_slice(s.as_bytes());
    }
    let mut o = b"WGOR".to_vec();
    u32_(&mut o, 1);
    u32_(&mut o, plugins.len());
    for p in plugins {
        str_(&mut o, p);
    }
    u32_(&mut o, rows.len());
    for r in rows {
        str_(&mut o, &r.id);
        str_(&mut o, &r.name);
        str_(&mut o, &r.model);
        u32_(&mut o, r.plugins.len());
        for &p in &r.plugins {
            u32_(&mut o, p);
        }
        u32_(&mut o, r.placed as usize);
    }
    o
}

/* Wraithguard: `WGTB`, the Editor's other lists as bytes - the same reason as `WGOR`
   (a JSON answer the page cannot read looked like an empty list, Static 2026-10-13), in
   one shape for every list rather than a format each:

   "WGTB" | u32 version 1 | u32 table count | per table: name (string), column types
   (string, one letter a column: `s` string, `u` u32, `i` i32, `d` f64), u32 row count,
   then the rows' values in order. A string is u32 byte length + UTF-8; little-endian.

   The page's `unpackTables` (03_core.js) reads it; `ui/tests/editor/wgtb_tables.json` is
   the one fixture both sides are tested against. The writer does not check a row against
   its column letters: each list is built in one place and the fixture holds the format. */
struct Tables {
    o: Vec<u8>,
    tables: u32,
}

impl Tables {
    fn new() -> Self {
        let mut o = b"WGTB".to_vec();
        o.extend_from_slice(&1u32.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // the table count, set by `done`
        Tables { o, tables: 0 }
    }
    fn table(&mut self, name: &str, cols: &str, rows: usize) -> &mut Self {
        self.tables += 1;
        self.s(name).s(cols);
        self.o.extend_from_slice(&(rows.min(u32::MAX as usize) as u32).to_le_bytes());
        self
    }
    fn s(&mut self, v: &str) -> &mut Self {
        self.o.extend_from_slice(&(v.len() as u32).to_le_bytes());
        self.o.extend_from_slice(v.as_bytes());
        self
    }
    fn u(&mut self, v: u64) -> &mut Self {
        self.o.extend_from_slice(&(v.min(u32::MAX as u64) as u32).to_le_bytes());
        self
    }
    fn i(&mut self, v: i64) -> &mut Self {
        self.o.extend_from_slice(&(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_le_bytes());
        self
    }
    fn d(&mut self, v: f64) -> &mut Self {
        self.o.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn done(mut self) -> Vec<u8> {
        let n = self.tables.to_le_bytes();
        self.o[8..12].copy_from_slice(&n);
        self.o
    }
}

/// One actor, `"key":{kind, model, twin, health, persistent, corpse, hello?}`, as the
/// cell's `actors` map carries it (and `editor_actors`).
fn push_actor(w: &viewcore::world::World, key: &str, a: &viewcore::world::ActorDef, actors: &mut String) {
    actors.push('"');
    viewcore::json::escape_into(key, actors);
    actors.push_str("\":{\"kind\":\"");
    actors.push_str(if a.creature { "crea" } else { "npc" });
    actors.push_str("\",\"model\":\"");
    viewcore::json::escape_into(&a.model, actors);
    // Round 18ag: and the `x` twin the game draws the living creature with.
    actors.push_str("\",\"twin\":");
    match &a.twin {
        Some(t) => {
            actors.push('"');
            viewcore::json::escape_into(t, actors);
            actors.push('"');
        }
        None => actors.push_str("null"),
    }
    actors.push_str(",\"health\":");
    match a.health {
        Some(h) => actors.push_str(&h.to_string()),
        None => actors.push_str("null"),
    }
    actors.push_str(",\"persistent\":");
    actors.push_str(if a.persistent { "true" } else { "false" });
    actors.push_str(",\"corpse\":");
    actors.push_str(if a.corpse() { "true" } else { "false" });
    // Wraithguard: AI Hello, for the greeting-distance overlay.
    if let Some(h) = w.hello.get(key) {
        actors.push_str(&format!(",\"hello\":{}", h));
    }
    actors.push('}');
}

/// A leveled creature list placed as a spawn point, as the cell's `actors` map carries it.
fn push_lev(key: &str, model: &str, actors: &mut String) {
    actors.push('"');
    viewcore::json::escape_into(key, actors);
    actors.push_str("\":{\"kind\":\"lev\",\"model\":\"");
    viewcore::json::escape_into(model, actors);
    actors.push_str("\",\"twin\":null,\"health\":null,\"persistent\":false,\"corpse\":false}");
}

/// Wraithguard: what the render window needs to draw objects it has not seen placed (a
/// reference the editor adds): `{actors:{...}, models:{id: mesh}}`, as a cell carries
/// them, for the ids asked.
#[tauri::command(async)]
fn editor_actors(ids: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut actors = String::from("{");
    let mut models = String::from("{");
    for id in ids.iter().take(4096) {
        let key = id.to_ascii_lowercase();
        if let Some(a) = w.actors.get(&key) {
            if actors.len() > 1 {
                actors.push(',');
            }
            push_actor(&w, &key, a, &mut actors);
        } else if let Some(o) = w.objects.get(&key).filter(|o| o.kind == Some(viewcore::objects::ObjKind::LeveledCreature)) {
            if actors.len() > 1 {
                actors.push(',');
            }
            push_lev(&key, &o.model, &mut actors);
        }
        if let Some(m) = w.models.get(&key) {
            if models.len() > 1 {
                models.push(',');
            }
            models.push('"');
            viewcore::json::escape_into(&key, &mut models);
            models.push_str("\":\"");
            viewcore::json::escape_into(m, &mut models);
            models.push('"');
        }
    }
    actors.push('}');
    models.push('}');
    let mut o = J::obj();
    o.raw("actors", &actors).raw("models", &models);
    Ok(o.done())
}

/// The Cell View's object list for one cell (`x,y`, or `int:<name>`): `{name, rows:[[key,
/// id, tag, [x,y,z], plugin]...]}` - every reference not deleted, sorted by id; `key` is
/// the viewport's reference key, `plugin` the file whose version of it wins.
#[tauri::command(async)]
fn editor_cell_refs(cell: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let c = cell_refs_of(&w, &cell)?;
    let rows: Vec<String> = c
        .rows
        .iter()
        .map(|r| {
            let mut row = String::from("[\"");
            viewcore::json::escape_into(&r.key, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.id, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.tag, &mut row);
            row.push_str(&format!("\",[{:.1},{:.1},{:.1}],\"", r.pos[0], r.pos[1], r.pos[2]));
            viewcore::json::escape_into(&r.plugin, &mut row);
            row.push_str("\",\"");
            viewcore::json::escape_into(&r.owner, &mut row);
            row.push_str("\"]");
            row
        })
        .collect();
    let mut o = J::obj();
    o.str("name", &c.name).str("refCell", &c.ref_cell).raw("plugins", &string_array(&c.plugins));
    o.raw("rows", &format!("[{}]", rows.join(",")));
    Ok(o.done())
}

/// Wraithguard: `editor_cell_refs` as bytes (`WGTB`): `meta` (`ss`: name, refCell, one
/// row), `plugins` (`s`) and `rows` (`sssdddss`: key, id, tag, x, y, z, plugin, owner;
/// the position to a tenth, as the JSON gives it).
#[tauri::command(async)]
fn editor_cell_refs_bytes(cell: String, state: State) -> Result<tauri::ipc::Response, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    Ok(tauri::ipc::Response::new(pack_cell_refs(&cell_refs_of(&w, &cell)?)))
}

/// One cell's references for the Cell View, as `cell_refs_of` lists them.
struct CellRefs {
    name: String,
    ref_cell: String,
    plugins: Vec<String>,
    rows: Vec<RefRow>,
}

struct RefRow {
    key: String,
    id: String,
    tag: String,
    pos: [f32; 3],
    plugin: String,
    owner: String,
}

fn pack_cell_refs(c: &CellRefs) -> Vec<u8> {
    // To a tenth exactly as the JSON writes it (`{:.1}`, which rounds a tie to even).
    let tenth = |v: f32| format!("{v:.1}").parse::<f64>().unwrap_or(0.0);
    let mut t = Tables::new();
    t.table("meta", "ss", 1).s(&c.name).s(&c.ref_cell);
    t.table("plugins", "s", c.plugins.len());
    for p in &c.plugins {
        t.s(p);
    }
    t.table("rows", "sssdddss", c.rows.len());
    for r in &c.rows {
        t.s(&r.key).s(&r.id).s(&r.tag).d(tenth(r.pos[0])).d(tenth(r.pos[1])).d(tenth(r.pos[2])).s(&r.plugin).s(&r.owner);
    }
    t.done()
}

/// The Cell View's references for one cell (`x,y`, or `int:<name>`): every reference not
/// deleted, sorted by id, each with its viewport key, its record's type, where it stands,
/// the plugin whose version wins and its owner.
fn cell_refs_of(w: &viewcore::world::World, cell: &str) -> Result<CellRefs, String> {
    // Also the cell's key as plugins' records key it, and the plugins with a CELL record
    // for it: what the editor's reference dialog asks Wraithguard with.
    type Cell<'a> = (String, String, &'a [u32], Vec<&'a viewcore::esp::CellRef>);
    let key = viewcore::cells::CellKey::parse(cell).ok_or_else(|| format!("not a cell: {cell}"))?;
    let (name, ref_cell, by, refs): Cell = match &key {
        viewcore::cells::CellKey::Interior(room) => {
            let k = viewcore::world::room_key(room);
            let r = w.interiors.get(&k).ok_or_else(|| format!("no interior {room}"))?;
            (r.name.clone(), r.name.clone(), &w.history.interior(&k).touched_by, r.refs.values().collect())
        }
        &viewcore::cells::CellKey::Exterior(x, y) => {
            let c = w.cells.get(&(x, y)).ok_or_else(|| format!("no exterior cell {x},{y}"))?;
            // The pool's spelling, `(x, y)`: how the Editor's records name the cell.
            (c.name.clone(), key.pool_key(), &w.history.exterior((x, y)).touched_by, c.refs.values().collect())
        }
    };
    let mut refs: Vec<&viewcore::esp::CellRef> = refs.into_iter().filter(|r| !r.deleted).collect();
    refs.sort_by(|a, b| a.id.to_ascii_lowercase().cmp(&b.id.to_ascii_lowercase()).then(a.num.index.cmp(&b.num.index)));
    /* Wraithguard: each reference's owner, as the plugin that last changed it writes it -
       the Construction Set's Ownership column. The world keeps no owners (drawing does not
       need them), so each plugin that last changed some of these is read once for them. */
    let sel = match key {
        viewcore::cells::CellKey::Interior(_) => viewcore::inspect::CellSel::Interior(ref_cell.clone()),
        viewcore::cells::CellKey::Exterior(x, y) => viewcore::inspect::CellSel::Exterior(x, y),
    };
    let mut owners: std::collections::HashMap<i32, std::collections::HashMap<(u32, String), viewcore::inspect::RefDetails>> = std::collections::HashMap::new();
    for r in &refs {
        if owners.contains_key(&r.plugin) {
            continue;
        }
        let got = usize::try_from(r.plugin)
            .ok()
            .and_then(|i| w.plugin_paths.get(i))
            .and_then(|p| std::fs::read(p).ok())
            .map(|buf| viewcore::inspect::cell_details(&buf, &sel))
            .unwrap_or_default();
        owners.insert(r.plugin, got);
    }
    let owner_of = |r: &viewcore::esp::CellRef| -> String {
        let Some(d) = owners.get(&r.plugin).and_then(|m| m.get(&(r.num.index & 0x00ff_ffff, r.id.to_ascii_lowercase()))) else {
            return String::new();
        };
        if !d.owner.is_empty() {
            d.owner.clone()
        } else if !d.faction.is_empty() {
            match d.rank {
                Some(k) => format!("{} (rank {k})", d.faction),
                None => d.faction.clone(),
            }
        } else {
            String::new()
        }
    };
    let rows: Vec<RefRow> = refs
        .iter()
        .map(|r| RefRow {
            key: viewcore::refkey::RefKey::of(r.num, &w.plugins).map(|k| viewcore::refkey::key_text(&k)).unwrap_or_default(),
            id: r.id.clone(),
            tag: w
                .record_where
                .get(&r.id.to_ascii_lowercase())
                .and_then(|d| d.last())
                .map(|(_, t)| tag_text(t))
                .unwrap_or_default(),
            pos: [r.pos[0], r.pos[1], r.pos[2]],
            plugin: plugin_name(w, usize::try_from(r.plugin).ok()).to_string(),
            owner: owner_of(r),
        })
        .collect();
    let plugins: Vec<String> = by.iter().map(|&i| plugin_name(w, Some(i as usize)).to_string()).collect();
    Ok(CellRefs { name, ref_cell, plugins, rows })
}

/// Wraithguard: what kind of record an id is, for the Editor opening one by id alone (a
/// link in the inspector or the full help, whose caller can only guess the type): `{tag,
/// plugins}` - the type the winning plugin gives it, and the plugins that define it as that
/// type, in load order - or `{tag: null}` when no plugin has it.
#[tauri::command(async)]
fn editor_record_tag(id: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut o = J::obj();
    match w.record_where.get(&id.trim().to_ascii_lowercase()).and_then(|d| d.last().map(|(_, t)| (d, *t))) {
        Some((defs, tag)) => {
            let mut ixs: Vec<usize> = defs.iter().filter(|(_, t)| *t == tag).map(|(p, _)| *p).collect();
            ixs.dedup();
            let plugins: Vec<String> = ixs.iter().map(|&i| plugin_name(&w, Some(i)).to_string()).collect();
            o.str("tag", &tag_text(&tag)).raw("plugins", &string_array(&plugins));
        }
        None => {
            o.raw("tag", "null").raw("plugins", "[]");
        }
    }
    Ok(o.done())
}

/// Wraithguard: the load order's OpenMW Lua scripts, found and checked by the engine
/// (`viewcore::luascan`, luacore's scan over the overlay - packed scripts included): `{openmw,
/// revision, resources, note, scripts:[{path, flags, file, providers, interface,
/// handlers}], findings:[{path, severity, code, line, message}], report}`. The Editor's
/// Lua panel. A Morrowind.ini setup has none, and says so in `note`.
#[tauri::command(async)]
fn lua_scan(state: State) -> Result<String, String> {
    let (vfs, order) = {
        let app = state.app();
        (app.vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))?, app.order_file.clone())
    };
    viewcore::luascan::scan_json(&vfs, order.as_deref())
}

/// The Editor's script flowchart (`editFlowchart` was Wraithguard's, opening its chart
/// window): the control flow of the script as typed, `{heading, chart}` (Mermaid), shown in
/// the viewer's own chart panel.
#[tauri::command(async)]
fn editor_script_flowchart(id: String, text: String) -> Result<String, String> {
    let v = serde_json::json!({"heading": format!("{id} - flowchart"), "chart": editcore::mwscript::flowchart::flowchart(&text)});
    Ok(v.to_string())
}

/// Wraithguard: a Lua script's charts, from luacore (`editLuaChart` was Wraithguard's): `kind`
/// `functions` (each function's name and line, to pick from), `flow` (the whole script's
/// flowchart, or the function's at `line`) or `calls` (its call graph). `text` is the script
/// as the page read it. Answers `{functions}` or `{heading, chart}` (Mermaid).
#[tauri::command(async)]
fn lua_chart(path: String, text: String, kind: String, line: Option<usize>) -> Result<String, String> {
    use viewcore::luacore::{mermaid, parser};
    let tree = parser::parse(&text, path.to_lowercase().ends_with(".tl")).map_err(|(m, l, c)| format!("{path}:{l}:{c}: {m}"))?;
    // Depth first, as the Python's walk; one per line (a named function's body shares it).
    fn walk<'a>(n: &'a parser::Node, out: &mut Vec<&'a parser::Node>) {
        out.push(n);
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut all = Vec::new();
    walk(&tree, &mut all);
    let mut by_line: std::collections::BTreeMap<usize, &parser::Node> = std::collections::BTreeMap::new();
    for n in all {
        if matches!(n.kind, "Function" | "LocalFunction" | "FunctionStat") {
            by_line.entry(n.line).or_insert(n);
        }
    }
    let name = |n: &parser::Node| n.value.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("function at line {}", n.line));
    match kind.as_str() {
        "functions" => Ok(serde_json::json!({"functions": by_line.values().map(|n| serde_json::json!({"name": name(n), "line": n.line})).collect::<Vec<_>>()}).to_string()),
        "calls" => Ok(serde_json::json!({"heading": format!("{path} - call graph"), "chart": mermaid::call_graph_chart(&tree)}).to_string()),
        "flow" => {
            let target = line.and_then(|l| by_line.get(&l).copied());
            let heading = match target {
                Some(n) => format!("{path} - {}", name(n)),
                None => path.clone(),
            };
            Ok(serde_json::json!({"heading": heading, "chart": mermaid::flowchart(target.unwrap_or(&tree))}).to_string())
        }
        _ => Err(format!("no chart {kind:?}")),
    }
}

/// Wraithguard: a POST to Wraithguard's loopback server, at a URL it handed the viewer
/// (`links` in the launch's extra file); the answer's text. The viewer asks it for the
/// next place to show (`poll`).
#[tauri::command(async)]
fn wg_post(url: String, body: String) -> Result<String, String> {
    let bytes = loopback_post(&url, body.as_bytes())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Wraithguard: asks Wraithguard (over its loopback server, at the URL it handed the
/// viewer) to show a record in its conflict viewer. `body` is `{tag, id}`.
#[tauri::command(async)]
fn wg_open_record(url: String, body: String) -> Result<String, String> {
    let bytes = loopback_post(&url, body.as_bytes())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/* ---- the Conflicts page (Wraithguard; port step 4.3, slice 11) ----------------------

   Wraithguard's conflict window, answered by `conflictcore`: the record scan over the load
   order the launch's openmw.cfg names (`conflicts_scan`, conflicts or either kind of single),
   one record's fields across its plugins with what each field and plugin is doing
   (`conflicts_fields`), the loose-file scan with its mesh findings (`conflicts_resources`), a
   texture pair compared (`conflicts_texture`), the reports and CSVs (`conflicts_report`,
   `conflicts_csv`), and a record or field queued for the patch in the journal the Editor and
   Wraithguard share (`conflicts_patch`). The last scans are kept here between calls. */

struct ConflictScan {
    list: Vec<conflictcore::records::Conflict>,
    stats: conflictcore::records::Stats,
    mode: String,
}

struct ResourceScan {
    list: Vec<conflictcore::resources::Resource>,
    stats: conflictcore::resources::ResourceStats,
    mesh: Vec<Option<conflictcore::mesh::Finding>>,
}

static CONFLICTS: Mutex<Option<ConflictScan>> = Mutex::new(None);
static RESOURCES: Mutex<Option<ResourceScan>> = Mutex::new(None);

fn cf_setup(cfg: &str) -> Result<viewcore::ordercore::cfg::OpenmwCfg, String> {
    let path = std::path::Path::new(cfg);
    viewcore::ordercore::cfg::read(path, &viewcore::ordercore::cfg::Tokens::for_cfg(path)).map_err(|e| format!("{cfg} could not be read: {e}"))
}

fn cf_conflict_json(c: &conflictcore::records::Conflict) -> serde_json::Value {
    serde_json::json!({"type": c.rtype, "id": c.id, "plugins": c.plugins, "winner": c.winner,
                       "involvesSubset": c.involves_subset, "deletedBy": c.deleted_by})
}

/// Wraithguard: the record scan (`detect_conflicts` / `list_subset_singles` /
/// `list_other_singles`). `mode`: `conflicts`, `mine` or `other`; `subset`: the user's own
/// plugins; `exclude`: patterns of plugins left out (`filter_plugins`). Answers `{list,
/// stats: {scanned, unreadable, records, found, excluded}, cells: [{x, y, total, mine, types,
/// plugins, byPlugin, winners}]}`.
#[tauri::command(async)]
fn conflicts_scan(cfg: String, subset: Vec<String>, mode: String, exclude: Vec<String>) -> Result<String, String> {
    use conflictcore::records;
    let setup = cf_setup(&cfg)?;
    let (order, excluded) = records::filter_plugins(&setup.plugins(), &exclude);
    let at: std::collections::HashMap<String, PathBuf> =
        setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let find = move |name: &str| at.get(name).cloned();
    let scan = records::scan_touch(&order, &find, &records::plugin_keys);
    let (list, stats) = match mode.as_str() {
        "conflicts" => records::conflicts(&scan, &subset),
        "mine" => records::singles(&scan, &subset, true),
        "other" => records::singles(&scan, &subset, false),
        _ => return Err(format!("no scan {mode:?}")),
    };
    let cells: Vec<serde_json::Value> = if mode == "conflicts" {
        conflictcore::map::group_by_cell(&list)
            .values()
            .map(|c| {
                serde_json::json!({"x": c.x, "y": c.y, "total": c.total, "mine": c.mine, "types": c.types,
                                   "plugins": c.plugins, "byPlugin": c.by_plugin, "winners": c.winners})
            })
            .collect()
    } else {
        Vec::new()
    };
    let out = serde_json::json!({
        "list": list.iter().map(cf_conflict_json).collect::<Vec<_>>(),
        "stats": {"scanned": stats.scanned, "unreadable": stats.unreadable, "records": stats.records,
                  "found": stats.found, "excluded": excluded},
        "cells": cells,
    });
    *CONFLICTS.lock().unwrap_or_else(|p| p.into_inner()) = Some(ConflictScan { list, stats, mode });
    Ok(out.to_string())
}

/// Wraithguard: one record of the last scan across its plugins (`diff_record_fields`, with
/// `wraithguard.patch.status`). Answers `{plugins, fields: [{name, values: [value | null],
/// present: [bool], differs, all, this: [status]}], all, this}` - `all` a field's or the
/// record's `ConflictAll` (0-4), `this` each plugin's `ConflictThis` name.
#[tauri::command(async)]
fn conflicts_fields(cfg: String, rtype: String, id: String) -> Result<String, String> {
    use conflictcore::fields::{conflict_all, conflict_this, diff, records_of, worst_all, worst_this};
    let plugins: Vec<String> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().find(|c| c.rtype == rtype && c.id == id).map(|c| c.plugins.clone()).ok_or("that record is not in the last scan")?
    };
    let setup = cf_setup(&cfg)?;
    let key = (rtype.clone(), id.clone());
    let wanted: std::collections::HashSet<(String, String)> = std::iter::once(key.clone()).collect();
    let records: Vec<Option<serde_json::Value>> = setup
        .locate(&plugins)
        .into_iter()
        .map(|(_, p)| p.and_then(|p| std::fs::read(p).ok()).and_then(|b| records_of(&b, &wanted).ok()).and_then(|mut m| m.remove(&key)))
        .collect();
    let versions: Vec<(String, Option<&serde_json::Value>)> = plugins.iter().cloned().zip(records.iter().map(Option::as_ref)).collect();
    let d = diff(&versions);
    let mut field_all = Vec::new();
    let mut field_this: Vec<Vec<conflictcore::fields::ConflictThis>> = Vec::new();
    let fields: Vec<serde_json::Value> = d
        .ordered
        .iter()
        .map(|name| {
            let values = d.values(name);
            let all = conflict_all(&values, false);
            let this = conflict_this(&values, false);
            field_all.push(all);
            field_this.push(this.clone());
            serde_json::json!({
                "name": name,
                "values": values.iter().map(|v| v.cloned().unwrap_or(serde_json::Value::Null)).collect::<Vec<_>>(),
                "present": values.iter().map(Option::is_some).collect::<Vec<_>>(),
                "differs": d.differing.contains(name),
                "all": all.code(),
                "this": this.iter().map(|t| t.name()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let per_plugin: Vec<&str> =
        (0..plugins.len()).map(|i| worst_this(field_this.iter().map(|t| t[i])).name()).collect();
    Ok(serde_json::json!({
        "plugins": plugins,
        "read": records.iter().map(Option::is_some).collect::<Vec<_>>(),
        "fields": fields,
        "all": worst_all(field_all).code(),
        "this": per_plugin,
    })
    .to_string())
}

/// Wraithguard: a conflicting `LAND` record's terrain in each plugin, and what each conflict
/// strategy would make of it (`_compare_merge_strategies`, `landcore::preview`): the first
/// plugin with heights the base, the rest merged in load order. `id` is the scan's `(x, y)`.
/// Answers `{plugins: [{name, heights}], strategies: [{name, heights}]}` (65x65, world units).
#[tauri::command(async)]
fn conflicts_land_surfaces(cfg: String, id: String) -> Result<String, String> {
    let plugins: Vec<String> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().find(|c| c.rtype == "Landscape" && c.id == id).map(|c| c.plugins.clone()).ok_or("that cell is not in the last scan")?
    };
    let nums: Vec<i64> = id.trim_matches(|c| c == '(' || c == ')').split(',').filter_map(|n| n.trim().parse().ok()).collect();
    let [gx, gy] = nums[..] else { return Err(format!("{id} is not a cell")) };
    let setup = cf_setup(&cfg)?;
    let mut surfaces: Vec<(String, Vec<i64>)> = Vec::new();
    for (name, path) in setup.locate(&plugins) {
        let Some(path) = path else { continue };
        let Ok(records) = landcore::read::read_landscape_records(&path) else { continue };
        let found = records.iter().rev().find(|r| matches!(r, landcore::read::Record::Land { grid, .. } if *grid == (gx, gy)));
        let layers = found.and_then(|r| landcore::layers::LandscapeLayers::from_record(r).ok().flatten());
        if let Some(h) = layers.and_then(|l| l.heights) {
            surfaces.push((name, h));
        }
    }
    if surfaces.len() < 2 {
        return Err("Only one plugin edits this cell's terrain, so every strategy gives the same result. Strategy choice only matters where two or more plugins change the same ground.".into());
    }
    let grids: Vec<Vec<i64>> = surfaces.iter().map(|(_, h)| h.clone()).collect();
    let strategies: Vec<serde_json::Value> = landcore::preview::PREVIEW_STRATEGIES
        .iter()
        .filter_map(|s| landcore::preview::merge_preview(&grids, *s).ok().map(|h| serde_json::json!({"name": s.name(), "heights": h})))
        .collect();
    Ok(serde_json::json!({
        "plugins": surfaces.iter().map(|(n, h)| serde_json::json!({"name": n, "heights": h})).collect::<Vec<_>>(),
        "strategies": strategies,
    })
    .to_string())
}

/// Wraithguard: the loose-file scan (`detect_resource_conflicts`) over the cfg's data
/// folders, and the mesh analysis of those that differ (`analyse_mesh_conflicts`; `meshes`
/// false: none; `meshLimit` 0: all). The user's own folders are `subsetDirs` and those the
/// Load order page's plan file names (`planFile`, its `customDataDirs`; `""`: none). Answers
/// `{list: [{path, providers, winner, involvesSubset, identical, mesh, meshColumns}], stats,
/// meshStats}`.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
fn conflicts_resources(cfg: String, subset_dirs: Vec<String>, exclude_exts: Vec<String>, compare: bool, meshes: bool, mesh_limit: usize, plan_file: String) -> Result<String, String> {
    let mut subset_dirs = subset_dirs;
    let plan: Option<serde_json::Value> =
        if plan_file.is_empty() { None } else { std::fs::read_to_string(&plan_file).ok().and_then(|t| serde_json::from_str(&t).ok()) };
    if let Some(plan) = plan {
        subset_dirs.extend(plan["customDataDirs"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)));
    }
    let setup = cf_setup(&cfg)?;
    let dirs: Vec<String> = setup.data.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let (list, stats) = conflictcore::resources::detect(&dirs, &subset_dirs, &exclude_exts, compare);
    let (mesh, mstats) = if meshes { conflictcore::mesh::analyse(&list, mesh_limit) } else { (vec![None; list.len()], conflictcore::mesh::MeshStats::default()) };
    let rows: Vec<serde_json::Value> = list
        .iter()
        .zip(&mesh)
        .map(|(c, m)| {
            serde_json::json!({"path": c.path, "providers": c.providers, "winner": c.winner, "involvesSubset": c.involves_subset,
                               "identical": c.identical, "mesh": m.as_ref().map(conflictcore::mesh::Finding::describe),
                               "meshColumns": m.as_ref().map(|f| { let (a, b, c, d) = f.columns(); [a, b, c, d] })})
        })
        .collect();
    let out = serde_json::json!({
        "list": rows,
        "stats": {"dirs": stats.dirs, "files": stats.files, "conflicts": stats.conflicts, "identical": stats.identical, "differing": stats.differing},
        "meshStats": {"analysed": mstats.analysed, "findings": mstats.findings, "unreadable": mstats.unreadable},
    });
    *RESOURCES.lock().unwrap_or_else(|p| p.into_inner()) = Some(ResourceScan { list, stats, mesh });
    Ok(out.to_string())
}

/// Bytes as standard base64, for the page's `atob`.
fn cf_b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

/// RGBA decoded for comparing: what the viewer's texture reader gives as straight RGBA or
/// blocks (the first level), else the reason.
fn cf_rgba(bytes: &[u8], path: &str) -> Result<conflictcore::textures::Rgba, String> {
    use viewcore::img::Kind;
    let t = viewcore::img::read(bytes, path, false)?;
    let (w, h, px) = t.levels.into_iter().next().ok_or("no image in the file")?;
    let pixels = match t.kind {
        Kind::Rgba => px,
        Kind::File | Kind::Volume => return Err("this format is not decoded here".into()),
        k => viewcore::img::decode_blocks(&px, w, h, k),
    };
    Ok(conflictcore::textures::Rgba { width: w as usize, height: h as usize, pixels })
}

/// Wraithguard: a texture conflict's pair compared (`_texture_compare_payload`): the file the
/// winner replaced and the winner. Answers `{left, right: {label, w, h, rgba (base64) | null},
/// verdict, detail, changedShare, worstChannel, meanChannel, diff: {w, h, rgba} | null}`, the
/// images at most `max` on their long side for the page.
#[tauri::command(async)]
fn conflicts_texture(path: String, providers: Vec<String>, max: u32) -> Result<String, String> {
    use conflictcore::textures::{compare_bytes, difference, pair, Verdict};
    let (lo, wi) = pair(&providers).ok_or("one provider: nothing to compare")?;
    let read = |dir: &str| std::fs::read(std::path::Path::new(dir).join(&path)).map_err(|e| format!("{dir}: {e}"));
    let (lb, wb) = (read(lo)?, read(wi)?);
    let c = compare_bytes(&lb, &wb, |b| cf_rgba(b, &path));
    let b64 = cf_b64;
    let side = |dir: &str, bytes: &[u8]| -> serde_json::Value {
        let label = format!("{} / {path}", std::path::Path::new(dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        match cf_rgba(bytes, &path) {
            Ok(i) => {
                let (w, h, px) = viewcore::img::shrink_rgba(i.width as u32, i.height as u32, i.pixels, max);
                serde_json::json!({"label": label, "w": w, "h": h, "rgba": b64(&px)})
            }
            Err(e) => serde_json::json!({"label": label, "w": 0, "h": 0, "rgba": null, "error": e}),
        }
    };
    let diff = if c.verdict == Verdict::Different && c.worst_channel > 0 {
        match (cf_rgba(&lb, &path), cf_rgba(&wb, &path)) {
            (Ok(a), Ok(b)) if a.width == b.width && a.height == b.height => {
                let d = difference(&a.pixels, &b.pixels, 4);
                let (w, h, px) = viewcore::img::shrink_rgba(a.width as u32, a.height as u32, d, max);
                serde_json::json!({"w": w, "h": h, "rgba": b64(&px)})
            }
            _ => serde_json::Value::Null,
        }
    } else {
        serde_json::Value::Null
    };
    Ok(serde_json::json!({
        "left": side(lo, &lb), "right": side(wi, &wb), "verdict": c.verdict.name(), "detail": c.detail,
        "changedShare": c.changed_share, "worstChannel": c.worst_channel, "meanChannel": c.mean_channel, "diff": diff,
    })
    .to_string())
}

/// Wraithguard: the text report of the last scan (`kind`: `records` or `files`), the user's
/// own only with `subsetOnly`, at most `limit` listed (0: all).
#[tauri::command(async)]
fn conflicts_report(kind: String, subset_only: bool, limit: usize) -> Result<String, String> {
    if kind == "files" {
        let slot = RESOURCES.lock().unwrap_or_else(|p| p.into_inner());
        let r = slot.as_ref().ok_or("scan the files first")?;
        let mesh: Vec<Option<conflictcore::resources::MeshColumns>> = r
            .mesh
            .iter()
            .map(|m| m.as_ref().map(|f| conflictcore::resources::MeshColumns { report_note: f.describe(), ..Default::default() }))
            .collect();
        return Ok(serde_json::Value::String(conflictcore::resources::report(&r.list, &r.stats, &mesh, subset_only, limit)).to_string());
    }
    let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
    let c = slot.as_ref().ok_or("scan first")?;
    if c.mode != "conflicts" {
        return Err("the report is of conflicts: scan for conflicts first".into());
    }
    Ok(serde_json::Value::String(conflictcore::records::report(&c.list, &c.stats, subset_only, limit)).to_string())
}

/// Wraithguard: the last scan's CSV (`write_conflict_csv` / `write_resource_csv`) written to
/// `path`.
#[tauri::command(async)]
fn conflicts_csv(kind: String, path: String) -> Result<(), String> {
    let text = if kind == "files" {
        let slot = RESOURCES.lock().unwrap_or_else(|p| p.into_inner());
        let r = slot.as_ref().ok_or("scan the files first")?;
        let mesh: Vec<Option<conflictcore::resources::MeshColumns>> = r
            .mesh
            .iter()
            .map(|m| {
                m.as_ref().map(|f| {
                    let (note, lost_collision, lost_animation, triangle_ratio) = f.columns();
                    conflictcore::resources::MeshColumns { note, lost_collision, lost_animation, triangle_ratio, report_note: String::new() }
                })
            })
            .collect();
        conflictcore::resources::csv(&r.list, &mesh)
    } else {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        conflictcore::records::conflict_csv(&slot.as_ref().ok_or("scan first")?.list)
    };
    write_path_whole(&path, text.as_bytes())
}

/// Wraithguard: queue a decision for the patch in the shared journal (`queue_whole_record`,
/// `queue_field`): `op` `record` (the record from `plugin`), `field` (`field` from `plugin`)
/// or `value` (`field` set to `value`, JSON). Answers how many records the queue holds.
#[tauri::command(async)]
fn conflicts_patch(journal: String, op: String, rtype: String, id: String, plugin: String, field: String, value: String) -> Result<usize, String> {
    use editcore::queue::{Choice, PatchQueue, Selection};
    if journal.is_empty() {
        return Err("no patch journal: open this from Wraithguard".into());
    }
    let path = std::path::Path::new(&journal);
    let mut q = PatchQueue::new();
    q.restore(path);
    match op.as_str() {
        "record" => q.add_whole(Selection { plugin, record_type: rtype, key: id }),
        "field" => q.add_field(&rtype, &id, Choice::Plugin { path: field, plugin }),
        "value" => {
            conflictcore::patch::can_define(&field).map_err(|r| r.message())?;
            let value: serde_json::Value = serde_json::from_str(&value).map_err(|e| format!("the value did not read: {e}"))?;
            q.add_field(&rtype, &id, Choice::Value { path: field, value });
        }
        _ => return Err(format!("no patch step {op:?}")),
    }
    q.save(path).map_err(|e| format!("the journal could not be written: {e}"))?;
    Ok(q.len())
}

/* ---- Merge Settings (Wraithguard's `.mergedlands.toml` editor; port step 4.4) -----------

   The per-plugin Merge Lands settings file beside a plugin (`landcore::meta`): read
   (`land_meta_read`), previewed (`land_meta_preview`) and written (`land_meta_write`). */

/// The layers as the page sends them, `[{name, included, strategy}]`.
fn land_meta_of(meta_type: &str, layers: &[serde_json::Value]) -> Result<landcore::meta::PluginMeta, String> {
    use landcore::meta::{MergeSettings, PluginMeta, Strategy, META_TYPES};
    if !META_TYPES.contains(&meta_type) {
        return Err(format!("unknown meta_type {meta_type:?}"));
    }
    let layers = layers
        .iter()
        .map(|l| {
            let name = l["name"].as_str().unwrap_or_default().to_string();
            let strategy = l["strategy"].as_str().unwrap_or("Auto");
            let conflict_strategy = Strategy::from_name(strategy).ok_or_else(|| format!("unknown strategy {strategy:?}"))?;
            Ok((name, MergeSettings { included: l["included"].as_bool().unwrap_or(true), conflict_strategy }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PluginMeta { meta_type: meta_type.to_string(), layers })
}

fn land_meta_json(meta: &landcore::meta::PluginMeta) -> serde_json::Value {
    serde_json::json!({
        "metaType": meta.meta_type,
        "layers": meta.full().iter().map(|(n, s)| serde_json::json!({"name": n, "included": s.included, "strategy": s.conflict_strategy.name()})).collect::<Vec<_>>(),
    })
}

/// Wraithguard: a plugin's `.mergedlands.toml` (`_open_merge_settings_editor`): `{path, exists,
/// error, metaType, layers: [{name, included, strategy}], metaTypes, strategies}`. No file, or
/// one that will not read (`error` says why), starts from a `Patch` at the defaults.
#[tauri::command(async)]
fn land_meta_read(plugin: String) -> Result<String, String> {
    use landcore::meta;
    let plugin = std::path::Path::new(&plugin);
    let path = meta::meta_path_for(plugin);
    let exists = path.is_file();
    let fresh = meta::PluginMeta { meta_type: meta::META_PATCH.into(), layers: Vec::new() };
    let (m, error) = if exists {
        match meta::load_meta(plugin) {
            Ok(m) => (m, String::new()),
            Err(e) => (fresh, e.0),
        }
    } else {
        (fresh, String::new())
    };
    let mut out = land_meta_json(&m);
    out["path"] = path.display().to_string().into();
    out["exists"] = exists.into();
    out["error"] = error.into();
    out["metaTypes"] = serde_json::json!(meta::META_TYPES);
    out["strategies"] = serde_json::json!(meta::STRATEGY_NAMES);
    Ok(out.to_string())
}

/// Wraithguard: the file the editor would write (`_ms_refresh_preview`), every layer spelled out
/// (a JSON string: the page's `Engine.call` reads every answer as JSON).
#[tauri::command(async)]
fn land_meta_preview(meta_type: String, layers: Vec<serde_json::Value>) -> Result<String, String> {
    Ok(serde_json::Value::String(landcore::meta::render_meta(&land_meta_of(&meta_type, &layers)?, true)).to_string())
}

/// Wraithguard: the `.mergedlands.toml` written beside `plugin` (`_ms_write`, `write_settings`),
/// header and every layer; answers its path.
#[tauri::command(async)]
fn land_meta_write(plugin: String, meta_type: String, layers: Vec<serde_json::Value>) -> Result<String, String> {
    let m = land_meta_of(&meta_type, &layers)?;
    landcore::meta::write_settings(std::path::Path::new(&plugin), &m).map(|p| serde_json::Value::String(p.display().to_string()).to_string()).map_err(|e| e.0)
}

/// Wraithguard: where Merge Lands writes by default (`_merged_lands_home`): the data folder
/// holding most of the load order. Answers `{folders, home, name, exists}` (`exists`: a
/// `Merged Lands.esp` is already there).
#[tauri::command(async)]
fn land_merge_home(cfg: String) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let folders: Vec<PathBuf> = setup.data.iter().filter(|d| d.is_dir()).cloned().collect();
    let mut best: Option<&PathBuf> = None;
    let mut best_count = 0;
    for f in &folders {
        let count = order.iter().filter(|n| f.join(n).is_file()).count();
        if count > best_count {
            best = Some(f);
            best_count = count;
        }
    }
    let name = landcore::service::DEFAULT_NAME;
    Ok(serde_json::json!({
        "folders": folders.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "home": best.map(|b| b.display().to_string()),
        "name": name,
        "exists": best.is_some_and(|b| b.join(name).is_file()),
        "ignore": landcore::service::DEFAULT_IGNORE,
    })
    .to_string())
}

/// Wraithguard: Merge Lands (`build_merged_lands`, landcore::service) over the cfg's load
/// order and data folders, `Merged Lands.esp` written into `folder` (`dryRun`: nothing
/// written). `model` `"masters"` measures each plugin against the masters, `"loadorder"`
/// against the load order before it (the OpenMW fork); `ignore` the plugins left out by name
/// or folder (`None`: the fork's defaults). Answers `{error, lines, notes, output, result: {cells, textures, contested, major,
/// seams, slopes, clamped, dropped, borrowed}}` - `error` the reason it stopped, the lines said
/// before it kept.
#[tauri::command(async)]
fn land_merge(cfg: String, folder: String, dry_run: bool, verbose: bool, model: String, ignore: Option<Vec<String>>) -> Result<String, String> {
    use landcore::pipeline::DiffModel;
    use landcore::service::{build_merged_lands, MergeOptions, DEFAULT_NAME};
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let dirs: Vec<PathBuf> = setup.data.iter().filter(|d| d.is_dir()).cloned().collect();
    let output = (!folder.is_empty()).then(|| PathBuf::from(&folder).join(DEFAULT_NAME));
    let model = if model == "loadorder" { DiffModel::LoadOrder } else { DiffModel::Masters };
    let mut opts = MergeOptions { output, dry_run, verbose, model, ..Default::default() };
    if let Some(ignore) = ignore {
        opts.ignore = ignore;
    }
    let out = match build_merged_lands(&dirs, &order, &opts, &mut |_| {}) {
        Ok(r) => serde_json::json!({
            "error": null, "lines": r.lines, "notes": r.notes,
            "output": r.output.map(|p| p.display().to_string()),
            "result": {"cells": r.cells_written, "textures": r.textures_written, "contested": r.contested, "major": r.major,
                       "seams": r.seam_vertices, "slopes": r.slope_vertices, "clamped": r.clamped, "dropped": r.dropped, "borrowed": r.borrowed},
        }),
        Err((error, lines)) => serde_json::json!({"error": error, "lines": lines, "notes": [], "output": null, "result": null}),
    };
    Ok(out.to_string())
}

/* ---- the Help page (Wraithguard's `viz/docs.py`; port step 4.4) -------------------------

   The program's documents (`help_docs`), one rendered (`help_doc`, `toolscore::docs`), and a
   web link in one opened in the browser (`help_open_link`). `dirs` is where Wraithguard
   says the documents are (`extra.docsDir`); the program's own folder and those above it
   follow. */

fn help_dirs(dirs: &[String]) -> Vec<PathBuf> {
    toolscore::docs::search_dirs(dirs, std::env::current_exe().ok().as_deref())
}

/// Wraithguard: the documents the Help offers (`HELP_DOCUMENTS`), `[{name, title, found}]`.
#[tauri::command(async)]
fn help_docs(dirs: Vec<String>) -> Result<String, String> {
    let search = help_dirs(&dirs);
    let out: Vec<serde_json::Value> = toolscore::docs::DOCUMENTS
        .iter()
        .map(|(name, title)| serde_json::json!({"name": name, "title": title, "found": toolscore::docs::locate(name, &search).is_some()}))
        .collect();
    Ok(serde_json::Value::Array(out).to_string())
}

/// Wraithguard: one document rendered (`_open_document`, `render_markdown`): `name` a `.md`
/// file name, no folders. Answers `{name, title, path, body, headings: [[level, text, id]]}`.
#[tauri::command(async)]
fn help_doc(name: String, dirs: Vec<String>) -> Result<String, String> {
    if name.contains(['/', '\\']) || name.starts_with('.') || !name.to_lowercase().ends_with(".md") {
        return Err(format!("{name} is not one of the program's documents"));
    }
    let path = toolscore::docs::locate(&name, &help_dirs(&dirs))
        .ok_or_else(|| format!("{name} was not found next to the program. It ships alongside the app; in a checkout it is in the project folder."))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{name} could not be read: {e}"))?;
    let r = toolscore::docs::render(&String::from_utf8_lossy(&bytes));
    let title = toolscore::docs::DOCUMENTS.iter().find(|(n, _)| *n == name).map(|(_, t)| t.to_string()).unwrap_or_else(|| name.clone());
    Ok(serde_json::json!({
        "name": name, "title": title, "path": path.display().to_string(), "body": r.body,
        "headings": r.headings.iter().map(|(l, t, i)| serde_json::json!([l, t, i])).collect::<Vec<_>>(),
    })
    .to_string())
}

/// Wraithguard: a document's web link, in the system's browser (http, https and mailto only).
#[tauri::command(async)]
fn help_open_link(url: String) -> Result<(), String> {
    let low = url.to_lowercase();
    if !(low.starts_with("http://") || low.starts_with("https://") || low.starts_with("mailto:")) {
        return Err(format!("{url} is not a web link"));
    }
    if std::env::var_os("GARDENFELL_NO_OPEN").is_some() {
        return Ok(());
    }
    viewcore::profiles::show_folder(std::path::Path::new(&url))
}

/// The Editor's "Preview it in a new Cell Preview" (`editPreviewPatch`, which Wraithguard
/// used to answer; port step 4.1): a new viewer window on the setup with `path` loaded last,
/// on the plugin's cells. The setup is written beside `cfg` (so its relative paths still
/// read), as `wraithguard_preview_setup.cfg`; the window keeps this one's profile and theme.
#[tauri::command(async)]
fn editor_preview_patch(cfg: String, path: String) -> Result<(), String> {
    let target = editcore::buildjob::preview_target(&serde_json::json!({"path": path}))?;
    let cfg_path = PathBuf::from(&cfg);
    let text = std::fs::read_to_string(&cfg_path).map_err(|e| format!("{cfg}: {e}"))?;
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let lines = editcore::buildjob::with_extra_plugins(&lines, std::slice::from_ref(&target));
    let setup = cfg_path.with_file_name("wraithguard_preview_setup.cfg");
    write_path_whole(&setup.to_string_lossy(), (lines.join("\n") + "\n").as_bytes())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut args: Vec<String> = vec!["--cell-viewer".into(), "--openmw-cfg".into(), setup.to_string_lossy().into_owned()];
    let mine: Vec<String> = std::env::args().collect();
    for keep in ["--prefs", "--theme"] {
        if let Some(v) = mine.iter().position(|a| a == keep).and_then(|i| mine.get(i + 1)) {
            args.extend([keep.to_string(), v.clone()]);
        }
    }
    if !name.is_empty() && !name.contains(['\r', '\n', '\0']) {
        args.extend(["--cell".to_string(), format!("plugin:{name}")]);
    }
    args.extend(["--title".to_string(), "Cell Preview".to_string()]);
    std::process::Command::new(exe).args(&args).spawn().map(|_| ()).map_err(|e| format!("a new Cell Preview could not be started: {e}"))
}

/* ---- the plugin view (Wraithguard's `pluginview.py`; port step 4.4) ----------------------

   The last record scan as plugin -> record type -> records (`conflicts_by_plugin`), a type
   judged for one plugin on demand (`conflicts_plugin_group`), and a plugin's fields taken
   across every record it shares (`conflicts_plugin_bulk`, `_apply`). */

/// The scan's records each plugin of `plugins` defines, read once per plugin file: `(plugin,
/// type, id)` to the record.
fn cf_read_many(
    setup: &viewcore::ordercore::cfg::OpenmwCfg,
    list: &[&conflictcore::records::Conflict],
) -> std::collections::HashMap<(String, String, String), serde_json::Value> {
    use std::collections::{HashMap, HashSet};
    let mut wanted: Vec<(String, HashSet<(String, String)>)> = Vec::new();
    for c in list {
        for p in &c.plugins {
            let key = (c.rtype.clone(), c.id.clone());
            match wanted.iter_mut().find(|(n, _)| n == p) {
                Some((_, w)) => {
                    w.insert(key);
                }
                None => wanted.push((p.clone(), std::iter::once(key).collect())),
            }
        }
    }
    let names: Vec<String> = wanted.iter().map(|(n, _)| n.clone()).collect();
    let at: HashMap<String, PathBuf> = setup.locate(&names).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let read = |(name, keys): &(String, HashSet<(String, String)>)| -> Vec<((String, String, String), serde_json::Value)> {
        let Some(bytes) = at.get(name).and_then(|p| std::fs::read(p).ok()) else { return Vec::new() };
        conflictcore::fields::records_of(&bytes, keys)
            .map(|m| m.into_iter().map(|((t, i), v)| ((name.clone(), t, i), v)).collect())
            .unwrap_or_default()
    };
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    let chunk = wanted.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let jobs: Vec<_> = wanted.chunks(chunk).map(|part| s.spawn(move || part.iter().flat_map(read).collect::<Vec<_>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    })
}

/// Wraithguard: the last record scan as plugin -> type (`group_by_plugin`), plugins in the
/// cfg's load order. Answers `[{plugin, records, mine, groups: [{type, count}]}]`.
#[tauri::command(async)]
fn conflicts_by_plugin(cfg: String, subset: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
    let scan = slot.as_ref().ok_or("scan first")?;
    let mine: std::collections::HashSet<String> = subset.iter().map(|s| s.to_lowercase()).collect();
    let out: Vec<serde_json::Value> = conflictcore::summary::group_by_plugin(&scan.list, &setup.plugins())
        .iter()
        .map(|b| {
            serde_json::json!({
                "plugin": b.plugin, "records": b.records(), "mine": mine.contains(&b.plugin.to_lowercase()),
                "groups": b.groups.iter().map(|(t, m)| serde_json::json!({"type": t, "count": m.len()})).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(serde_json::Value::Array(out).to_string())
}

/// Wraithguard: one plugin's records of one type in the last scan (`rtype` `""`: every
/// type), each judged across its plugins (`_judge_group`, `record_status`,
/// `record_plugin_statuses`). Answers `{records: [{type, id, plugins, winner, all, this}],
/// tally: {status: count}, worst}` - `all` the record's `ConflictAll` (0-4), `this` and
/// `worst` the plugin's `ConflictThis` names.
#[tauri::command(async)]
fn conflicts_plugin_group(cfg: String, plugin: String, rtype: String) -> Result<String, String> {
    use conflictcore::summary::{judge, tally};
    let setup = cf_setup(&cfg)?;
    let list: Vec<conflictcore::records::Conflict> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().filter(|c| (rtype.is_empty() || c.rtype == rtype) && c.plugins.contains(&plugin)).cloned().collect()
    };
    let refs: Vec<&conflictcore::records::Conflict> = list.iter().collect();
    let read = cf_read_many(&setup, &refs);
    let mut statuses = Vec::new();
    let records: Vec<serde_json::Value> = list
        .iter()
        .map(|c| {
            let versions: Vec<(String, Option<&serde_json::Value>)> =
                c.plugins.iter().map(|p| (p.clone(), read.get(&(p.clone(), c.rtype.clone(), c.id.clone())))).collect();
            let (all, this) = judge(&versions);
            let mine = c.plugins.iter().position(|p| *p == plugin).map(|i| this[i]).unwrap_or(conflictcore::fields::ConflictThis::Unknown);
            statuses.push(mine);
            serde_json::json!({"type": c.rtype, "id": c.id, "plugins": c.plugins, "winner": c.winner, "all": all.code(), "this": mine.name()})
        })
        .collect();
    let mut counts = Vec::new();
    tally(statuses.iter().copied(), &mut counts);
    let worst = conflictcore::fields::worst_this(statuses.iter().copied());
    let tally: serde_json::Map<String, serde_json::Value> = counts.into_iter().map(|(s, n)| (s.name().to_string(), n.into())).collect();
    Ok(serde_json::json!({"records": records, "tally": tally, "worst": worst.name()}).to_string())
}

/// The records `conflicts_plugin_bulk` read, kept for `_apply`: the plugin and its records.
static BULK: Mutex<Option<(String, Vec<conflictcore::summary::BulkRecord>)>> = Mutex::new(None);

/// Wraithguard: Merge fields from a plugin (`_bulk_merge_selected_plugin`): reads every record
/// of the last scan the plugin shares with another, and answers the fields taking would
/// change, `[{path, count, value}]` (`count` the records it would change, `value` the
/// plugin's from the first of them).
#[tauri::command(async)]
fn conflicts_plugin_bulk(cfg: String, plugin: String) -> Result<String, String> {
    use conflictcore::summary::{bulk_candidates, BulkRecord};
    let setup = cf_setup(&cfg)?;
    let list: Vec<conflictcore::records::Conflict> = {
        let slot = CONFLICTS.lock().unwrap_or_else(|p| p.into_inner());
        let scan = slot.as_ref().ok_or("scan first")?;
        scan.list.iter().filter(|c| c.plugins.len() >= 2 && c.plugins.contains(&plugin)).cloned().collect()
    };
    if list.is_empty() {
        return Err(format!("{plugin} shares no record with another plugin, so there is nothing to take."));
    }
    let refs: Vec<&conflictcore::records::Conflict> = list.iter().collect();
    let mut read = cf_read_many(&setup, &refs);
    let records: Vec<BulkRecord> = list
        .iter()
        .map(|c| BulkRecord {
            rtype: c.rtype.clone(),
            key: c.id.clone(),
            plugins: c.plugins.clone(),
            values: c
                .plugins
                .iter()
                .filter_map(|p| read.remove(&(p.clone(), c.rtype.clone(), c.id.clone())).map(|v| (p.clone(), conflictcore::fields::flatten(&v))))
                .collect(),
        })
        .collect();
    let out: Vec<serde_json::Value> = bulk_candidates(&records, &plugin)
        .into_iter()
        .map(|(path, count, value)| serde_json::json!({"path": path, "count": count, "value": value}))
        .collect();
    *BULK.lock().unwrap_or_else(|p| p.into_inner()) = Some((plugin, records));
    Ok(serde_json::Value::Array(out).to_string())
}

/// Wraithguard: queue the chosen fields from the plugin `conflicts_plugin_bulk` read
/// (`apply_choices`, `bulk_field_choices`) into the patch journal; `dry` only counts. Answers
/// how many field takes that is.
#[tauri::command(async)]
fn conflicts_plugin_bulk_apply(journal: String, plugin: String, paths: Vec<String>, dry: bool) -> Result<usize, String> {
    use editcore::queue::{Choice, PatchQueue};
    let slot = BULK.lock().unwrap_or_else(|p| p.into_inner());
    let (read_for, records) = slot.as_ref().ok_or("read the plugin first")?;
    if *read_for != plugin {
        return Err("read the plugin first".into());
    }
    let takes = conflictcore::summary::bulk_takes(records, &plugin, &paths);
    if dry {
        return Ok(takes.len());
    }
    if journal.is_empty() {
        return Err("no patch journal: open this from Wraithguard".into());
    }
    let path = std::path::Path::new(&journal);
    let mut q = PatchQueue::new();
    q.restore(path);
    for t in &takes {
        q.add_field(&t.rtype, &t.key, Choice::Plugin { path: t.path.clone(), plugin: plugin.clone() });
    }
    q.save(path).map_err(|e| format!("the journal could not be written: {e}"))?;
    Ok(takes.len())
}

/* ---- the Tools page (Wraithguard; port step 4.4) ----------------------------------------

   Wraithguard's tools that work on the whole load order, answered by `toolscore`, over the
   load order the launch's openmw.cfg names: the lint (`tools_lint`), the backups
   (`tools_backups`, `_restore`, `_delete`), Remove / Rename Master (`tools_masters`,
   `tools_remove_master`, `tools_rename_master`), Save Check (`tools_save_check`), the Journal view
   (`tools_journal`), tes3cmd (`tools_tes3cmd`, `_find`, `tools_locate`). */

/// Wraithguard: the load-order lint (`lint_plugins`): `subset` the user's own plugins (their
/// header and script checks, and the `[your mod]` tag). Answers `{warnings, stats: {scanned,
/// unreadable, warnings, interiorCells, pathgrids}}`.
#[tauri::command(async)]
fn tools_lint(cfg: String, subset: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let at: std::collections::HashMap<String, PathBuf> = setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let find = move |name: &str| at.get(name).cloned();
    let origins: std::collections::BTreeMap<String, String> = subset.iter().map(|s| (s.to_lowercase(), "your mod".to_string())).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    let (warnings, st) = toolscore::lint::lint_order(&order, &find, &subset, &origins, threads);
    Ok(serde_json::json!({
        "warnings": warnings,
        "stats": {"scanned": st.scanned, "unreadable": st.unreadable, "warnings": st.warnings, "interiorCells": st.interior_cells, "pathgrids": st.pathgrids},
    })
    .to_string())
}

/// Wraithguard: the backups the tools leave (`scan_backups`, `toolscore::backups`) in the
/// cfg's data folders, the cfg's own folder and `dirs` (the rule files' and plugin-order.yml's
/// folders), four folders deep. Answers `[{path, original, kind, originalExists}]`.
#[tauri::command(async)]
fn tools_backups(cfg: String, dirs: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let mut roots: Vec<PathBuf> = setup.data.clone();
    roots.extend(dirs.into_iter().filter(|d| !d.is_empty()).map(PathBuf::from));
    if let Some(parent) = std::path::Path::new(&cfg).parent() {
        roots.push(parent.to_path_buf());
    }
    let found = toolscore::backups::scan(&roots, 4);
    Ok(serde_json::Value::from(
        found
            .iter()
            .map(|b| serde_json::json!({"path": b.path, "original": b.original, "kind": b.kind, "originalExists": b.original.exists()}))
            .collect::<Vec<_>>(),
    )
    .to_string())
}

/// Wraithguard: backups copied over their originals (`[[backup, original]]`). Answers `{restored,
/// failed: [[path, why]]}`.
#[tauri::command(async)]
fn tools_backups_restore(pairs: Vec<(String, String)>) -> Result<String, String> {
    let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).collect();
    let (ok, failed) = toolscore::backups::restore(&pairs);
    Ok(serde_json::json!({"restored": ok, "failed": failed.iter().map(|(p, e)| serde_json::json!([p, e])).collect::<Vec<_>>()}).to_string())
}

/// Wraithguard: backups deleted. Answers how many were.
#[tauri::command(async)]
fn tools_backups_delete(paths: Vec<String>) -> Result<usize, String> {
    Ok(toolscore::backups::delete(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>()))
}

/// Wraithguard: a plugin's masters, `[[name, size]]` (Remove / Rename Master's list).
#[tauri::command(async)]
fn tools_masters(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let list = toolscore::masters::masters(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(serde_json::json!(list).to_string())
}

/// Wraithguard: a master removed from a plugin, its references renumbered
/// (`remove_master_from_bytes`), the original kept once as `.premaster.bak`; `dryRun` reports
/// without writing. Answers `{master, removed, dropped, droppedByCell, remapped, remaining,
/// backup}`.
#[tauri::command(async)]
fn tools_remove_master(path: String, master: String, dry_run: bool) -> Result<String, String> {
    let (r, backup) = toolscore::masters::remove_master_file(std::path::Path::new(&path), &master, dry_run).map_err(|e| format!("{path}: {e}"))?;
    Ok(serde_json::json!({"master": r.master, "removed": r.removed, "dropped": r.references_dropped, "droppedByCell": r.dropped_by_cell,
                          "remapped": r.references_remapped, "remaining": r.remaining_masters, "backup": backup})
    .to_string())
}

/// Wraithguard: a plugin's master `old` pointed at the file `newPath` (its name and size), every
/// reference kept (`rename_master_in_bytes`), the original kept once as `.premaster.bak`.
/// Answers `{renamed, remaining, backup}`.
#[tauri::command(async)]
fn tools_rename_master(path: String, old: String, new_path: String) -> Result<String, String> {
    let (r, backup) = toolscore::masters::rename_master_file(std::path::Path::new(&path), &old, std::path::Path::new(&new_path)).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"renamed": r.renamed, "remaining": r.remaining_masters, "backup": backup}).to_string())
}

/// Wraithguard: Save Check (`toolscore::save`): each save's content files against the load
/// order the cfg names, and `openmw-save-stats`' report - one save, or two compared (older
/// first). `details`: the largest mods detailed; `mods`: those mods instead; `all`: the tables
/// of the whole save too; `dump`: the saved data of the scripts whose path holds these. Answers
/// `{saves: [{name, files, missing, error}], report}`.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
fn tools_save_check(cfg: String, saves: Vec<String>, details: usize, mods: Vec<String>, all: bool, dump: Vec<String>) -> Result<String, String> {
    use toolscore::save;
    if saves.is_empty() || saves.len() > 2 {
        return Err("one save, or two to compare".into());
    }
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let dirs = save::DataDirs::from_cfg(std::path::Path::new(&cfg)).ok();
    let mut checks = Vec::new();
    let mut stats = Vec::new();
    for path in &saves {
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
        let data = std::fs::read(path).map_err(|e| format!("can't read save: {e}"))?;
        match save::content_files(&data) {
            Ok(files) => {
                let missing = save::missing_from(&files, &order);
                checks.push(serde_json::json!({"name": name, "files": files, "missing": missing, "error": null}));
            }
            Err(e) => checks.push(serde_json::json!({"name": name, "files": [], "missing": [], "error": e})),
        }
        stats.push(save::analyse(&name, data, dirs.as_ref(), 4));
    }
    let report = if stats.len() == 2 {
        save::report_diff(&stats[0], &stats[1], 30, &mods)
    } else {
        let opt = save::ReportOptions { details, patterns: mods, full: all, dump, ..Default::default() };
        save::report(&stats[0], &opt)
    };
    Ok(serde_json::json!({"saves": checks, "report": report}).to_string())
}

/// Wraithguard: the Journal view (`editcore::journal`): every quest of the load order the cfg
/// names, its stages by index, and the calls that set each (where, under what conditions, what
/// else they do). Only the topics, responses and scripts are read, the plugins side by side.
#[tauri::command(async)]
fn tools_journal(cfg: String) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let order = setup.plugins();
    let found: Vec<(String, PathBuf)> = setup.locate(&order).into_iter().filter_map(|(n, p)| Some((n, p?))).collect();
    let keep: [[u8; 4]; 3] = [*b"DIAL", *b"INFO", *b"SCPT"];
    let read = |path: &PathBuf| -> Vec<serde_json::Value> {
        let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
        editcore::plugin::objects(&bytes, Some(&keep), false)
            .map(|objs| objs.iter().filter_map(|o| serde_json::to_value(o).ok()).collect())
            .unwrap_or_default()
    };
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    let chunk = found.len().div_ceil(threads).max(1);
    let mut sources: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
    std::thread::scope(|sc| {
        let handles: Vec<_> = found.chunks(chunk).map(|part| sc.spawn(move || part.iter().map(|(n, p)| (n.clone(), read(p))).collect::<Vec<_>>())).collect();
        for h in handles {
            if let Ok(part) = h.join() {
                sources.extend(part);
            }
        }
    });
    Ok(editcore::journal::view(&sources, &order).to_string())
}

/// Wraithguard: tes3cmd's file (`find_tes3cmd`): `explicit` when it is one, else `$MLOX_TES3CMD`,
/// `PATH`, the cfg's folder. Answers its path, or `""`.
#[tauri::command(async)]
fn tools_tes3cmd_find(cfg: String, explicit: String) -> String {
    let dirs: Vec<PathBuf> = std::path::Path::new(&cfg).parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    serde_json::Value::String(toolscore::tes3cmd::find(Some(&explicit), &dirs).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()).to_string()
}

/// Wraithguard: plugins of the load order the cfg names, located (`[[name, path]]`; the path
/// `null` when no data folder has it) - the tes3cmd list's "Add your mods".
#[tauri::command(async)]
fn tools_locate(cfg: String, names: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    Ok(serde_json::json!(setup.locate(&names)).to_string())
}

/// Wraithguard: tes3cmd over `files` (`toolscore::tes3cmd`): `cmd` `clean`, `header`, `custom`
/// (`extra` the whole command) or `sync` (master sizes, done here). `clean` and `custom` run on
/// a copy staged with the plugin's masters and copy back what changed, a backup kept. Answers
/// `{lines, status}`.
#[tauri::command(async)]
fn tools_tes3cmd(cfg: String, exe: String, cmd: String, extra: Vec<String>, files: Vec<String>) -> Result<String, String> {
    let setup = cf_setup(&cfg)?;
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    let mut dirs = setup.data.clone();
    dirs.extend(files.iter().filter_map(|f| f.parent().map(std::path::Path::to_path_buf)));
    let index = viewcore::ordercore::inputs::PluginIndex::build(&dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>());
    let find = |n: &str| index.find(n).map(std::path::Path::to_path_buf);
    if cmd == "sync" {
        let mut lines = vec!["=".repeat(70), " MASTER-SIZE RESYNC (in-app, VFS-aware)".to_string(), "=".repeat(70)];
        let (mut ok, mut fixed, mut fail) = (0, 0, 0);
        for f in &files {
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match toolscore::tes3cmd::sync_master_sizes(f, &find, true) {
                Err(e) => {
                    fail += 1;
                    lines.push(format!("  {name}: ERROR: {e}"));
                }
                Ok((updated, unresolved)) => {
                    ok += 1;
                    if updated.is_empty() {
                        lines.push(format!("  {name}: already in sync"));
                    } else {
                        fixed += 1;
                        lines.extend(updated.iter().map(|(m, a, b)| format!("  {name}: '{m}' {a} -> {b}")));
                    }
                    lines.extend(unresolved.iter().map(|m| format!("  {name}: WARNING: master '{m}' not found in any data folder -- its size field left untouched")));
                }
            }
        }
        let status = format!("Resync: {fixed} plugin(s) updated, {} already in sync, {fail} error(s).", ok - fixed);
        return Ok(serde_json::json!({"lines": lines, "status": status}).to_string());
    }
    let exe_path = PathBuf::from(&exe);
    if !exe_path.is_file() {
        return Err(if exe.is_empty() { "tes3cmd not found. Browse to the compiled tes3cmd.exe (MOMW Tools Pack).".into() } else { format!("'{exe}' does not exist.") });
    }
    if cmd == "custom" && extra.is_empty() {
        return Err("Type a tes3cmd command first, e.g. 'dump --type CELL' or 'fixcells'.".into());
    }
    let argv = toolscore::tes3cmd::invocation(&exe_path)?;
    let staging = std::env::temp_dir().join("wraithguard_tes3cmd_staging");
    let r = toolscore::tes3cmd::run(&argv, &cmd, &extra, &files, &find, &staging, &setup.plugins());
    Ok(serde_json::json!({"lines": r.lines, "status": r.status}).to_string())
}

/* ---- the Editor, answered here (Wraithguard; port step 4.1, slice 7) ------------------

   The Editor's links answered by the viewer's own engine (`editcore::links`), the same JSON
   in and out as Wraithguard's loopback handlers (gui/editorlink.py): the page asks
   `editor_link` for the links `editor_open` names and Wraithguard for the rest. The patch
   queue is shared with Wraithguard's conflict window and Patch Builder through the journal
   both write. One Editor, for the load order the launch named (`extra.editor`). */

static EDITOR: Mutex<Option<std::sync::Arc<editcore::links::Links>>> = Mutex::new(None);

/// Wraithguard: the viewer's Editor opened on a load order (`plugins`: `[[name, path]]`, in
/// order) and the journal it shares with Wraithguard (`""`: none) - kept when it is open on
/// those already. `folders`: the setup's data folders, the highest priority first (where
/// a patch would go); `last`: the patch last written (`""`: none); `testSettings`: where
/// Test in OpenMW's choices are kept (`""`: testing refused). The links it answers, as
/// JSON.
#[tauri::command(async)]
fn editor_open(plugins: Vec<(String, String)>, journal: String, folders: Option<Vec<String>>, last: Option<String>, test_settings: Option<String>) -> Result<String, String> {
    let plugins: Vec<(String, PathBuf)> = plugins.into_iter().map(|(n, p)| (n, PathBuf::from(p))).collect();
    let path = |s: Option<String>| s.filter(|s| !s.is_empty()).map(PathBuf::from);
    let journal = path(Some(journal));
    let mut slot = EDITOR.lock().unwrap_or_else(|p| p.into_inner());
    let same = slot.as_ref().is_some_and(|l| l.plugins == plugins && l.journal == journal);
    if !same {
        let links = editcore::links::Links::open(plugins, journal).with_setup(folders.unwrap_or_default(), path(last), path(test_settings));
        *slot = Some(std::sync::Arc::new(links));
    }
    // Link names are bare words: no escaping to do.
    Ok(format!("[{}]", editcore::links::ANSWERS.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(",")))
}

/// Wraithguard: one Editor link (`name`, its JSON `body`) answered by the viewer's Editor:
/// the answer's text, or the reason it is refused. The verifier looks for the files records
/// name in the install the viewer has open (loose or archived), when it has one.
#[tauri::command(async)]
fn editor_link(state: State, name: String, body: String) -> Result<String, String> {
    let links = EDITOR.lock().unwrap_or_else(|p| p.into_inner()).clone().ok_or_else(|| "The viewer's Editor is not open".to_string())?;
    let vfs = if name == "editVerify" { state.app().vfs.clone() } else { None };
    let exists = |path: &str| vfs.as_ref().is_some_and(|v| v.resolve(path).is_some());
    let file_exists: Option<&dyn Fn(&str) -> bool> = if vfs.is_some() { Some(&exists) } else { None };
    links.call_with(&name, &body, &editcore::links::Ctx { file_exists })
}

/* ---- the Load order page (Wraithguard; port step 4.2, slice 2k) ---------------------

   Sort and Export, as Wraithguard's main window did them, answered by `ordercore::plan`: the
   page sends what the run reads (`loadorder_compute`), gets the plan's orders and the report
   as items it lists and filters, lets the person reorder and opt out, and exports
   (`loadorder_export`), which also writes the plan file the Tk tools still waiting for 4.3 /
   4.4 read their order from. The last plan is kept here between the two. */

static LOADORDER: Mutex<Option<viewcore::ordercore::plan::Computed>> = Mutex::new(None);

fn lo_items(report: &viewcore::ordercore::plan::Report) -> serde_json::Value {
    use viewcore::ordercore::plan::ItemKind;
    report
        .items
        .iter()
        .map(|i| {
            let kind = match i.kind {
                ItemKind::Section => "section",
                ItemKind::Subsection => "subsection",
                ItemKind::Line => "line",
                ItemKind::Log => "log",
                ItemKind::Scans => "scans",
                ItemKind::Preview => "preview",
            };
            serde_json::json!({"kind": kind, "text": i.text, "warning": i.warning, "names": i.names})
        })
        .collect()
}

/// Wraithguard: a whole sort (`ordercore::plan::compute_plan`). `args`: JSON with the fields of
/// `PlanArgs` in camelCase. Answers `{refused, report: [{kind, text, warning, names}], plan}`;
/// the plan is kept for `loadorder_export`.
#[tauri::command(async)]
fn loadorder_compute(args: String) -> Result<String, String> {
    use viewcore::ordercore::plan::{compute_plan, PlanArgs, PlanError};
    let a: serde_json::Value = serde_json::from_str(&args).map_err(|e| format!("the sort's arguments did not read: {e}"))?;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| -> Vec<String> {
        a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    let args = PlanArgs {
        cfg: s("cfg").ok_or("no openmw.cfg given")?,
        customizations: s("customizations"),
        subset: list("subset"),
        subset_file: s("subsetFile"),
        subset_lines: a.get("subsetLines").filter(|v| v.is_array()).map(|_| list("subsetLines")),
        scan_dir: s("scanDir"),
        subset_from_cfg: b("subsetFromCfg"),
        allow_empty_sort: b("allowEmptySort"),
        sort_data_paths: b("sortDataPaths"),
        emit_toml: b("emitToml"),
        groundcover: list("groundcover"),
        plugin_order_yml: s("pluginOrderYml"),
        list_name: s("listName"),
        rules: list("rules"),
        no_predicate_warnings: b("noPredicateWarnings"),
        data_paths_cache: s("dataPathsCache"),
        verbose: b("verbose"),
    };
    match compute_plan(&args) {
        Err(PlanError::Unsupported) => {
            Err("this setup holds something the load order reader does not read (a file that is not UTF-8, or one that could not be opened)".into())
        }
        Err(PlanError::Refused { message, report }) => {
            Ok(serde_json::json!({"refused": message, "report": lo_items(&report), "plan": null}).to_string())
        }
        Ok(got) => {
            let p = &got.plan;
            let plan = serde_json::json!({
                "finalOrder": if got.sorted { serde_json::json!(p.final_order) } else { serde_json::Value::Null },
                "baseOrder": p.base_order_names,
                "subset": p.subset,
                "dataOrder": p.data_order,
                "dataResult": p.data_result.as_ref().map(|r| r.iter().map(|(l, n, v)| serde_json::json!([l, n, v])).collect::<Vec<_>>()),
                "curated": p.curated_set,
                "newGroundcover": p.new_groundcover,
                "orphanData": p.orphan_data_paths,
                "origins": got.subset_origins,
                "masterProblems": got.master_problem_plugins,
                "scanDirs": got.scan_dirs,
            });
            let out = serde_json::json!({"refused": null, "report": lo_items(&got.report), "plan": plan}).to_string();
            *LOADORDER.lock().unwrap_or_else(|p| p.into_inner()) = Some(got);
            Ok(out)
        }
    }
}

/// Wraithguard: export the last sort (`ordercore::plan::export`). `args`: JSON with `cfg`,
/// `writeCfg`, `emitToml`, `dryRun`, `noBackup`, `sortDataPaths`, `listName`, `stamp`,
/// `finalOrder` / `dataOrder` (the panels' enabled entries; null: the plan's),
/// `disabledPlugins`, `disabledData`, and `planFile` (where the Tk tools read the order from;
/// empty: not written). Answers `{report, wroteCfg, wroteToml, toml}`.
#[tauri::command(async)]
fn loadorder_export(args: String) -> Result<String, String> {
    use viewcore::ordercore::plan::{export, Counts, ExportArgs};
    let a: serde_json::Value = serde_json::from_str(&args).map_err(|e| format!("the export's arguments did not read: {e}"))?;
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string);
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let list = |k: &str| -> Option<Vec<String>> {
        a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    };
    let slot = LOADORDER.lock().unwrap_or_else(|p| p.into_inner());
    let got = slot.as_ref().ok_or("sort first: there is no plan to export")?;
    let ex = ExportArgs {
        write_cfg: b("writeCfg"),
        cfg: s("cfg").ok_or("no openmw.cfg given")?,
        emit_toml: s("emitToml"),
        dry_run: b("dryRun"),
        no_backup: b("noBackup"),
        sort_data_paths: b("sortDataPaths"),
        list_name: s("listName"),
        stamp: s("stamp").unwrap_or_else(|| "backup".into()),
        toml_line_ending: None,
    };
    let counts = Counts { rule_warnings: got.predicate_warnings.len(), yml_warnings: got.yml_warnings.len() };
    let final_order = list("finalOrder");
    let data_order = list("dataOrder");
    let done = export(
        &ex,
        &got.plan,
        counts,
        final_order.as_deref(),
        data_order.as_deref(),
        &list("disabledPlugins").unwrap_or_default(),
        &list("disabledData").unwrap_or_default(),
    )
    .map_err(|e| match e {
        viewcore::ordercore::export::ExportError::Unsupported => "the export holds something the writer does not write".to_string(),
        viewcore::ordercore::export::ExportError::Io(e) => format!("the export could not write: {e}"),
    })?;
    if let Some(path) = s("planFile") {
        let order = final_order.clone().unwrap_or_else(|| got.plan.final_order.clone());
        let data = data_order.clone().unwrap_or_else(|| {
            got.plan.data_result.as_ref().map(|r| r.iter().map(|(l, _, _)| l.clone()).collect()).unwrap_or_else(|| got.plan.data_order.clone())
        });
        let file = serde_json::json!({
            "version": 1,
            "cfg": ex.cfg,
            "finalOrder": order,
            "dataLines": data,
            "subset": got.plan.subset,
            "scanDirs": got.scan_dirs,
            "customDataDirs": got.custom_data_dirs,
            "newDataLines": got.plan.data_result.iter().flatten().filter(|(_, n, _)| *n).map(|(l, _, _)| l.clone()).collect::<Vec<_>>(),
            "masterProblems": got.master_problem_plugins,
            "orphanData": got.plan.orphan_data_paths,
            "newGroundcover": got.plan.new_groundcover,
        });
        write_path_whole(&path, file.to_string().as_bytes())?;
    }
    Ok(serde_json::json!({
        "report": lo_items(&done.report),
        "wroteCfg": done.written.wrote_cfg,
        "wroteToml": done.written.wrote_toml,
        "toml": done.written.toml_text,
    })
    .to_string())
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
fn loadorder_scan(dir: String) -> Result<String, String> {
    let scan = viewcore::ordercore::inputs::scan_mod_directories(&dir);
    Ok(serde_json::json!({"lines": scan.lines, "folders": scan.folders, "plugins": scan.plugins, "start": scan.start}).to_string())
}

/* ---- the Load order page: Update rules, Sources and the rule maker ------------------

   The downloads (`ordercore::updates`, as Wraithguard's Update rules and Sources do them) over
   `ureq`: HTTPS in plain Rust, the size capped as the Python's `_get` caps it; and the rule
   maker (`ordercore::authoring`) on its plain JSON form. */

/// The network for `ordercore::updates`: one GET, at most `max_bytes` of the body.
struct Net;

impl viewcore::ordercore::updates::Transport for Net {
    fn get(&self, url: &str, timeout_secs: u64, max_bytes: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
            .build()
            .into();
        let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(max_bytes as u64)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// How long a download may take, as the Python's default.
const LO_TIMEOUT: u64 = 30;

fn lo_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn lo_update_err(e: viewcore::ordercore::updates::UpdateError) -> String {
    match e {
        viewcore::ordercore::updates::UpdateError::Unsupported => "the download holds something the updater does not read".into(),
        viewcore::ordercore::updates::UpdateError::Io(e) => format!("could not write the download: {e}"),
    }
}

/// Runs `f` with the update flows' outside world: the `.bak` stamp and the three variables.
fn lo_with_env<T>(stamp: &str, f: impl FnOnce(&viewcore::ordercore::updates::Env<'_>) -> T) -> T {
    let (po, rt, host) = (lo_env_var("MLOX_PLUGIN_ORDER_URL"), lo_env_var("MLOX_RULES_URL_TEMPLATE"), lo_env_var("MOMW_API_HOST"));
    let env = viewcore::ordercore::updates::Env {
        stamp,
        plugin_order_url: po.as_deref(),
        rules_url_template: rt.as_deref(),
        momw_api_host: host.as_deref(),
    };
    f(&env)
}

/// Wraithguard: Update rules - each managed rule file downloaded and replaced (a `.bak` of the
/// old). `rules`: the rule files and folders; `stamp`: the person's clock. The report's lines.
#[tauri::command(async)]
fn loadorder_update_rules(rules: Vec<String>, stamp: String) -> Result<String, String> {
    let paths: Vec<PathBuf> = rules.iter().map(PathBuf::from).collect();
    let template = lo_env_var("MLOX_RULES_URL_TEMPLATE");
    let lines = lo_with_env(&stamp, |env| viewcore::ordercore::updates::update_rule_files(&paths, template.as_deref(), LO_TIMEOUT, env, &Net))
        .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: Sources - MOMW's plugin-order.yml downloaded to `path` (checked as MOMW's before
/// it replaces the file). The report's lines.
#[tauri::command(async)]
fn loadorder_update_yml(path: String, stamp: String) -> Result<String, String> {
    let count = |data: &[u8]| viewcore::ordercore::momw::parse_bytes(data).map(|e| e.len()).map_err(|e| e.0);
    let lines = lo_with_env(&stamp, |env| {
        viewcore::ordercore::updates::update_plugin_order_yml(std::path::Path::new(&path), None, LO_TIMEOUT, env, &Net, &count)
    })
    .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: Sources - a curated list's data paths, from MOMW's cfg generator, cached at
/// `path`. The report's lines.
#[tauri::command(async)]
fn loadorder_fetch_data_paths(path: String, list_name: String, stamp: String) -> Result<String, String> {
    let host = lo_env_var("MOMW_API_HOST");
    let lines = lo_with_env(&stamp, |env| {
        viewcore::ordercore::updates::fetch_list_data_paths(std::path::Path::new(&path), &list_name, host.as_deref(), LO_TIMEOUT, env, &Net)
    })
    .map_err(lo_update_err)?;
    Ok(serde_json::Value::from(lines).to_string())
}

/// Wraithguard: the rule maker. `rule`: the plain JSON form (`kind`, `plugins`, `expressions`,
/// `message`, `ref`, ...). `op`: `check` - `{text, problems: [[severity, message, remedy]]}`;
/// `append` - written to the rule file `path`, `{written}`, or the problems as the error.
#[tauri::command(async)]
fn loadorder_author(op: String, rule: String, path: Option<String>) -> Result<String, String> {
    use viewcore::ordercore::authoring;
    let v: serde_json::Value = serde_json::from_str(&rule).map_err(|e| format!("the rule did not read: {e}"))?;
    let mut rule = authoring::rule_from_json(&v).ok_or("the rule is not in the rule maker's form")?;
    // The page's warning rules come as rule text, one expression a line (`[DESC]`, `[SIZE]`,
    // `[VER]`, `[ANY]`/`[ALL]`/`[NOT]` groups or a plain name), read here as the sort reads them.
    if let Some(lines) = v.get("lines").and_then(|l| l.as_array()) {
        rule.expressions = lines
            .iter()
            .filter_map(|l| l.as_str())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(authoring::expr_from_text)
            .collect::<Result<_, _>>()?;
    }
    let declined = |_| "the rule holds something the rule maker does not write".to_string();
    match op.as_str() {
        "check" => {
            let problems = authoring::validate(&rule).map_err(declined)?;
            let text = authoring::render_rule(&rule).unwrap_or_default();
            let problems: Vec<serde_json::Value> =
                problems.into_iter().map(|p| serde_json::json!([p.severity.as_str(), p.message, p.remedy])).collect();
            Ok(serde_json::json!({"text": text, "problems": problems}).to_string())
        }
        "append" => {
            let path = path.filter(|p| !p.is_empty()).ok_or("no rule file to write to")?;
            let written = authoring::append_authored_rule(
                std::path::Path::new(&path),
                &rule,
                viewcore::ordercore::export::LineEnding::platform(),
            )
            .map_err(|e| match e {
                authoring::AppendError::Refused(m) => m,
                authoring::AppendError::Unsupported => "the rule file holds something the rule maker does not write".into(),
                authoring::AppendError::Io(e) => format!("could not write the rule file: {e}"),
            })?;
            Ok(serde_json::json!({"written": written}).to_string())
        }
        other => Err(format!("no such rule maker step: {other}")),
    }
}

/* ---- the cell map's coverage (Wraithguard) -------------------------------------------

   Which plugins touch which cells - Wraithguard's modmapper-style cell map, answered
   from the world the viewer already loaded rather than a second scan. Each exterior grid
   and each interior lists the plugins with a CELL record for it, by index into
   `plugins`, so a large load order does not repeat every name on every cell. */

/// `{plugins:[names], ext:[[x,y,[plugin ix...]]...], int:[[name,[plugin ix...]]...]}`.
#[tauri::command(async)]
fn cell_coverage(state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let ix = |v: &Vec<u32>| format!("[{}]", v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));
    let mut ext: Vec<String> = w
        .cells
        .values()
        .map(|c| (c, &w.history.exterior(c.grid).touched_by))
        .filter(|(_, by)| !by.is_empty())
        .map(|(c, by)| format!("[{},{},{}]", c.grid.0, c.grid.1, ix(by)))
        .collect();
    ext.sort();
    /* Wraithguard: and per exterior, how contested its references are and how many
       plugins edit its ground - the map's other two heat modes. */
    let mut grids: std::collections::BTreeSet<(i32, i32)> = w.cells.keys().copied().collect();
    grids.extend(w.land_edits.keys().copied());
    let extc: Vec<String> = grids
        .into_iter()
        .filter_map(|g| {
            let conf = w.cells.get(&g).map(|c| viewcore::usage::conflicts(&c.refs, &w.history, w.history.exterior(g))).unwrap_or(0);
            let land = w.land_edits.get(&g).map(|v| v.len()).unwrap_or(0);
            (conf > 0 || land > 1).then(|| format!("[{},{},{},{}]", g.0, g.1, conf, land))
        })
        .collect();
    let mut int: Vec<String> = w
        .interiors
        .iter()
        .map(|(k, r)| (r, &w.history.interior(k).touched_by))
        .filter(|(_, by)| !by.is_empty())
        .map(|(r, by)| {
            let mut name = String::new();
            viewcore::json::escape_into(&r.name, &mut name);
            format!("[\"{}\",{}]", name, ix(by))
        })
        .collect();
    int.sort();
    Ok(format!(
        "{{\"plugins\":{},\"ext\":[{}],\"int\":[{}],\"extc\":[{}]}}",
        string_array(&w.plugins),
        ext.join(","),
        int.join(","),
        extc.join(",")
    ))
}

/// Wraithguard: the path grids of the cells asked for - `"x,y"` or `"int:<name>"` - with
/// their points in world units: `[{spec, from, points:[[x,y,z]..], edges:[[a,b]..]}]`.
#[tauri::command(async)]
fn pathgrid(cells: Vec<String>, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }.ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let mut out: Vec<String> = Vec::new();
    for spec in &cells {
        let (pg, origin) = match viewcore::cells::CellKey::parse(spec) {
            Some(viewcore::cells::CellKey::Interior(name)) => (w.pathgrids_int.get(&viewcore::world::room_key(&name)), (0.0, 0.0)),
            Some(viewcore::cells::CellKey::Exterior(x, y)) => (w.pathgrids_ext.get(&(x, y)), (x as f32 * 8192.0, y as f32 * 8192.0)),
            None => (None, (0.0, 0.0)),
        };
        let Some(pg) = pg else { continue };
        let pts: Vec<String> = pg
            .points
            .iter()
            .map(|p| format!("[{},{},{}]", p[0] as f32 + origin.0, p[1] as f32 + origin.1, p[2]))
            .collect();
        let edges: Vec<String> = pg.edges.iter().map(|(a, b)| format!("[{a},{b}]")).collect();
        let mut o = J::obj();
        o.str("spec", spec)
            .str("from", plugin_name(&w, usize::try_from(pg.plugin).ok()))
            .raw("points", &format!("[{}]", pts.join(",")))
            .raw("edges", &format!("[{}]", edges.join(",")));
        out.push(o.done());
    }
    Ok(format!("[{}]", out.join(",")))
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
#[tauri::command(async)]
fn where_used(kind: String, name: String, state: State) -> Result<String, String> {
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
    let place = |p: &viewcore::usage::Place, on_ground: bool| {
        let mut o = J::obj();
        o.str("spec", &p.spec)
            .str("label", &p.label)
            .int("count", p.count as u64)
            .raw("pos", &format!("[{:.1},{:.1},{:.1}]", p.pos[0], p.pos[1], p.pos[2]))
            .num("rot", p.rot as f64)
            .bool("ground", on_ground);
        o.done()
    };
    let mut all: Vec<String> = places.iter().map(|p| place(p, false)).collect();
    all.extend(ground.iter().map(|p| place(p, true)));
    let mut idv: Vec<String> = ids.into_iter().collect();
    idv.sort();
    let mut o = J::obj();
    o.raw("ids", &string_array(&idv)).raw("meshes", &string_array(&meshes)).raw("places", &format!("[{}]", all.join(",")));
    Ok(o.done())
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
    Ok(format!(
        "{{\"activate\":{},\"greetMult\":{}}}",
        g("imaxactivatedist", 192.0),
        g("igreetdistancemultiplier", 6.0)
    ))
}

/// Wraithguard: writes base64 bytes (a screenshot for the report bundle) to `path`.
#[tauri::command(async)]
fn write_b64(path: String, data: String) -> Result<(), String> {
    let bytes = viewcore::json::base64_decode(&data).ok_or_else(|| "not base64".to_string())?;
    write_path_whole(&path, &bytes)
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

/// The mesh viewer's Save: posts the block edits to Wraithguard's loopback handler
/// (`url`, which Wraithguard gave in the extra file) and writes the edited file it
/// answers with to `out`, the path the user picked. Only `http://127.0.0.1:<port>/`
/// is accepted: this reaches Wraithguard, never the network.
#[tauri::command(async)]
fn wg_save_edited(url: String, body: String, out: String) -> Result<String, String> {
    let bytes = loopback_post(&url, body.as_bytes())?;
    let tmp = std::path::PathBuf::from(format!("{out}.tmp"));
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &out).map_err(|e| format!("{out}: {e}"))?;
    Ok(out)
}

/// A minimal HTTP/1.0 POST to a loopback URL; the response body on 200.
pub(crate) fn loopback_post(url: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    let rest = url.strip_prefix("http://127.0.0.1:").ok_or("not a loopback URL")?;
    let (port, path) = rest.split_once('/').ok_or("bad URL")?;
    let port: u16 = port.parse().map_err(|_| "bad port")?;
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(60))).ok();
    let head = format!(
        "POST /{path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).and_then(|_| s.write_all(body)).map_err(|e| e.to_string())?;
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).map_err(|e| e.to_string())?;
    let split = resp.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no response")?;
    let status = String::from_utf8_lossy(&resp[..split]);
    let code = status.split_whitespace().nth(1).unwrap_or("");
    let payload = resp[split + 4..].to_vec();
    if code != "200" {
        let msg = String::from_utf8_lossy(&payload);
        return Err(format!("Wraithguard answered {code}: {}", msg.trim()));
    }
    Ok(payload)
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

#[cfg(test)]
// Before `builder()` on purpose: check-commands' build.rs keeps this file up to
// `pub fn builder()`, so a test module after it would leave that build.
#[allow(clippy::items_after_test_module)]
mod loopback_tests {
    use super::loopback_post;
    use std::io::{Read, Write};

    #[test]
    fn posts_and_returns_the_body_on_200_only() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            for code in ["200 OK", "400 Bad Request"] {
                let (mut c, _) = l.accept().unwrap();
                // The whole request, body included, as a real server reads it.
                let mut req = Vec::new();
                let mut buf = [0u8; 4096];
                while !req.ends_with(b"\r\n\r\n{}") {
                    let n = c.read(&mut buf).unwrap();
                    assert!(n > 0, "request cut short");
                    req.extend_from_slice(&buf[..n]);
                }
                assert!(String::from_utf8_lossy(&req).starts_with("POST /s/apply?t=x "));
                write!(c, "HTTP/1.0 {code}\r\nContent-Length: 3\r\n\r\nNIF").unwrap();
            }
        });
        let url = format!("http://127.0.0.1:{port}/s/apply?t=x");
        assert_eq!(loopback_post(&url, b"{}").unwrap(), b"NIF");
        assert!(loopback_post(&url, b"{}").unwrap_err().contains("400"));
        t.join().unwrap();
        assert!(loopback_post("http://example.com/x", b"").is_err());
    }
}

pub fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(App::default()))
        .invoke_handler(tauri::generate_handler![
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
            write_b64,
            wg_save_edited,
            wg_open_record,
            wg_post,
            editor_open,
            editor_link,
            loadorder_compute,
            loadorder_export,
            loadorder_rule_ages,
            loadorder_scan,
            loadorder_update_rules,
            loadorder_update_yml,
            loadorder_fetch_data_paths,
            loadorder_author,
            conflicts_scan,
            conflicts_fields,
            conflicts_resources,
            conflicts_texture,
            conflicts_report,
            conflicts_csv,
            conflicts_patch,
            conflicts_land_surfaces,
            editor_preview_patch,
            conflicts_by_plugin,
            conflicts_plugin_group,
            conflicts_plugin_bulk,
            conflicts_plugin_bulk_apply,
            help_docs,
            help_doc,
            help_open_link,
            land_meta_read,
            land_meta_preview,
            land_meta_write,
            land_merge_home,
            land_merge,
            tools_lint,
            tools_backups,
            tools_backups_restore,
            tools_backups_delete,
            tools_masters,
            tools_remove_master,
            tools_rename_master,
            tools_save_check,
            tools_journal,
            tools_tes3cmd_find,
            tools_locate,
            tools_tes3cmd,
            find_record,
            editor_tags,
            editor_records,
            editor_rows,
            editor_cell_refs,
            editor_actors,
            lua_scan,
            lua_chart,
            editor_script_flowchart,
            editor_record_tag,
            ori_dialogue,
            mesh_collision,
            open_install,
            textures,
            cells,
            texture_usage,
            assets,
            cell_data,
            interior_data,
            scopes,
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
            mesh_data,
            assets_bundle,
            texture_data,
            cpu_threads,
            viewer_profile_read,
            cell_coverage,
            ori,
            viewer_profile_write,
        ])
}
