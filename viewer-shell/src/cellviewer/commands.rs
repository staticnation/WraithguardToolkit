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
}

/// The install identity a pending overlay belongs to. A connect to anything else rebuilds.
type InstallId = (String, String, Option<String>);

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
    let order = read_load_order(&l.order_file);
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
    let mut vfs = Vfs::with_roots(&dirs);
    let ms_index = lap(&mut since);
    // The archives (round 11 item 10), opened by the overlay itself since round 18bo so
    // that `gdn_cli` cannot end up with a different one — see `Vfs::add_archives_named_by`.
    let mut warnings = l.warnings.clone();
    warnings.extend(vfs.add_archives_named_by(&l.order_file));
    let ms_archives = lap(&mut since);
    let on_disk = vfs.plugin_files();
    let ms_plugins = lap(&mut since);
    Ok(Opened { l, vfs, order, on_disk, order_file, dirs, warnings, ms: [ms_layout, ms_index, ms_archives, ms_plugins] })
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
    let want: InstallId = (kind.clone(), path.clone(), profile.clone());
    let reuse = {
        let mut app = state.app();
        match app.pending.take() {
            Some((id, o)) if id == want => Some(o),
            _ => None,
        }
    };
    let reused = reuse.is_some();
    let Opened { l, vfs, order, on_disk, order_file, dirs, mut warnings, ms: ms_overlay } =
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
    let world = World::load_with(&paths, &vfs, true, &viewcore::world::LoadOpts { script_plays: true, ..Default::default() });
    let mut world = world;
    // Wraithguard: which plugins are grass (`groundcover=`), so their references go to
    // the grass renderer.
    let gc = viewcore::world::read_groundcover_text(&std::fs::read_to_string(&l.order_file).unwrap_or_default());
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
        .int("refsMoved", s.refs_moved as u64)
        .int("msRead", s.ms_read as u64)
        .int("msParse", s.ms_parse as u64)
        .int("msMerge", s.ms_merge as u64)
        /* Round 18cr: and the overlay's own clock - the layout, the index walk, the
           archives, the plugin listing - whether it was built here or by the probe
           before (`overlayReused` says which, so the page adds it to the right step). */
        .int("msLayout", ms_overlay[0]).int("msIndex", ms_overlay[1])
        .int("msArchives", ms_overlay[2]).int("msPlugins", ms_overlay[3])
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
    let pal = std::sync::Arc::new(viewcore::worldmap::palette(&w, &v));
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
    out.int("tile", viewcore::worldmap::TILE as u64).raw("cells", &list);
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

/// Every land texture the loaded world knows about.
///
/// The id and the file are both needed: an ini section names the LTEX id, but
/// what gets drawn is the filename in that record's DATA field, and the two are
/// rarely the same string. Sending only the id leaves the terrain untextured.
#[tauri::command(async)]
fn textures(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
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

    let mut out = String::from("[");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        viewcore::json::escape_into(id, &mut out);
        out.push_str("\",\"file\":\"");
        if let Some(f) = w.ltex_file.get(&id.to_ascii_lowercase()) {
            viewcore::json::escape_into(f, &mut out);
        }
        out.push_str("\"}");
    }
    out.push(']');
    Ok(out)
}

