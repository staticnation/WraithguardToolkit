// Headless cell-viewer engine for tests: the command bodies over JSON lines on stdin/stdout.
// From Gardenfell's gdn_cli serve.rs ((c) 2026 Robin Hjelte, MIT).
// The shell's commands, over a pipe.
//
// The app reaches the engine through Tauri. Everything else that wants to drive it —
// the browser test suite, above all — could not, which is why the page grew a second
// implementation of half the engine in the first place, and why the tests that
// exercised it were testing the copy rather than the thing that ships.
//
// This runs the *same command bodies* the shell runs: `wraithguard_viewer_check_commands` is the crate
// that compiles `src-tauri/src/main.rs` against a stub, so calling into it is calling
// the shipping code. Not a mock, not a parallel implementation — one JSON object per
// line in, one out, and the functions in between are the ones the desktop app calls.
//
// ```text
// {"id":1,"cmd":"open_install","args":{"kind":"vanilla","path":"…/Data Files"}}
// {"id":1,"ok":{…}}
// ```
//
// Binary replies (`read_asset`, `mesh_data`) come back as base64 under `b64`, because
// the transport is lines of text and the alternative is a framing protocol nobody
// needs for a mesh.

use viewcore::json::{parse, Val};
use wraithguard_viewer_check_commands::{tauri::State, App};
use std::io::{BufRead, Write};
use std::sync::Mutex;

fn s(a: &Val, k: &str) -> String {
    a.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}
fn i(a: &Val, k: &str) -> i32 {
    a.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}
