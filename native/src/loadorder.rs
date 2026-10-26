//! The load order's rules, sort, checks, export, rule maker and updates, answered by
//! `ordercore` (viewer-shell/ordercore, port step 4.2 slice 2j): what `wraithguard/rules`,
//! `wraithguard/sort`, `wraithguard/momw.py`, `wraithguard/configurator` and
//! `wraithguard/net/updaters.py` call, so Python and the viewer read a load order one way; and
//! since 2k the sort's inputs (`ordercore::inputs`): the subset, the customizations TOML, the
//! mods-folder scan, grass, the cfg's orphans, a list's data paths, anchors and the master check.
//!
//! Where `ordercore` declines (`Unsupported`), these raise `OrderUnsupportedError` and the
//! Python caller keeps its own answer (`wraithguard/ordercore_bridge.py`).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use pyo3::exceptions::{PyException, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};

use ordercore::momw::Entry;
use ordercore::patterns::Unsupported;

pyo3::create_exception!(
    wraithguard_native,
    OrderUnsupportedError,
    PyException,
    "ordercore does not read this input; the caller keeps the Python answer."
);

fn declined(_: Unsupported) -> PyErr {
    OrderUnsupportedError::new_err("not read by ordercore")
}

// ---------------------------------------------------------------- 2a: rule files

/// `order_parse_rules(text)`: the ordering blocks of a rule file's text
/// (`ordercore::rules::parse_text`), as `parse_mlox_text` gives them: `([(keyword, names)],
/// skipped)`.
#[pyfunction]
fn order_parse_rules(text: &str) -> (Vec<(String, Vec<String>)>, usize) {
    let parsed = ordercore::rules::parse_text(text);
    let blocks = parsed.blocks.into_iter().map(|b| (b.kind.as_str().to_string(), b.names)).collect();
    (blocks, parsed.skipped)
}

// ---------------------------------------------------------------- 2c/2d: the sort

/// `order_build_and_sort(base, subset, blocks, masters, nearstart, nearend)`: the sort
/// (`ordercore::order::build_and_sort`): `(order, anchors, conflicts)`, `anchors` keyed by the
/// custom in lower case with `(how, anchor or None)`, `conflicts` the refused rule edges.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn order_build_and_sort(
    py: Python<'_>,
    base: Vec<String>,
    subset: Vec<String>,
    blocks: Vec<(Vec<String>, i64)>,
    masters: BTreeMap<String, Vec<String>>,
    nearstart: Vec<String>,
    nearend: Vec<String>,
) -> PyResult<(Vec<String>, BTreeMap<String, (String, Option<String>)>, Vec<(String, String)>)> {
    let sorted = py
        .detach(|| ordercore::order::build_and_sort(&base, &subset, &blocks, &masters, &nearstart, &nearend))
        .map_err(declined)?;
    let anchors = sorted.anchors.into_iter().map(|(k, (how, a))| (k, (how.as_str().to_string(), a))).collect();
    Ok((sorted.order, anchors, sorted.conflicts))
}

// ---------------------------------------------------------------- 2e: predicate rules

/// The plugin files, asked of a Python object with `usable()`, `exists(name)`, `size(name)`
/// and `description(name)` (`wraithguard.rules.predicates._PluginFacts`). A method that raises
/// answers "unknown": not usable, not there, no size, no description.
struct PyPlugins<'py> {
    obj: Bound<'py, PyAny>,
}

impl ordercore::predicates::Plugins for PyPlugins<'_> {
    fn usable(&self) -> bool {
        self.obj.call_method0("usable").and_then(|v| v.extract()).unwrap_or(false)
    }
    fn exists(&self, name: &str) -> bool {
        self.obj.call_method1("exists", (name,)).and_then(|v| v.extract()).unwrap_or(false)
    }
    fn size(&self, name: &str) -> Option<u64> {
        self.obj.call_method1("size", (name,)).and_then(|v| v.extract()).unwrap_or(None)
    }
    fn description(&self, name: &str) -> String {
        self.obj.call_method1("description", (name,)).and_then(|v| v.extract()).unwrap_or_default()
    }
}

/// `order_check_predicates(rules_text, final_order, subset_origins, plugins)`: the warnings
/// the predicate rules give for a sorted order (`ordercore::predicates::check_predicates`).
#[pyfunction]
fn order_check_predicates(
    rules_text: &str,
    final_order: Vec<String>,
    subset_origins: BTreeMap<String, String>,
    plugins: Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let facts = PyPlugins { obj: plugins };
    ordercore::predicates::check_predicates(rules_text, &final_order, &subset_origins, &facts).map_err(declined)
}

// ---------------------------------------------------------------- 2f: MOMW's plugin-order.yml

fn entry_dict<'py>(py: Python<'py>, e: &Entry) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("file_name", &e.file_name)?;
    d.set_item("for_mod", &e.for_mod)?;
    d.set_item("on_lists", &e.on_lists)?;
    d.set_item("needs_cleaning", e.needs_cleaning)?;
    Ok(d)
}