/// Named cells and regions, for the scope picker.
#[tauri::command(async)]
fn scopes(state: State) -> Result<String, String> {
    let app = state.app();
    let w = app.world.as_ref().ok_or_else(|| viewcore::msg!("eng.no_install"))?;
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
    let mut out = J::obj();
    out.raw("cells", &string_array(&cells))
        .raw("regions", &string_array(&regions));
    Ok(out.done())
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

    // [x, y, name, region] per cell, flat, because a few thousand small objects
    // is measurably slower to hand across than one array.
    let mut grid = String::from("[");
    let mut first = true;
    for (g, c) in &w.cells {
        if !first {
            grid.push(',');
        }
        first = false;
        grid.push_str(&format!("[{},{},", g.0, g.1));
        grid.push('"');
        viewcore::json::escape_into(&c.name, &mut grid);
        grid.push_str("\",\"");
        viewcore::json::escape_into(&c.region, &mut grid);
        grid.push_str("\"]");
    }
    grid.push(']');

    // [name, references] per interior, sorted, so the picker's list is stable between
    // scans of the same install rather than in hash order.
    let mut ints: Vec<&viewcore::world::Interior> = w.interiors.values().collect();
    ints.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut inter = String::from("[");
    for (i, c) in ints.iter().enumerate() {
        if i > 0 {
            inter.push(',');
        }
        inter.push_str("[\"");
        viewcore::json::escape_into(&c.name, &mut inter);
        inter.push_str(&format!("\",{}]", c.refs.len()));
    }
    inter.push(']');

    let mut out = J::obj();
    out.raw("grid", &grid)
        .int("count", w.cells.len() as u64)
        .raw("interiors", &inter)
        .int("interiorCount", w.interiors.len() as u64);
    Ok(out.done())
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
fn cell_data(gx: i32, gy: i32, state: State) -> Result<String, String> {
    // A cell load asks for one of these per cell and each writes a hundred kilobytes of
    // heights; done under the lock they queue behind every scatter. See `snapshot`.
    let w = { state.app().world.clone() }
        .ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let land = w.lands.get(&(gx, gy));
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
                if let Some(id) = w.ltex_for(*v, l.plugin) {
                    if ltex.len() > 1 {
                        ltex.push(',');
                    }
                    // keyed by v-1, which is what the page's resolver looks up
                    ltex.push_str(&format!("\"{}\":", v - 1));
                    ltex.push('"');
                    viewcore::json::escape_into(id, &mut ltex);
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
    let layers = match land {
        Some(l) => viewcore::coverage::layers_json(&viewcore::coverage::layers(&w, l)),
        None => "[]".to_string(),
    };

    // References, each already carrying the mesh it draws.
    let (refs, models, actors, lights) = refs_json(&w, cell.map(|c| &c.refs), false);

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
        .raw("prov", &prov);
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
    if let Some(list) = src {
        let mut named: Vec<&viewcore::esp::CellRef> = list.values().collect();
        named.sort_by_key(|r| (r.num.content_file, r.num.index));
        for r in named {
            let key = r.id.to_ascii_lowercase();
            let actor = w.actors.get(&key);
            if actor.is_none() && !w.models.contains_key(&key) {
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
            if let Some(a) = actor {
                if !first_actor {
                    actors.push(',');
                }
                first_actor = false;
                actors.push('"');
                viewcore::json::escape_into(&key, &mut actors);
                actors.push_str("\":{\"kind\":\"");
                actors.push_str(if a.creature { "crea" } else { "npc" });
                actors.push_str("\",\"model\":\"");
                viewcore::json::escape_into(&a.model, &mut actors);
                // Round 18ag: and the `x` twin the game draws the living creature with.
                actors.push_str("\",\"twin\":");
                match &a.twin {
                    Some(t) => {
                        actors.push('"');
                        viewcore::json::escape_into(t, &mut actors);
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
                actors.push('}');
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
fn interior_data(name: String, state: State) -> Result<String, String> {
    let w = { state.app().world.clone() }
        .ok_or_else(|| viewcore::msg!("eng.no_install"))?;
    let room = w.room(&name).ok_or_else(|| viewcore::msg!("eng.no_interior", name = name))?;

    let (refs, models, actors, lights) = refs_json(&w, Some(&room.refs), !room.quasi);
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
        .raw("prov", &prov);
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
    Ok(viewcore::weather::atmosphere_json(&order, &vfs, &layout))
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
fn texture_payload(v: &Vfs, path: &str, bc: bool, max_size: u32) -> Result<Vec<u8>, String> {
    let n = viewcore::vfs::norm(path);
    let p = v
        .resolve_texture(&n)
        .or_else(|| v.resolve(&n))
        .ok_or_else(|| viewcore::msg!("eng.not_in_load_order", path = path))?;
    let bytes = v.load(&p).ok_or_else(|| viewcore::msg!("eng.could_not_read", path = path))?;
    // Where it came from, for the report: a decode error names the file that failed.
    let from = p.ident(v);
    let mut t = viewcore::img::read(&bytes, &n, bc).map_err(|e| format!("{e} — {from}"))?;
    if max_size > 0 && t.levels.len() > 1 {
        while t.levels.len() > 1 && t.levels[0].0.max(t.levels[0].1) > max_size {
            t.levels.remove(0);
        }
        t.w = t.levels[0].0;
        t.h = t.levels[0].1;
    }

    let mut head = J::obj();
    head.str("kind", t.kind.name()).int("w", t.w as u64).int("h", t.h as u64).int("depth", t.depth as u64).str("from", &from);
    /* Round 17n: what the alpha channel is for — opaque, a stencil, or a real gradient.
       A NiAlphaProperty says how to draw a shape but not what its texture holds, and the
       two want opposite treatment: cutting a gradient at a threshold turned Vivec's
       waterfalls into hard stencils. Read at full resolution in `img::read`. */
    head.str("alpha", t.alpha.name());
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
    state: State,
) -> Result<tauri::ipc::Response, String> {
    let v = vfs_of(&state)?;
    Ok(tauri::ipc::Response::new(texture_payload(&v, &path, bc.unwrap_or(false), max_size.unwrap_or(0))?))
}

/// One mesh, parsed and ready to draw.
///
/// The page used to read the NIF bytes over this same boundary and parse them again
/// in JavaScript — a second reader of a format with no record lengths, which is
/// where a season of avoidance bugs came from. It now draws what the engine read.
/// The bytes `mesh_data` answers with, and the textures the mesh names - the bundle
/// (below) fetches those in the same call.
fn mesh_payload(v: &Vfs, path: &str) -> Result<(Vec<u8>, Vec<String>), String> {
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
        return Ok(mesh_payload_of(&bytes, kf.as_ref(), None));
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
    let kf = kf_beside(&v, path);
    // Round 17m: and where a light hangs on it, for the LIGH records that name it.
    // Round 18cr: the parts, the particle systems and the light in one read of the file.
    Ok(mesh_payload_of(&bytes, kf.as_ref(), None))

}


/// A mesh's payload from its bytes: the drawable parts, the collision geometry (`geom`
/// when the world has it cached, else parsed here), attach point, particles and light,
/// and every texture it names.
fn mesh_payload_of(
    bytes: &[u8],
    kf: Option<&viewcore::nif::KfSequence>,
    geom: Option<viewcore::nif::MeshGeom>,
) -> (Vec<u8>, Vec<String>) {
    // One read of the file for both the drawing and the collision (the world no longer
    // reads every mesh's collision at startup - that was for grass generation).
    let (parts, attach, systems, light, read_geom) = match viewcore::nif::read_for_draw_and_geom(bytes, kf) {
        Some((r, g)) => (r.parts, r.attach, r.systems, r.light, g),
        None => (Vec::new(), None, Vec::new(), None, None),
    };
    let geom = geom.or(read_geom);
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
        for t in [&part.glow_tex, &part.decal_tex, &part.env_tex, &part.bump_tex] {
            if !t.is_empty() && !texs.iter().any(|x| x == t) {
                texs.push(t.clone());
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
fn mesh_data(path: String, state: State) -> Result<tauri::ipc::Response, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    Ok(tauri::ipc::Response::new(mesh_payload(&v, &path)?.0))
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
fn assets_bundle(
    meshes: Vec<String>,
    have: Vec<String>,
    bc: Option<bool>,
    max_size: Option<u32>,
    state: State,
) -> Result<tauri::ipc::Response, String> {
    let v = { state.app().vfs.clone().ok_or_else(|| viewcore::msg!("eng.no_install"))? };
    let bc = bc.unwrap_or(false);
    let max_size = max_size.unwrap_or(0);
    /* Round 18cr: the engine's own clock on the bundle, for the report's Loading line -
       so a slow "placing objects" can be told apart into the engine's reading, the trip
       through the shell, and the page's decoding. */
    let clock = std::time::Instant::now();
    let got: Vec<Result<(Vec<u8>, Vec<String>), String>> =
        viewcore::pool::par_map(&meshes, |_, m| mesh_payload(&v, m));
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
    let texs: Vec<Result<Vec<u8>, String>> =
        viewcore::pool::par_map(&want, |_, t| texture_payload(&v, t, bc, max_size));

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
    // The reference, and the cell it stands in.
    let mut found: Option<(&viewcore::esp::CellRef, String)> = None;
    for (g, c) in &w.cells {
        if let Some(r) = c.refs.iter().find(|(n, _)| is_key(n)).map(|(_, r)| r) {
            let label = if c.name.is_empty() { format!("{}, {}", g.0, g.1) } else { format!("{} ({}, {})", c.name, g.0, g.1) };
            found = Some((r, label));
            break;
        }
    }
    if found.is_none() {
        for room in w.interiors.values() {
            if let Some(r) = room.refs.iter().find(|(n, _)| is_key(n)).map(|(_, r)| r) {
                found = Some((r, room.name.clone()));
                break;
            }
        }
    }
    let (r, cell) = found.ok_or_else(|| format!("no reference {key} in the loaded world"))?;
    let name = |i: i32| usize::try_from(i).ok().and_then(|i| w.plugins.get(i)).cloned().unwrap_or_default();
    let lid = r.id.to_ascii_lowercase();

    let mut o = J::obj();
    o.str("key", &key).str("id", &r.id).str("cell", &cell);
    // The plugin that created the reference, spelled as the load order spells it.
    let created = viewcore::refkey::RefKey::of(r.num, &w.plugins).map(|k| k.plugin).unwrap_or_default();
    let created = w.plugins.iter().find(|p| p.eq_ignore_ascii_case(&created)).cloned().unwrap_or(created);
    o.str("createdBy", &created);
    o.str("lastChangedBy", &name(r.plugin));
    let touched: Vec<String> = r.touched.iter().map(|&i| name(i)).collect();
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
    // The mesh and its textures, and where each resolves in the setup.
    let mesh = model.filter(|m| !m.is_empty()).or_else(|| w.models.get(&lid).cloned()).unwrap_or_default();
    o.str("mesh", &mesh);
    if let Some(v) = v.as_ref() {
        if !mesh.is_empty() {
            let src = v.resolve_mesh(&mesh).map(|l| l.ident(v)).unwrap_or_default();
            o.str("meshFrom", &src);
            if let Ok((_, texs)) = mesh_payload(v, &mesh) {
                let rows: Vec<String> = texs
                    .iter()
                    .map(|t| {
                        let mut d = J::obj();
                        d.str("texture", t)
                            .str("from", &v.resolve_texture(t).map(|l| l.ident(v)).unwrap_or_default());
                        d.done()
                    })
                    .collect();
                o.raw("textures", &format!("[{}]", rows.join(",")));
            }
        }
    }
    Ok(o.done())
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
        .filter(|c| !c.touched_by.is_empty())
        .map(|c| format!("[{},{},{}]", c.grid.0, c.grid.1, ix(&c.touched_by)))
        .collect();
    ext.sort();
    let mut int: Vec<String> = w
        .interiors
        .values()
        .filter(|r| !r.touched_by.is_empty())
        .map(|r| {
            let mut name = String::new();
            viewcore::json::escape_into(&r.name, &mut name);
            format!("[\"{}\",{}]", name, ix(&r.touched_by))
        })
        .collect();
    int.sort();
    Ok(format!(
        "{{\"plugins\":{},\"ext\":[{}],\"int\":[{}]}}",
        string_array(&w.plugins),
        ext.join(","),
        int.join(",")
    ))
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
#[tauri::command(async)]
fn viewer_profile_write(text: String) -> Result<String, String> {
    let p = viewer_prefs_path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {}", p.display(), e))?;
    Ok(p.display().to_string())
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
            wg_save_edited,
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