fn b(a: &Val, k: &str) -> bool {
    a.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// An optional array of flags (`None` when the page did not send it).
fn bools(a: &Val, k: &str) -> Option<Vec<bool>> {
    a.get(k).and_then(|v| v.as_arr()).map(|xs| xs.iter().map(|x| x.as_bool().unwrap_or(false)).collect())
}

fn strs(a: &Val, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(|v| v.as_arr())
        .map(|xs| xs.iter().filter_map(|x| x.as_str()).map(String::from).collect())
        .unwrap_or_default()
}


/// A count or a size (absent, or below nothing: 0).
fn u(a: &Val, k: &str) -> usize {
    a.get(k).and_then(|v| v.as_i64()).unwrap_or(0).max(0) as usize
}

/// A value handed on as the shell's commands take one (`serde_json::Value`).
fn sj(v: &Val) -> serde_json::Value {
    match v {
        Val::Null => serde_json::Value::Null,
        Val::Bool(b) => (*b).into(),
        Val::Num(n) if n.fract() == 0.0 && n.abs() < 9e15 => (*n as i64).into(),
        Val::Num(n) => (*n).into(),
        Val::Str(s) => s.as_str().into(),
        Val::Arr(xs) => xs.iter().map(sj).collect::<Vec<_>>().into(),
        Val::Obj(kv) => serde_json::Value::Object(kv.iter().map(|(k, v)| (k.clone(), sj(v))).collect()),
    }
}

/// A member as a `serde_json::Value` (absent: null), and a list of them.
fn val(a: &Val, k: &str) -> serde_json::Value {
    a.get(k).map(sj).unwrap_or(serde_json::Value::Null)
}
fn vals(a: &Val, k: &str) -> Vec<serde_json::Value> {
    a.get(k).and_then(|v| v.as_arr()).map(|xs| xs.iter().map(sj).collect()).unwrap_or_default()
}

/// An optional string: absent and empty mean the same thing to every caller here.
fn opt(a: &Val, k: &str) -> Option<String> {
    let v = s(a, k);
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(B64[(n >> 18 & 63) as usize] as char);
        out.push(B64[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 { B64[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Runs one command. `Ok(Some(json))` is a JSON reply, `Ok(None)` a base64 one already
/// wrapped by the caller.
fn dispatch(cmd: &str, a: &Val, app: &Mutex<App>) -> Result<Reply, String> {
    use wraithguard_viewer_check_commands as t;
    let st = || State::new(app);
    Ok(match cmd {
        /* Round 18bi: `plugins` is which of the load order to parse — absent means all of
           it, and an empty array really does mean none, the same distinction `opt_ix`
           draws for the export's rules. */
        "open_install" => Reply::Json(t::open_install(
            s(a, "kind"),
            s(a, "path"),
            opt(a, "profile"),
            a.get("plugins").and_then(|v| v.as_arr()).map(|xs| {
                xs.iter().filter_map(|x| x.as_str()).map(String::from).collect::<Vec<String>>()
            }),
            opt(a, "last"),
            st(),
        )?),
        "cell_coverage" => Reply::Json(t::cell_coverage(st())?),
        "ori" => Reply::Json(t::ori(s(a, "key"), opt(a, "model"), st())?),
        "connect_stage" => Reply::Json(t::connect_stage()),
        "viewer_profile_read" => Reply::Text(t::viewer_profile_read()?),
        "viewer_profile_write" => Reply::Text(t::viewer_profile_write(s(a, "text"))?),
        "wg_save_edited" => Reply::Text(t::wg_save_edited(s(a, "url"), s(a, "body"), s(a, "out"))?),
        "wg_post" => Reply::Text(t::wg_post(s(a, "url"), s(a, "body"))?),
        "find_record" => Reply::Json(t::find_record(s(a, "tag"), s(a, "id"), st())?),
        "editor_tags" => Reply::Json(t::editor_tags(st())?),
        "editor_records" => Reply::Json(t::editor_records(s(a, "tag"), st())?),
        "editor_rows" => Reply::Bytes(t::editor_rows(s(a, "tag"), st())?.0),
        "editor_tags_bytes" => Reply::Bytes(t::editor_tags_bytes(st())?.0),
        "cells_bytes" => Reply::Bytes(t::cells_bytes(st())?.0),
        "textures_bytes" => Reply::Bytes(t::textures_bytes(st())?.0),
        "scopes_bytes" => Reply::Bytes(t::scopes_bytes(st())?.0),
        "texture_usage_bytes" => Reply::Bytes(t::texture_usage_bytes(st())?.0),
        "assets_bytes" => Reply::Bytes(t::assets_bytes(st())?.0),
        "editor_cell_refs_bytes" => Reply::Bytes(t::editor_cell_refs_bytes(s(a, "cell"), st())?.0),
        "editor_cell_refs" => Reply::Json(t::editor_cell_refs(s(a, "cell"), st())?),
        "editor_ref_names" => Reply::Json(t::editor_ref_names(strs(a, "keys"), st())?),
        "editor_actors" => Reply::Json(t::editor_actors(strs(a, "ids"), st())?),
        "lua_scan" => Reply::Json(t::lua_scan(st())?),
        "editor_record_tag" => Reply::Json(t::editor_record_tag(s(a, "id"), st())?),
        "ori_dialogue" => Reply::Json(t::ori_dialogue(s(a, "id"), st())?),
        "startup_install" => Reply::Json(t::startup_install()?),
        // Round 18cj: the colour theme, remembered for the next start-up.
        "theme_set" => Reply::Json(t::theme_set(s(a, "code"))?),
        "picker_set" => Reply::Json(t::picker_set(s(a, "mode"))?),
        "window_set" => Reply::Json(t::window_set(i(a, "x"), i(a, "y"), i(a, "w").max(0) as u32, i(a, "h").max(0) as u32, b(a, "max"))?),
        "world_map" => Reply::Json(t::world_map(st())?),
        "world_map_tiles" => Reply::Bytes(t::world_map_tiles(i(a, "from").max(0) as u32, i(a, "count").max(0) as u32, st())?.0),
        "distant_land" => Reply::Bytes(t::distant_land(i(a, "x"), i(a, "y"), i(a, "inner"), i(a, "radius"), st())?.0),
        "distant_objects" => Reply::Json(t::distant_objects(i(a, "x"), i(a, "y"), i(a, "inner"), i(a, "radius"), st())?),
        "textures" => Reply::Json(t::textures(st())?),
        "scopes" => Reply::Json(t::scopes(st())?),
        "cells" => Reply::Json(t::cells(st())?),
        "texture_usage" => Reply::Json(t::texture_usage(st())?),
        "assets" => Reply::Json(t::assets(st())?),
        "cell_data" => Reply::Json(t::cell_data(i(a, "gx"), i(a, "gy"), opt(a, "review"), Some(b(a, "without")), opt(a, "land"), st())?),
        "interior_data" => Reply::Json(t::interior_data(s(a, "name"), opt(a, "review"), Some(b(a, "without")), st())?),
        // Wraithguard: the review, overlay and link tools.
        "pathgrid" => Reply::Json(t::pathgrid(strs(a, "cells"), st())?),
        "navmesh" => Reply::Bytes(t::navmesh(strs(a, "cells"), opt(a, "db"), st())?.0),
        "collision_bundle" => Reply::Bytes(t::collision_bundle(strs(a, "meshes"), st())?.0),
        "where_used" => Reply::Json(t::where_used(s(a, "kind"), s(a, "name"), st())?),
        "asset_providers" => Reply::Json(t::asset_providers(s(a, "path"), s(a, "kind"), st())?),
        "game_settings" => Reply::Json(t::game_settings(st())?),
        "walk_player_from_save" => Reply::Json(t::walk_player_from_save(s(a, "path"))?),
        "scene_scripts" => Reply::Json(t::scene_scripts(strs(a, "ids"), st())?),
        "script_texts" => Reply::Json(t::script_texts(strs(a, "names"), st())?),
        "lua_scene" => Reply::Json(t::lua_scene(strs(a, "ids"), st())?),
        "lua_run_start" => Reply::Json(t::lua_run_start(strs(a, "weathers"), Some(s(a, "openmw")).filter(|x| !x.is_empty()), st())?),
        "lua_run_step" => Reply::Json(t::lua_run_step(s(a, "frame"), st())?),
        "lua_run_stop" => Reply::Json(t::lua_run_stop()?),
        "dialogue_state_from_save" => Reply::Json(t::dialogue_state_from_save(s(a, "path"))?),
        "phys_drop" => Reply::Json(t::phys_drop(s(a, "request"), st())?),
        "lua_ui_font" => Reply::Bytes(t::lua_ui_font()?.0),
        "write_b64" => {
            t::write_b64(s(a, "path"), s(a, "data"))?;
            Reply::Raw("true".into())
        }
        // Text, not JSON: the caller wants the file's bytes.
        "texture_data" => Reply::Bytes(t::texture_data(s(a, "path"), Some(b(a, "bc")), a.get("maxSize").and_then(|v| v.as_i64()).map(|v| v.max(0) as u32), Some(strs(a, "bcx")), st())?.0),
        "assets_bundle" => Reply::Bytes(t::assets_bundle(strs(a, "meshes"), strs(a, "have"), Some(b(a, "bc")), a.get("maxSize").and_then(|v| v.as_i64()).map(|v| v.max(0) as u32), Some(strs(a, "bcx")), Some(strs(a, "maps")), bools(a, "still"), st())?.0),
        // Spelled by hand here, as every argument in this file is; renaming one is a
        // two-file edit and the symptom of forgetting is the engine insisting a value is
        // missing while the page plainly sent it.
        "atmosphere" => Reply::Json(t::atmosphere(st())?),
        // The world's sound and the region map (SOUND.md, TODO "The world").
        "regions_list" => Reply::Json(t::regions_list(st())?),
        "region_sounds" => Reply::Json(t::region_sounds(s(a, "region"), st())?),
        "sound_emitters" => Reply::Json(t::sound_emitters(strs(a, "ids"), st())?),
        "sound_records" => Reply::Json(t::sound_records(strs(a, "ids"), st())?),
        "sound_fallbacks" => Reply::Json(t::sound_fallbacks(st())?),
        "music_list" => Reply::Json(t::music_list(s(a, "kind"), st())?),
        "favourites_list" => Reply::Json(t::favourites_list()?),
        "lang_list" => Reply::Json(t::lang_list()?),
        "lang_get" => Reply::Json(t::lang_get(s(a, "code"))?),
        "lang_create" => Reply::Json(t::lang_create(s(a, "code"), s(a, "name"))?),
        "lang_update" => Reply::Json(t::lang_update(s(a, "code"))?),
        "open_folder" => Reply::Json(t::open_folder(s(a, "what"))?),
        "favourite_set" => {
            t::favourite_set(s(a, "kind"), s(a, "item"), b(a, "on"))?;
            Reply::Raw("true".into())
        }
        "write_text" => {
            t::write_text(s(a, "path"), s(a, "text"))?;
            Reply::Raw("true".into())
        }
        /* Round 18r: there is no pointer to move out here. The command is dispatched all
           the same rather than left out, so the page can call it without asking which
           shell it is in - the browser suites reach the same branch the desktop does and
           get the same "done". */
        "cursor_park" => {
            t::cursor_park(
                a.get("show").and_then(|v| v.as_bool()),
                a.get("x").and_then(|v| v.as_f64()),
                a.get("y").and_then(|v| v.as_f64()),
                t::tauri::Window::default(),
            )?;
            Reply::Raw("true".into())
        }
        // Wraithguard's pages (the Load order, Conflicts, Tools and Help pages, Merge Lands, the
        // sound and the charts): the argument names are the page's (Tauri's camelCase).
        "sound_info" => Reply::Json(t::sound_info(s(a, "path"), u(a, "n"), st())?),
        "sound_pcm" => Reply::Bytes(t::sound_pcm(s(a, "path"), st())?.0),
        "mesh_save" => Reply::Text(t::mesh_save(s(a, "path"), s(a, "edits"), s(a, "out"), st())?),
        "editor_script_flowchart" => Reply::Json(t::editor_script_flowchart(s(a, "id"), s(a, "text"))?),
        "lua_chart" => Reply::Json(t::lua_chart(s(a, "path"), s(a, "text"), s(a, "kind"), a.get("line").and_then(|v| v.as_i64()).map(|v| v.max(0) as usize))?),
        "loadorder_drop_probe" => Reply::Json(t::loadorder_drop_probe(strs(a, "paths"), strs(a, "existing"), b(a, "quoted"))?),
        "loadorder_compute" => Reply::Json(t::loadorder_compute(s(a, "args"))?),
        "loadorder_export" => Reply::Json(t::loadorder_export(s(a, "args"))?),
        "loadorder_plain_sort" => Reply::Json(t::loadorder_plain_sort(s(a, "cfg"), strs(a, "rules"))?),
        "loadorder_plain_write" => Reply::Json(t::loadorder_plain_write(s(a, "cfg"), strs(a, "order"), b(a, "dryRun"), s(a, "stamp"))?),
        "loadorder_rule_ages" => Reply::Json(t::loadorder_rule_ages(strs(a, "rules"))?),
        "loadorder_scan" => Reply::Json(t::loadorder_scan(s(a, "dir"))?),
        "loadorder_update_rules" => Reply::Json(t::loadorder_update_rules(strs(a, "rules"), s(a, "stamp"))?),
        "loadorder_update_yml" => Reply::Json(t::loadorder_update_yml(s(a, "path"), s(a, "stamp"))?),
        "loadorder_fetch_data_paths" => Reply::Json(t::loadorder_fetch_data_paths(s(a, "path"), s(a, "listName"), s(a, "stamp"))?),
        "meshopt_analyse" => Reply::Json(t::meshopt_analyse(s(a, "cfg"), val(a, "opts"), s(a, "outDir"), u(a, "limit"))?),
        "meshopt_write" => Reply::Json(t::meshopt_write(s(a, "cfg"), val(a, "opts"), s(a, "outDir"), strs(a, "paths"))?),
        "meshopt_progress" => {
            let [done, total] = t::meshopt_progress();
            Reply::Raw(format!("[{done},{total}]"))
        }
        "conflicts_scan" => Reply::Json(t::conflicts_scan(s(a, "cfg"), strs(a, "subset"), s(a, "mode"), strs(a, "exclude"))?),
        "conflicts_fields" => Reply::Json(t::conflicts_fields(s(a, "cfg"), s(a, "rtype"), s(a, "id"))?),
        "conflicts_land_surfaces" => Reply::Json(t::conflicts_land_surfaces(s(a, "cfg"), s(a, "id"))?),
        "conflicts_resources" => Reply::Json(t::conflicts_resources(
            s(a, "cfg"),
            strs(a, "subsetDirs"),
            strs(a, "excludeExts"),
            b(a, "compare"),
            b(a, "meshes"),
            u(a, "meshLimit"),
            s(a, "planFile"),
        )?),
        "conflicts_texture" => Reply::Json(t::conflicts_texture(s(a, "path"), strs(a, "providers"), u(a, "max") as u32)?),
        "conflicts_report" => Reply::Text(t::conflicts_report(s(a, "kind"), b(a, "subsetOnly"), u(a, "limit"))?),
        "conflicts_csv" => {
            t::conflicts_csv(s(a, "kind"), s(a, "path"))?;
            Reply::Raw("null".into())
        }
        "conflicts_patch" => Reply::Raw(
            t::conflicts_patch(s(a, "journal"), s(a, "op"), s(a, "rtype"), s(a, "id"), s(a, "plugin"), s(a, "field"), s(a, "value"))?.to_string(),
        ),
        "conflicts_by_plugin" => Reply::Json(t::conflicts_by_plugin(s(a, "cfg"), strs(a, "subset"))?),
        "conflicts_plugin_group" => Reply::Json(t::conflicts_plugin_group(s(a, "cfg"), s(a, "plugin"), s(a, "rtype"))?),
        "conflicts_verdicts" => Reply::Json(t::conflicts_verdicts(s(a, "cfg"))?),
        "conflicts_plugin_bulk" => Reply::Json(t::conflicts_plugin_bulk(s(a, "cfg"), s(a, "plugin"))?),
        "conflicts_plugin_bulk_apply" => Reply::Raw(t::conflicts_plugin_bulk_apply(s(a, "journal"), s(a, "plugin"), strs(a, "paths"), b(a, "dry"))?.to_string()),
        "land_meta_read" => Reply::Json(t::land_meta_read(s(a, "plugin"))?),
        "land_meta_preview" => Reply::Json(t::land_meta_preview(s(a, "metaType"), vals(a, "layers"))?),
        "land_meta_write" => Reply::Json(t::land_meta_write(s(a, "plugin"), s(a, "metaType"), vals(a, "layers"))?),
        "land_merge_home" => Reply::Json(t::land_merge_home(s(a, "cfg"))?),
        "land_merge" => Reply::Json(t::land_merge(
            s(a, "cfg"),
            s(a, "folder"),
            b(a, "dryRun"),
            b(a, "verbose"),
            s(a, "model"),
            a.get("ignore").and_then(|v| v.as_arr()).map(|_| strs(a, "ignore")),
        )?),
        "land_merge_cancel" => Reply::Raw(t::land_merge_cancel()?.to_string()),
        "land_merge_progress" => Reply::Json(t::land_merge_progress(u(a, "from"))?),
        "help_docs" => Reply::Json(t::help_docs(strs(a, "dirs"))?),
        "help_doc" => Reply::Json(t::help_doc(s(a, "name"), strs(a, "dirs"))?),
        "tools_lint" => Reply::Json(t::tools_lint(s(a, "cfg"), strs(a, "subset"))?),
        "tools_backups" => Reply::Json(t::tools_backups(s(a, "cfg"), strs(a, "dirs"))?),
        "tools_backups_restore" => Reply::Json(t::tools_backups_restore(
            a.get("pairs")
                .and_then(|v| v.as_arr())
                .map(|xs| {
                    xs.iter()
                        .filter_map(|p| p.as_arr())
                        .filter_map(|p| Some((p.first()?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        )?),
        "tools_backups_delete" => Reply::Raw(t::tools_backups_delete(strs(a, "paths"))?.to_string()),
        "tools_masters" => Reply::Json(t::tools_masters(s(a, "path"))?),
        "tools_remove_master" => Reply::Json(t::tools_remove_master(s(a, "path"), s(a, "master"), b(a, "dryRun"))?),
        "tools_merge_master" => Reply::Json(t::tools_merge_master(
            s(a, "mode"),
            s(a, "plugin"),
            s(a, "master"),
            strs(a, "plugins"),
            s(a, "out"),
            b(a, "removeDeleted"),
            b(a, "applyMoved"),
            b(a, "keepDuplicates"),
        )?),
        "tools_rename_master" => Reply::Json(t::tools_rename_master(s(a, "path"), s(a, "old"), s(a, "newPath"))?),
        "tools_save_check" => Reply::Json(t::tools_save_check(s(a, "cfg"), strs(a, "saves"), u(a, "details"), strs(a, "mods"), b(a, "all"), strs(a, "dump"))?),
        "tools_journal" => Reply::Json(t::tools_journal(s(a, "cfg"))?),
        "tools_tes3cmd_find" => Reply::Text(t::tools_tes3cmd_find(s(a, "cfg"), s(a, "explicit"))),
        "tools_locate" => Reply::Json(t::tools_locate(s(a, "cfg"), strs(a, "names"))?),
        "tools_tes3cmd" => Reply::Json(t::tools_tes3cmd(s(a, "cfg"), s(a, "exe"), s(a, "cmd"), strs(a, "extra"), strs(a, "files"))?),
        "read_asset" => Reply::Bytes(t::read_asset(s(a, "path"), st())?.0),
        "mesh_data" => Reply::Bytes(t::mesh_data(s(a, "path"), a.get("still").and_then(|v| v.as_bool()), st())?.0),
        "mesh_collision" => Reply::Bytes(t::mesh_collision(s(a, "path"), st())?.0),
        // 4.6, folding native/: the mesh viewer's block panel, and the Editor opened on the
        // install itself when no host hands it a setup.
        "mesh_inspect" => Reply::Json(t::mesh_inspect(s(a, "path"), st())?),
        "editor_setup" => Reply::Json(t::editor_setup(st())?),
        "editor_open" => Reply::Json(t::editor_open(
            a.get("plugins")
                .and_then(|v| v.as_arr())
                .map(|xs| {
                    xs.iter()
                        .filter_map(|x| {
                            let p = x.as_arr()?;
                            Some((p.first()?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            s(a, "journal"),
            Some(strs(a, "folders")),
            opt(a, "last"),
            opt(a, "testSettings"),
        )?),
        "editor_link" => Reply::Text(t::editor_link(st(), s(a, "name"), s(a, "body"))?),
        "cpu_threads" => Reply::Raw(t::cpu_threads().to_string()),
        // The settings file and the Setup page (port 4.5): the page's first start is driven
        // through these (ui/tests/boot.js, WG_SETUP=1).
        "setup_state" => Reply::Json(t::setup_state(s(a, "settingsFile"))?),
        "setup_check" => Reply::Json(t::setup_check(s(a, "fields"))?),
        "setup_find_cfgs" => Reply::Json(t::setup_find_cfgs()?),
        "setup_mark" => {
            t::setup_mark(s(a, "state"), s(a, "cfg"))?;
            Reply::Raw("null".into())
        }
        // The rule maker (66_wg_rulemaker.js) and the scans' cache warm-up after connecting.
        "loadorder_author" => Reply::Json(t::loadorder_author(s(a, "op"), s(a, "rule"), opt(a, "path"))?),
        "scan_cache_warm" => {
            t::scan_cache_warm(s(a, "cfg"))?;
            Reply::Raw("null".into())
        }
        "settings_seed" => Reply::Json(t::settings_seed(s(a, "settingsFile"))?),
        "settings_write" => Reply::Json(t::settings_write(s(a, "settingsFile"), s(a, "fields"))?),
        other => return Err(format!("no such command: {other}")),
    })
}

enum Reply {
    /// Already JSON; passed through as-is.
    Json(String),
    /// A string the caller wants as a JSON string.
    Text(String),
    /// A number or literal.
    Raw(String),
    Bytes(Vec<u8>),
}

/// How many requests may be in flight at once — round 18bf (I8).
///
/// It used to be one: the loop read a line, ran the command on the main thread, and wrote
/// the reply before reading the next. So nothing the caller sent could ever arrive *while*
/// a command was running, and round 18n's supersede path — a scatter that stops because a
/// newer ask has come in — was not merely untested over the pipe, it was unreachable. The
/// browser suites drive the engine through here, so the mechanism that makes a burst of
/// edits cost one wait rather than one per edit had no coverage from the tests that
/// exercise it.
///
/// Nothing reorders for a caller that awaits each reply before sending the next, which is
/// what the page does for everything but the preview: concurrency here is the *caller's*
/// to use. Replies carry the `id` they answer and `engine.js` matches on it, so they may
/// come back in any order.
const SERVE_MAX: usize = 4;

/// One request, answered. Whatever happens, a line comes back.
fn serve_one(line: &str, app: &Mutex<App>) -> String {
    let (id, reply) = {
        match parse(line) {
            Err(e) => (0.0, Err(format!("unreadable request: {e}"))),
            Ok(req) => {
                let id = req.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let args = req.get("args").cloned().unwrap_or(Val::Obj(vec![]));
                /* A command that panics takes the whole session with it otherwise, and
                   a test suite that dies mid-run tells you far less than one that
                   reports which call went wrong. */
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    dispatch(&cmd, &args, app)
                }));
                (
                    id,
                    match caught {
                        Ok(r) => r,
                        Err(_) => Err(format!("{cmd} panicked")),
                    },
                )
            }
        }
    };

    {
        let mut s = format!("{{\"id\":{id},");
        match reply {
            Ok(Reply::Json(j)) => {
                s.push_str("\"ok\":");
                s.push_str(&j);
            }
            Ok(Reply::Raw(v)) => {
                s.push_str("\"ok\":");
                s.push_str(&v);
            }
            Ok(Reply::Text(t)) => {
                s.push_str("\"ok\":\"");
                viewcore::json::escape_into(&t, &mut s);
                s.push('"');
            }
            Ok(Reply::Bytes(bytes)) => {
                s.push_str("\"b64\":\"");
                s.push_str(&base64(&bytes));
                s.push('"');
            }
            Err(e) => {
                s.push_str("\"err\":\"");
                viewcore::json::escape_into(&e, &mut s);
                s.push('"');
            }
        }
        s.push('}');
        s
    }
}

fn run() -> i32 {
    viewcore::init();
    /* Leaked on purpose. The workers below borrow the state for as long as they live, and
       they live for as long as the session does — a `serve` session that is over is a
       process that is over, so there is nothing for a drop to do. */
    let app: &'static Mutex<App> = Box::leak(Box::new(Mutex::new(App::default())));

    /* One writer, so two replies cannot interleave on a line. It announces readiness
       itself rather than the main thread doing it, which keeps "ready" first in the
       stream by construction instead of by timing. */
    let (done_tx, done_rx) = std::sync::mpsc::channel::<String>();
    let writer = std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{{\"ready\":true}}");
        let _ = out.flush();
        for line in done_rx {
            if writeln!(out, "{line}").is_err() || out.flush().is_err() {
                break;
            }
        }
    });

    let (job_tx, job_rx) = std::sync::mpsc::channel::<String>();
    let job_rx = std::sync::Arc::new(Mutex::new(job_rx));
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(2, SERVE_MAX);
    let mut workers = Vec::with_capacity(n);
    for _ in 0..n {
        let job_rx = job_rx.clone();
        let done = done_tx.clone();
        workers.push(std::thread::spawn(move || loop {
            // The guard is dropped before the work, so handing a job out is serialised
            // and doing it is not.
            let job = {
                let Ok(rx) = job_rx.lock() else { break };
                rx.recv()
            };
            let Ok(line) = job else { break };
            if done.send(serve_one(&line, app)).is_err() {
                break;
            }
        }));
    }
    drop(done_tx);

    /* Round 18bf (I7): bytes, not lines.

       `stdin.lock().lines()` yields `Err` for a line that is not UTF-8, and the loop
       ended on it — returning 0, which is exactly what a clean shutdown returns. A test
       harness whose pipe picked up one stray byte got a session that simply stopped
       answering, with no message and a success code. Read as bytes and decoded lossily, a
       bad byte is one request's problem: the JSON fails to parse and the caller gets
       "unreadable request" against the right id. A real I/O error on the pipe is a
       different thing — said on stderr, and answered with a non-zero code. */
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buf: Vec<u8> = Vec::new();
    let mut code = 0;
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) => break, // end of input: the caller is done with us
            Ok(_) => {}
            Err(e) => {
                eprintln!("GardenfellCLI serve: stdin: {e}");
                code = 2;
                break;
            }
        }
        let line = String::from_utf8_lossy(&buf).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if job_tx.send(line).is_err() {
            break; // every worker is gone
        }
    }
    drop(job_tx);
    for w in workers {
        let _ = w.join();
    }
    let _ = writer.join();
    code
}

fn main() { std::process::exit(run()) }

/* Round 18bk — the two registries a command has to be in, checked against each other.
   
   `plugins_probe` shipped in round 18bi defined, dispatched here, and covered by 41 tests,
   and the desktop app still answered "Command plugins_probe not found". It was never added
   to `generate_handler!`, and nothing could have caught that: `wraithguard_viewer_check_commands` rewrites
   `main.rs` by cutting everything from `fn main()` onward, which is precisely where the
   registration lives, and every browser suite talks to `serve` rather than to Tauri. So the
   one list that decides what the shipped program can do was the one list under no test at
   all. A whole round of green gates said nothing.

   These read the two files as text, which is the only way to see a macro list that never
   reaches this crate's compiler. */
#[cfg(test)]
mod registry {
    /// commands.rs and its pages' files (`commands/`), as one text: the commands are looked
    /// for in all of it, the `generate_handler!` list is commands.rs's own.
    const MAIN: &str = concat!(
        include_str!("../../../src/cellviewer/commands.rs"),
        "\n",
        include_str!("../../../src/cellviewer/commands/conflicts.rs"),
        include_str!("../../../src/cellviewer/commands/editor.rs"),
        include_str!("../../../src/cellviewer/commands/help.rs"),
        include_str!("../../../src/cellviewer/commands/land.rs"),
        include_str!("../../../src/cellviewer/commands/loadorder.rs"),
        include_str!("../../../src/cellviewer/commands/sound.rs"),
        include_str!("../../../src/cellviewer/commands/tools.rs"),
    );
    const SERVE: &str = include_str!("wg-view-serve.rs");

    /// Every `#[tauri::command]` function in the shell, by name.
    fn commands() -> Vec<String> {
        let mut out = Vec::new();
        let mut lines = MAIN.lines().peekable();
        while let Some(l) = lines.next() {
            if !l.trim_start().starts_with("#[tauri::command") {
                continue;
            }
            // The `fn` line, possibly after further attributes.
            for next in lines.by_ref() {
                let t = next.trim_start();
                if t.starts_with('#') || t.starts_with("//") || t.is_empty() {
                    continue;
                }
                let t = t.strip_prefix("pub(super) ").or_else(|| t.strip_prefix("pub ")).unwrap_or(t);
                if let Some(rest) = t.strip_prefix("fn ") {
                    let name: String =
                        rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if !name.is_empty() {
                        out.push(name);
                    }
                }
                break;
            }
        }
        out
    }

    /// The names inside `invoke_handler(tauri::generate_handler![ … ])`.
    fn registered() -> Vec<String> {
        let at = MAIN
            .find("invoke_handler(tauri::generate_handler![")
            .expect("no generate_handler! in commands.rs — has the shell been restructured?");
        let rest = &MAIN[at..];
        let end = rest.find("])").expect("unterminated generate_handler!");
        rest[..end]
            .lines()
            .skip(1)
            .filter_map(|l| {
                let t = l.trim().trim_end_matches(',');
                (!t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| t.to_string())
            })
            .collect()
    }

    #[test]
    fn every_command_the_shell_defines_is_one_the_shell_registers() {
        /* The check that would have caught round 18bi's miss before it reached Robin.
           An unregistered command compiles, tests clean through `serve`, and then answers
           "Command <name> not found" the first time somebody uses the program. */
        let reg = registered();
        let missing: Vec<String> = commands().into_iter().filter(|c| !reg.contains(c)).collect();
        assert!(
            missing.is_empty(),
            "these are #[tauri::command] but not in generate_handler!, so the desktop app \
             cannot call them: {missing:?} — add each to the list in `builder()`"
        );
    }

    #[test]
    fn every_command_the_shell_registers_is_one_it_defines() {
        // The other way round: a name left in the list after its function was renamed or
        // removed fails the Tauri build with a macro error that names no useful cause.
        let cmds = commands();
        let stale: Vec<String> = registered().into_iter().filter(|r| !cmds.contains(r)).collect();
        assert!(stale.is_empty(), "named in generate_handler! but no such command: {stale:?}");
    }

    /// The commands serve's dispatch does not reach, on purpose: each starts another program
    /// (a second viewer, the system's browser) or ends this one, which a test server must not.
    /// A new command goes into the dispatch, not onto this list.
    const NOT_SERVED: &[&str] = &["help_open_link", "editor_preview_patch", "app_restart"];

    #[test]
    fn every_command_is_reachable_through_serve_as_well() {
        /* Every browser suite talks to `serve`, so a command the dispatch does not reach is
           a command no test can exercise however many are written. Reached either under its
           own name (`"foo" => … t::foo(…)`) or under another — `paint_layer_overrides` is
           the page's name for `paint_layer_set` — so what is checked is that the function
           is called, not what the arm is spelled. The ones that must not be are `NOT_SERVED`
           (this test failed on 54 of them, unseen: CI built this crate and never ran it). */
        let reached = |c: &str| SERVE.contains(&format!("t::{c}("));
        let unreached: Vec<String> = commands().into_iter().filter(|c| !reached(c) && !NOT_SERVED.contains(&c.as_str())).collect();
        assert!(
            unreached.is_empty(),
            "defined in the shell but never called from serve's dispatch, so no browser \
             test can reach them: {unreached:?}"
        );
        let cmds = commands();
        let stale: Vec<&&str> = NOT_SERVED.iter().filter(|n| reached(n) || !cmds.iter().any(|c| c == *n)).collect();
        assert!(stale.is_empty(), "on NOT_SERVED but served now, or no longer a command: {stale:?}");
    }
}