/// An entry from the dict `wraithguard.momw` keeps it as.
fn entry_of(d: &Bound<'_, PyAny>) -> PyResult<Entry> {
    Ok(Entry {
        file_name: d.get_item("file_name")?.extract()?,
        for_mod: d.get_item("for_mod")?.extract()?,
        on_lists: d.get_item("on_lists")?.extract()?,
        needs_cleaning: d.get_item("needs_cleaning")?.extract()?,
    })
}

fn entries_of(list: &Bound<'_, PyAny>) -> PyResult<Vec<Entry>> {
    list.try_iter()?.map(|e| entry_of(&e?)).collect()
}

/// `order_momw_parse(data)`: `plugin-order.yml`'s entries from its bytes, as dicts
/// (`ordercore::momw::parse_bytes`); raises `ValueError` with where it went wrong for a file
/// that is not MOMW's.
#[pyfunction]
fn order_momw_parse<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let entries = ordercore::momw::parse_bytes(data).map_err(|e| PyValueError::new_err(e.0))?;
    entries.iter().map(|e| entry_dict(py, e)).collect()
}

/// `order_momw_curated_for_list(entries, list_name)`: `(lower-cased set, ordered names)`.
#[pyfunction]
fn order_momw_curated_for_list(entries: Bound<'_, PyAny>, list_name: &str) -> PyResult<(HashSet<String>, Vec<String>)> {
    Ok(ordercore::momw::curated_for_list(&entries_of(&entries)?, list_name))
}

/// `order_momw_needs_cleaning(entries)`: the lower-cased names flagged for cleaning.
#[pyfunction]
fn order_momw_needs_cleaning(entries: Bound<'_, PyAny>) -> PyResult<HashSet<String>> {
    Ok(ordercore::momw::needs_cleaning_set(&entries_of(&entries)?))
}

/// `order_momw_post_sort_findings(final_order, base, entries, curated_set, curated_order,
/// declared_lower, list_name)`: the notes for a finished sort.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn order_momw_post_sort_findings(
    final_order: Option<Vec<String>>,
    base: Vec<String>,
    entries: Bound<'_, PyAny>,
    curated_set: HashSet<String>,
    curated_order: Vec<String>,
    declared_lower: HashSet<String>,
    list_name: Option<String>,
) -> PyResult<Vec<String>> {
    let entries = entries_of(&entries)?;
    Ok(ordercore::momw::post_sort_findings(
        final_order.as_deref(),
        &base,
        &entries,
        &curated_set,
        &curated_order,
        &declared_lower,
        list_name.as_deref(),
    ))
}

// ---------------------------------------------------------------- values from Python

/// A Python value as JSON (lists, dicts, strings, numbers, booleans, None).
fn json_of(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    pythonize::depythonize(obj).map_err(|_| OrderUnsupportedError::new_err("not a plain value"))
}

/// Not the shape ordercore reads: the caller keeps the Python answer.
fn shape() -> PyErr {
    OrderUnsupportedError::new_err("an input of a shape ordercore does not read")
}

fn str_of(v: &Value) -> PyResult<String> {
    v.as_str().map(str::to_string).ok_or_else(shape)
}

fn opt_str(v: Option<&Value>) -> PyResult<Option<String>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(x) => str_of(x).map(Some),
    }
}

fn strs_of(v: &Value) -> PyResult<Vec<String>> {
    v.as_array().ok_or_else(shape)?.iter().map(str_of).collect()
}

fn opt_strs(v: Option<&Value>) -> PyResult<Vec<String>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(x) => strs_of(x),
    }
}

// ---------------------------------------------------------------- 2h: the rule maker

use ordercore::authoring::{Expr, Rule};

/// An expression from its plain form: `["plugin", name]`, `["desc", pattern, plugin,
/// negated]`, `["size", size, plugin, negated]`, `["ver", operator, version, plugin]` or
/// `["group", operator, [parts]]` (`wraithguard.rules.authoring.expr_plain`).
fn expr_of(v: &Value) -> PyResult<Expr> {
    let a = v.as_array().ok_or_else(shape)?;
    let at = |i: usize| a.get(i).ok_or_else(shape);
    let flag = |i: usize| at(i)?.as_bool().ok_or_else(shape);
    Ok(match str_of(at(0)?)?.as_str() {
        "plugin" => Expr::Plugin(str_of(at(1)?)?),
        "desc" => Expr::Desc { pattern: str_of(at(1)?)?, plugin: str_of(at(2)?)?, negated: flag(3)? },
        "size" => Expr::Size { size: at(1)?.as_i64().ok_or_else(shape)?, plugin: str_of(at(2)?)?, negated: flag(3)? },
        "ver" => Expr::Ver { operator: str_of(at(1)?)?, version: str_of(at(2)?)?, plugin: str_of(at(3)?)? },
        "group" => Expr::Group {
            operator: str_of(at(1)?)?,
            parts: at(2)?.as_array().ok_or_else(shape)?.iter().map(expr_of).collect::<PyResult<_>>()?,
        },
        _ => return Err(shape()),
    })
}

fn expr_plain(e: &Expr) -> Value {
    match e {
        Expr::Plugin(n) => json!(["plugin", n]),
        Expr::Desc { pattern, plugin, negated } => json!(["desc", pattern, plugin, negated]),
        Expr::Size { size, plugin, negated } => json!(["size", size, plugin, negated]),
        Expr::Ver { operator, version, plugin } => json!(["ver", operator, version, plugin]),
        Expr::Group { operator, parts } => json!(["group", operator, parts.iter().map(expr_plain).collect::<Vec<_>>()]),
    }
}

/// A rule from its plain form (`wraithguard.rules.authoring.rule_plain`).
fn rule_of(v: &Value) -> PyResult<Rule> {
    let field = |k: &str| v.get(k).ok_or_else(shape);
    Ok(Rule {
        kind: str_of(field("kind")?)?,
        plugins: strs_of(field("plugins")?)?,
        expressions: field("expressions")?.as_array().ok_or_else(shape)?.iter().map(expr_of).collect::<PyResult<_>>()?,
        message: str_of(field("message")?)?,
        ref_: str_of(field("ref")?)?,
        priority: field("priority")?.as_i64().ok_or_else(shape)?,
        section: str_of(field("section")?)?,
        comment: str_of(field("comment")?)?,
        force_block: field("force_block")?.as_bool().ok_or_else(shape)?,
    })
}

fn rule_plain(r: &Rule) -> Value {
    json!({
        "kind": r.kind, "plugins": r.plugins, "expressions": r.expressions.iter().map(expr_plain).collect::<Vec<_>>(),
        "message": r.message, "ref": r.ref_, "priority": r.priority, "section": r.section,
        "comment": r.comment, "force_block": r.force_block,
    })
}

fn to_py(py: Python<'_>, v: &Value) -> PyResult<Py<PyAny>> {
    Ok(pythonize::pythonize(py, v).map_err(|e| PyValueError::new_err(e.to_string()))?.unbind())
}

/// `order_render_rule(rule)`: the rule's text (`ordercore::authoring::render_rule`).
#[pyfunction]
fn order_render_rule(rule: Bound<'_, PyAny>) -> PyResult<String> {
    ordercore::authoring::render_rule(&rule_of(&json_of(&rule)?)?).map_err(declined)
}

/// `order_validate_rule(rule)`: `[(severity, message, remedy)]` (`ordercore::authoring::validate`).
#[pyfunction]
fn order_validate_rule(rule: Bound<'_, PyAny>) -> PyResult<Vec<(String, String, String)>> {
    let problems = ordercore::authoring::validate(&rule_of(&json_of(&rule)?)?).map_err(declined)?;
    Ok(problems.into_iter().map(|p| (p.severity.as_str().to_string(), p.message, p.remedy)).collect())
}

/// `order_format_ref(ref)`: a citation as the message carries it.
#[pyfunction]
fn order_format_ref(r#ref: &str) -> PyResult<String> {
    ordercore::authoring::format_ref(r#ref).map_err(declined)
}

fn append_err(e: ordercore::authoring::AppendError) -> PyErr {
    match e {
        ordercore::authoring::AppendError::Refused(m) => PyValueError::new_err(m),
        ordercore::authoring::AppendError::Unsupported => OrderUnsupportedError::new_err("not read by ordercore"),
        ordercore::authoring::AppendError::Io(e) => e.into(),
    }
}

/// `order_append_authored_rule(path, rule)`: write the rule to the user's rule file; the text
/// written. `ValueError` with every error for a rule that has any.
#[pyfunction]
fn order_append_authored_rule(path: PathBuf, rule: Bound<'_, PyAny>) -> PyResult<String> {
    let rule = rule_of(&json_of(&rule)?)?;
    ordercore::authoring::append_authored_rule(&path, &rule, LineEnding::platform()).map_err(append_err)
}

/// `order_append_user_rule(path, keyword, names, comment)`: an ordering rule, as above.
#[pyfunction]
#[pyo3(signature = (path, keyword, names, comment=None))]
fn order_append_user_rule(path: PathBuf, keyword: &str, names: Vec<String>, comment: Option<String>) -> PyResult<String> {
    ordercore::authoring::append_user_rule(&path, keyword, &names, comment.as_deref(), LineEnding::platform())
        .map_err(append_err)
}

/// `order_rule_frozen_conflicts(names, final_order, curated_lower)`: the pairs a proposed
/// `[Order]` would fight the cfg over.
#[pyfunction]
fn order_rule_frozen_conflicts(names: Vec<String>, final_order: Vec<String>, curated_lower: Vec<String>) -> Vec<(String, String)> {
    ordercore::authoring::order_rule_frozen_conflicts(&names, &final_order, &curated_lower)
}

/// `order_derive(which, masters_by_plugin, active, conflicts, minimum_records)`: the proposals
/// of `ordercore::derive` (`which`: `order_from_masters`, `requires_from_masters`,
/// `patch_candidates`, `order_candidates_from_conflicts` or `propose_all`), each a dict of
/// `rule`, `confidence`, `reason`, `evidence`, `alternatives` (rules in their plain form).
#[pyfunction]
fn order_derive(
    py: Python<'_>,
    which: &str,
    masters_by_plugin: Vec<(String, Vec<String>)>,
    active: Vec<String>,
    conflicts: Bound<'_, PyAny>,
    minimum_records: i64,
) -> PyResult<Py<PyAny>> {
    use ordercore::derive::{ConflictRecord, Proposal};
    let records: Vec<ConflictRecord> = json_of(&conflicts)?
        .as_array()
        .ok_or_else(shape)?
        .iter()
        .map(|r| {
            Ok(ConflictRecord {
                plugins: match r.get("plugins") {
                    None | Some(Value::Null) => None,
                    Some(p) => Some(strs_of(p)?),
                },
                rectype: opt_str(r.get("rectype"))?.unwrap_or_default(),
            })
        })
        .collect::<PyResult<_>>()?;
    let got: Vec<Proposal> = match which {
        "order_from_masters" => ordercore::derive::order_from_masters(&masters_by_plugin, &active),
        "requires_from_masters" => ordercore::derive::requires_from_masters(&masters_by_plugin, &active),
        "patch_candidates" => ordercore::derive::patch_candidates(&masters_by_plugin, &active),
        "order_candidates_from_conflicts" => ordercore::derive::order_candidates_from_conflicts(&records, minimum_records),
        "propose_all" => ordercore::derive::propose_all(&masters_by_plugin, &active, &records, minimum_records),
        _ => return Err(PyValueError::new_err(format!("no derivation {which}"))),
    };
    let plain: Vec<Value> = got
        .iter()
        .map(|p| {
            json!({
                "rule": rule_plain(&p.rule), "confidence": p.confidence.as_str(), "reason": p.reason,
                "evidence": p.evidence, "alternatives": p.alternatives.iter().map(rule_plain).collect::<Vec<_>>(),
            })
        })
        .collect();
    to_py(py, &Value::Array(plain))
}

// ---------------------------------------------------------------- 2g: export

use ordercore::export::{Append, CustomBlock, DataInsert, DataLine, LineEnding, Replace, TomlInput};

fn inserts_of(v: &Value) -> PyResult<Vec<DataInsert>> {
    v.as_array()
        .ok_or_else(shape)?
        .iter()
        .map(|d| {
            Ok(DataInsert {
                value: str_of(d.get("value").ok_or_else(shape)?)?,
                after: opt_str(d.get("after"))?,
                before: opt_str(d.get("before"))?,
            })
        })
        .collect()
}

fn data_lines_of(v: &Value) -> PyResult<Vec<DataLine>> {
    v.as_array()
        .ok_or_else(shape)?
        .iter()
        .map(|r| {
            let a = r.as_array().filter(|a| a.len() == 3).ok_or_else(shape)?;
            Ok((str_of(&a[0])?, a[1].as_bool().ok_or_else(shape)?, opt_str(Some(&a[2]))?))
        })
        .collect()
}

fn blocks_of(original: &Value) -> PyResult<Vec<CustomBlock>> {
    let Some(list) = original.get("Customizations") else { return Ok(Vec::new()) };
    if list.is_null() {
        return Ok(Vec::new());
    }
    list.as_array()
        .ok_or_else(shape)?
        .iter()
        .map(|b| {
            if !b.is_object() {
                return Err(shape());
            }
            let entries = |key: &str| -> PyResult<Vec<Value>> {
                match b.get(key) {
                    None | Some(Value::Null) => Ok(Vec::new()),
                    Some(x) => Ok(x.as_array().ok_or_else(shape)?.clone()),
                }
            };
            Ok(CustomBlock {
                list_name: opt_str(b.get("listName"))?,
                remove_data: opt_strs(b.get("removeData"))?,
                remove_content: opt_strs(b.get("removeContent"))?,
                remove_fallback: opt_strs(b.get("removeFallback"))?,
                remove_groundcover: opt_strs(b.get("removeGroundcover"))?,
                replace: entries("replace")?
                    .iter()
                    .map(|r| Ok(Replace { source: opt_str(r.get("source"))?, dest: opt_str(r.get("dest"))? }))
                    .collect::<PyResult<_>>()?,
                append: entries("append")?
                    .iter()
                    .map(|r| Ok(Append { append: opt_str(r.get("append"))?, append_block: opt_str(r.get("appendBlock"))? }))
                    .collect::<PyResult<_>>()?,
            })
        })
        .collect()
}

/// `order_insert_data_paths(data_lines, data_inserts)`: `(lines, notes)`, each line
/// `(text, is_new, source value)`, the notes the Python prints.
#[pyfunction]
fn order_insert_data_paths(data_lines: Vec<String>, data_inserts: Bound<'_, PyAny>) -> PyResult<(Vec<DataLine>, Vec<String>)> {
    let inserts = inserts_of(&json_of(&data_inserts)?)?;
    ordercore::export::insert_data_paths(&data_lines, &inserts).map_err(declined)
}

/// `order_customizations_toml(args)`: `(text, warnings)` (`generate_customizations_toml`);
/// `args` is a dict of its arguments by name.
#[pyfunction]
fn order_customizations_toml(args: Bound<'_, PyAny>) -> PyResult<(String, Vec<String>)> {
    let a = json_of(&args)?;
    let get = |k: &str| a.get(k).unwrap_or(&Value::Null);
    let original = get("original_data");
    let blocks = if original.is_null() { Vec::new() } else { blocks_of(original)? };
    let order = opt_strs(Some(get("final_content_order")))?;
    let subset = opt_strs(Some(get("subset_set")))?;
    let values: BTreeMap<String, String> = match get("original_content_values") {
        Value::Null => BTreeMap::new(),
        Value::Object(m) => m.iter().map(|(k, v)| Ok((k.clone(), str_of(v)?))).collect::<PyResult<_>>()?,
        _ => return Err(shape()),
    };
    let data_result = match get("data_result_tuples") {
        Value::Null => None,
        v => Some(data_lines_of(v)?),
    };
    let raw = match get("raw_data_inserts") {
        Value::Null => None,
        v => Some(inserts_of(v)?),
    };
    let replace_dest: HashSet<String> = opt_strs(Some(get("replace_dest_names")))?.into_iter().collect();
    let user_vals = opt_strs(Some(get("user_data_values")))?;
    let list_name = opt_str(Some(get("list_name")))?;
    let remove_content = opt_strs(Some(get("remove_content")))?;
    let remove_data = opt_strs(Some(get("remove_data")))?;
    let anchors: BTreeMap<String, (String, Option<String>)> = match get("custom_anchors") {
        Value::Null => BTreeMap::new(),
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| {
                let p = v.as_array().filter(|p| p.len() == 2).ok_or_else(shape)?;
                Ok((k.clone(), (str_of(&p[0])?, opt_str(Some(&p[1]))?)))
            })
            .collect::<PyResult<_>>()?,
        _ => return Err(shape()),
    };
    let grass = opt_strs(Some(get("new_groundcover")))?;
    let toml = ordercore::export::generate_customizations_toml(&TomlInput {
        original_blocks: &blocks,
        final_content_order: &order,
        subset: &subset,
        original_content_values: &values,
        data_result: data_result.as_deref(),
        raw_data_inserts: raw.as_deref(),
        replace_dest_names: &replace_dest,
        user_data_values: &user_vals,
        list_name: list_name.as_deref(),
        remove_content: &remove_content,
        remove_data: &remove_data,
        custom_anchors: &anchors,
        new_groundcover: &grass,
    })
    .map_err(declined)?;
    Ok((toml.text, toml.warnings))
}

/// `order_plugins_needing_removal(disabled, curated_set, base_order_names)`.
#[pyfunction]
fn order_plugins_needing_removal(disabled: Vec<String>, curated_set: Vec<String>, base_order_names: Vec<String>) -> Vec<String> {
    ordercore::export::plugins_needing_removal(&disabled, &curated_set, &base_order_names)
}

/// `order_data_paths_needing_removal(disabled_data, our_values, base_data_norms)`.
#[pyfunction]
fn order_data_paths_needing_removal(
    disabled_data: Vec<String>,
    our_values: HashSet<String>,
    base_data_norms: HashSet<String>,
) -> PyResult<Vec<String>> {
    ordercore::export::data_paths_needing_removal(&disabled_data, &our_values, &base_data_norms).map_err(declined)
}

/// `order_read_groundcover_names(lines)`.
#[pyfunction]
fn order_read_groundcover_names(lines: Vec<String>) -> PyResult<Vec<String>> {
    ordercore::export::read_groundcover_names(&lines).map_err(declined)
}

/// `order_splice_cfg_lines(lines, segments)`.
#[pyfunction]
fn order_splice_cfg_lines(lines: Vec<String>, segments: Vec<(Vec<usize>, Vec<String>)>) -> Vec<String> {
    ordercore::export::splice_cfg_lines(&lines, &segments)
}

// ---------------------------------------------------------------- 2i: updates

use ordercore::updates::{Env, FetchFail, Transport, UpdateError};

/// The network, as a Python callable `get(url, timeout, max_bytes) -> bytes` that raises on a
/// failed request (its text goes into the report).
struct PyTransport<'py> {
    get: Bound<'py, PyAny>,
}

impl Transport for PyTransport<'_> {
    fn get(&self, url: &str, timeout_secs: u64, max_bytes: usize) -> Result<Vec<u8>, String> {
        let got = self.get.call1((url, timeout_secs, max_bytes)).map_err(|e| e.value(self.get.py()).to_string())?;
        got.extract::<Vec<u8>>().map_err(|e| e.to_string())
    }
}

fn update_err(e: UpdateError) -> PyErr {
    match e {
        UpdateError::Unsupported => OrderUnsupportedError::new_err("not read by ordercore"),
        UpdateError::Io(e) => e.into(),
    }
}

/// The three environment variables and the `.bak` stamp, given by the caller.
#[derive(FromPyObject)]
#[pyo3(from_item_all)]
struct EnvArgs {
    stamp: String,
    plugin_order_url: Option<String>,
    rules_url_template: Option<String>,
    momw_api_host: Option<String>,
}

impl EnvArgs {
    fn env(&self) -> Env<'_> {
        Env {
            stamp: &self.stamp,
            plugin_order_url: self.plugin_order_url.as_deref(),
            rules_url_template: self.rules_url_template.as_deref(),
            momw_api_host: self.momw_api_host.as_deref(),
        }
    }
}

/// `order_fetch_url_bytes(get, url, timeout, max_bytes)`: the checked download; `ValueError`
/// for a refused URL or body, `OSError` for a failed request.
#[pyfunction]
fn order_fetch_url_bytes<'py>(py: Python<'py>, get: Bound<'py, PyAny>, url: &str, timeout: u64, max_bytes: usize) -> PyResult<Bound<'py, PyBytes>> {
    let t = PyTransport { get };
    match ordercore::updates::fetch_url_bytes(&t, url, timeout, max_bytes) {
        Ok(d) => Ok(PyBytes::new(py, &d)),
        Err(FetchFail::Value(m)) => Err(PyValueError::new_err(m)),
        Err(FetchFail::Os(m)) => Err(PyOSError::new_err(m)),
        Err(FetchFail::Unsupported) => Err(OrderUnsupportedError::new_err("not read by ordercore")),
    }
}

/// `order_update_plugin_order_yml(path, urls, timeout, count, env, get)`: the report lines.
/// `count(data) -> int` reads a download as the file would be read (the caller's
/// `parse_plugin_order_yml`, which is `ordercore::momw`'s reader) and raises for one that is
/// not MOMW's; nothing on disk is touched before it has passed.
#[pyfunction]
fn order_update_plugin_order_yml(
    path: PathBuf,
    urls: Option<Vec<String>>,
    timeout: u64,
    count: Bound<'_, PyAny>,
    env: EnvArgs,
    get: Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let t = PyTransport { get };
    let py = count.py();
    let parse = |data: &[u8]| -> Result<usize, String> {
        count.call1((PyBytes::new(py, data),)).and_then(|n| n.extract()).map_err(|e| e.value(py).to_string())
    };
    ordercore::updates::update_plugin_order_yml(&path, urls.as_deref(), timeout, &env.env(), &t, &parse).map_err(update_err)
}

/// `order_fetch_list_data_paths(path, list_name, host, timeout, env, get)`: the report lines.
#[pyfunction]
fn order_fetch_list_data_paths(
    path: PathBuf,
    list_name: &str,
    host: Option<String>,
    timeout: u64,
    env: EnvArgs,
    get: Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let t = PyTransport { get };
    ordercore::updates::fetch_list_data_paths(&path, list_name, host.as_deref(), timeout, &env.env(), &t).map_err(update_err)
}

/// `order_update_rule_files(rule_paths, url_template, timeout, env, get)`: the report lines.
#[pyfunction]
fn order_update_rule_files(
    rule_paths: Vec<PathBuf>,
    url_template: Option<String>,
    timeout: u64,
    env: EnvArgs,
    get: Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let t = PyTransport { get };
    ordercore::updates::update_rule_files(&rule_paths, url_template.as_deref(), timeout, &env.env(), &t).map_err(update_err)
}

/// `order_rule_file_ages(rule_paths, now)`: `(file name, age in days or None)`.
#[pyfunction]
fn order_rule_file_ages(rule_paths: Vec<PathBuf>, now: f64) -> Vec<(String, Option<i64>)> {
    ordercore::updates::rule_file_ages(&rule_paths, now)
}

// ---------------------------------------------------------------- 2k: the sort's inputs

use ordercore::inputs;

/// `order_subset_from_lines(lines)`: `(plugins, data folders, skipped entries)`
/// (`extract_subset_from_lines`; the caller warns about the skipped ones).
#[pyfunction]
fn order_subset_from_lines(lines: Vec<String>) -> PyResult<(Vec<String>, Vec<String>, Vec<String>)> {
    let got = inputs::subset_from_lines(&lines).map_err(declined)?;
    Ok((got.plugins, got.data, got.skipped))
}

/// `order_groundcover_declarations(lines)`: `(names, declared entries that are not plugins)`
/// (`extract_groundcover_declarations`).
#[pyfunction]
fn order_groundcover_declarations(lines: Vec<String>) -> PyResult<(Vec<String>, Vec<String>)> {
    inputs::groundcover_declarations(&lines).map_err(declined)
}

/// `order_subset_from_toml(text)`: the subset file's TOML form, `(plugins, data folders, skipped,
/// groundcover)`. Declines a file tomllib would refuse, so the Python gives its own message.
#[pyfunction]
fn order_subset_from_toml(text: &str) -> PyResult<(Vec<String>, Vec<String>, Vec<String>, Vec<String>)> {
    let got = inputs::subset_from_toml(text).map_err(declined)?;
    Ok((got.entries.plugins, got.entries.data, got.entries.skipped, got.groundcover))
}

type Insert = (String, Option<String>, Option<String>);

/// `order_customizations(text)`: a `customizations.toml` (`extract_subset_from_toml`), as
/// `(subset, [(value, after, before)], replace destinations, {plugin: listName})`.
#[pyfunction]
fn order_customizations(text: &str) -> PyResult<(Vec<String>, Vec<Insert>, Vec<String>, BTreeMap<String, String>)> {
    let got = inputs::customizations(text).map_err(declined)?;
    let data = got.data.into_iter().map(|d| (d.value, d.after, d.before)).collect();
    Ok((got.subset, data, got.replace_dest, got.list_names))
}

/// `order_hold_back_groundcover(subset, groundcover)`: `(kept, held back)`.
#[pyfunction]
fn order_hold_back_groundcover(subset: Vec<String>, groundcover: Vec<String>) -> (Vec<String>, Vec<String>) {
    inputs::hold_back_groundcover(&subset, &groundcover)
}

/// `order_all_scan_dirs(data_order, raw_values, insert_values)` (`all_scan_dirs`).
#[pyfunction]
fn order_all_scan_dirs(data_order: Vec<String>, raw_values: Vec<String>, insert_values: Vec<String>) -> PyResult<Vec<String>> {
    inputs::all_scan_dirs(&data_order, &raw_values, &insert_values).map_err(declined)
}

/// `order_pending_custom_dirs(raw_values, insert_values)` (`pending_custom_dirs`).
#[pyfunction]
fn order_pending_custom_dirs(raw_values: Vec<String>, insert_values: Vec<String>) -> Vec<String> {
    inputs::pending_custom_dirs(&raw_values, &insert_values)
}

/// `order_scan_mod_directories(start)`: `(lines, folders, plugins, start made absolute)`
/// (`scan_mod_directories`, without the file and the report, which the caller writes).
#[pyfunction]
fn order_scan_mod_directories(py: Python<'_>, start: &str) -> (Vec<String>, usize, usize, String) {
    let scan = py.detach(|| inputs::scan_mod_directories(start));
    (scan.lines, scan.folders, scan.plugins, scan.start)
}

/// `order_parse_data_paths_cache(text)`: a list's cached data-path tails.
#[pyfunction]
fn order_parse_data_paths_cache(text: &str) -> Vec<String> {
    inputs::parse_data_paths_cache(text)
}

/// `order_managed_data_path_norms(tails, cfg_values)` (`managed_cfg_data_path_norms`).
#[pyfunction]
fn order_managed_data_path_norms(tails: Vec<String>, cfg_values: Vec<String>) -> HashSet<String> {
    inputs::managed_cfg_data_path_norms(&tails, &cfg_values)
}

/// `order_reconcile_data_paths(tails, cfg_values, report_missing)` (`reconcile_data_paths`).
#[pyfunction]
fn order_reconcile_data_paths(tails: Vec<String>, cfg_values: Vec<String>, report_missing: bool) -> Vec<String> {
    inputs::reconcile_data_paths(&tails, &cfg_values, report_missing)
}

/// `order_cfg_orphans(subset, base_order, data_order, curated, needs_cleaning, declared,
/// groundcover, declared_data, tails)`: the `--subset-from-cfg` pull, `(new plugins, orphan
/// data= values or None without a cache)`. The name sets are lower case.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn order_cfg_orphans(
    subset: Vec<String>,
    base_order: Vec<String>,
    data_order: Vec<String>,
    curated: HashSet<String>,
    needs_cleaning: HashSet<String>,
    declared: HashSet<String>,
    groundcover: HashSet<String>,
    declared_data: Vec<String>,
    tails: Option<Vec<String>>,
) -> PyResult<(Vec<String>, Option<Vec<String>>)> {
    let q = inputs::OrphanQuery {
        subset: &subset,
        base_order_names: &base_order,
        data_order: &data_order,
        curated_lower: curated,
        needs_cleaning_lower: needs_cleaning,
        declared_lower: declared,
        groundcover_lower: groundcover,
        declared_data: &declared_data,
        tails: tails.as_deref(),
    };
    inputs::cfg_orphans(&q).map_err(declined)
}

type Anchors = Vec<(Option<String>, Option<String>)>;
type AnchorNote = (String, String, String, String);

/// `order_infer_data_path_anchors(inserts, data_order, final_order, base_dir)`: each insert's
/// `(after, before)` once anchored, and `(value, mode, anchor, via)` for each anchor chosen.
#[pyfunction]
fn order_infer_data_path_anchors(
    py: Python<'_>,
    inserts: Bound<'_, PyAny>,
    data_order: Vec<String>,
    final_order: Vec<String>,
    base_dir: Option<PathBuf>,
) -> PyResult<(Anchors, Vec<AnchorNote>)> {
    let mut items = inserts_of(&json_of(&inserts)?)?;
    let notes = py
        .detach(|| inputs::infer_data_path_anchors(&mut items, &data_order, &final_order, base_dir.as_deref()))
        .map_err(declined)?;
    Ok((
        items.into_iter().map(|i| (i.after, i.before)).collect(),
        notes.into_iter().map(|n| (n.value, n.mode.to_string(), n.anchor, n.via)).collect(),
    ))
}

type MasterReport = (Vec<String>, Vec<String>, Vec<String>, usize, Vec<String>);

/// `order_check_missing_masters(active_order, dirs, origins)`: the plugin index is built over
/// `dirs` as `PluginFileIndex` builds it; `(missing, order problems, size notes, checked,
/// problem plugins)` (`check_missing_masters`).
#[pyfunction]
fn order_check_missing_masters(
    py: Python<'_>,
    active_order: Vec<String>,
    dirs: Vec<String>,
    origins: std::collections::HashMap<String, String>,
) -> MasterReport {
    let got = py.detach(|| {
        let index = inputs::PluginIndex::build(&dirs);
        inputs::check_missing_masters(&active_order, &index, &origins)
    });
    (got.missing, got.order_problems, got.size_notes, got.checked, got.problem_plugins)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("OrderUnsupportedError", m.py().get_type::<OrderUnsupportedError>())?;
    m.add_function(wrap_pyfunction!(order_parse_rules, m)?)?;
    m.add_function(wrap_pyfunction!(order_build_and_sort, m)?)?;
    m.add_function(wrap_pyfunction!(order_check_predicates, m)?)?;
    m.add_function(wrap_pyfunction!(order_momw_parse, m)?)?;
    m.add_function(wrap_pyfunction!(order_momw_curated_for_list, m)?)?;
    m.add_function(wrap_pyfunction!(order_momw_needs_cleaning, m)?)?;
    m.add_function(wrap_pyfunction!(order_momw_post_sort_findings, m)?)?;
    m.add_function(wrap_pyfunction!(order_render_rule, m)?)?;
    m.add_function(wrap_pyfunction!(order_validate_rule, m)?)?;
    m.add_function(wrap_pyfunction!(order_format_ref, m)?)?;
    m.add_function(wrap_pyfunction!(order_append_authored_rule, m)?)?;
    m.add_function(wrap_pyfunction!(order_append_user_rule, m)?)?;
    m.add_function(wrap_pyfunction!(order_rule_frozen_conflicts, m)?)?;
    m.add_function(wrap_pyfunction!(order_derive, m)?)?;
    m.add_function(wrap_pyfunction!(order_insert_data_paths, m)?)?;
    m.add_function(wrap_pyfunction!(order_customizations_toml, m)?)?;
    m.add_function(wrap_pyfunction!(order_plugins_needing_removal, m)?)?;
    m.add_function(wrap_pyfunction!(order_data_paths_needing_removal, m)?)?;
    m.add_function(wrap_pyfunction!(order_read_groundcover_names, m)?)?;
    m.add_function(wrap_pyfunction!(order_splice_cfg_lines, m)?)?;
    m.add_function(wrap_pyfunction!(order_fetch_url_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(order_update_plugin_order_yml, m)?)?;
    m.add_function(wrap_pyfunction!(order_fetch_list_data_paths, m)?)?;
    m.add_function(wrap_pyfunction!(order_update_rule_files, m)?)?;
    m.add_function(wrap_pyfunction!(order_rule_file_ages, m)?)?;
    m.add_function(wrap_pyfunction!(order_subset_from_lines, m)?)?;
    m.add_function(wrap_pyfunction!(order_groundcover_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(order_subset_from_toml, m)?)?;
    m.add_function(wrap_pyfunction!(order_customizations, m)?)?;
    m.add_function(wrap_pyfunction!(order_hold_back_groundcover, m)?)?;
    m.add_function(wrap_pyfunction!(order_all_scan_dirs, m)?)?;
    m.add_function(wrap_pyfunction!(order_pending_custom_dirs, m)?)?;
    m.add_function(wrap_pyfunction!(order_scan_mod_directories, m)?)?;
    m.add_function(wrap_pyfunction!(order_parse_data_paths_cache, m)?)?;
    m.add_function(wrap_pyfunction!(order_managed_data_path_norms, m)?)?;
    m.add_function(wrap_pyfunction!(order_reconcile_data_paths, m)?)?;
    m.add_function(wrap_pyfunction!(order_cfg_orphans, m)?)?;
    m.add_function(wrap_pyfunction!(order_infer_data_path_anchors, m)?)?;
    m.add_function(wrap_pyfunction!(order_check_missing_masters, m)?)?;
    Ok(())
}
