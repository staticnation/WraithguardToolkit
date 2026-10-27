//! The cell map (`--cell-map`): which mods touch which cells, as one self-contained HTML page
//! (`wraithguard/viz/cellmap.py`, with `build_cell_coverage` and the coverage ramp of
//! `viz/palette.py`). The same page byte for byte; its stylesheet and script are the Python's
//! (`cellmap.css`, `cellmap.js`).
//!
//! The viewer has a live cell map of its own (the Map's coverage); this is the file one keeps,
//! moves or attaches to a bug report.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use editcore::plugin::CellRef;

pub const TITLE: &str = "Wraithguard Toolkit - Cell Map";

const CSS: &str = include_str!("cellmap.css");
const JS: &str = include_str!("cellmap.js");

/// Largest exterior coordinate treated as real; beyond it a plugin is almost surely corrupt.
const CELL_GRID_LIMIT: i64 = 4096;
const CELL_PX: i64 = 12;
const STEP_PX: i64 = 13;

/// Which mods touch which cells (`build_cell_coverage`), each in load order, the cells in the
/// order first met.
#[derive(Default)]
pub struct Coverage {
    pub exterior: Vec<((i64, i64), Vec<String>)>,
    pub interior: Vec<(String, Vec<String>)>,
    pub scanned: usize,
    pub unreadable: Vec<String>,
    pub subset_lower: HashSet<String>,
}

/// Read each plugin's cells. A plugin whose file is not found is unreadable; one that does not
/// read touches nothing, as the Python's session answers.
pub fn coverage(order: &[String], find: &(dyn Fn(&str) -> Option<PathBuf> + Sync), subset: &[String]) -> Coverage {
    let mut cov = Coverage { subset_lower: subset.iter().map(|s| s.to_lowercase()).collect(), ..Default::default() };
    let mut ext_at: HashMap<(i64, i64), usize> = HashMap::new();
    let mut int_at: HashMap<String, usize> = HashMap::new();
    for plugin in order {
        let Some(path) = find(plugin) else {
            cov.unreadable.push(plugin.clone());
            continue;
        };
        cov.scanned += 1;
        let cells = std::fs::read(&path).ok().and_then(|b| editcore::plugin::keys_and_cells(&b).ok()).map(|(_, c)| c).unwrap_or_default();
        let (mut seen_e, mut seen_i) = (HashSet::new(), HashSet::new());
        for cell in cells {
            match cell {
                CellRef::Ext(x, y) => {
                    if seen_e.insert((x, y)) {
                        let i = *ext_at.entry((x, y)).or_insert_with(|| {
                            cov.exterior.push(((x, y), Vec::new()));
                            cov.exterior.len() - 1
                        });
                        cov.exterior[i].1.push(plugin.clone());
                    }
                }
                CellRef::Int(name) => {
                    if seen_i.insert(name.clone()) {
                        let i = *int_at.entry(name.clone()).or_insert_with(|| {
                            cov.interior.push((name.clone(), Vec::new()));
                            cov.interior.len() - 1
                        });
                        cov.interior[i].1.push(plugin.clone());
                    }
                }
            }
        }
    }
    cov
}

// ---------------------------------------------------------------- the ramp (viz/palette.py)

const NEUTRAL: &str = "#2c313a";
const COVERAGE_STOPS: [(f64, f64, f64, f64); 7] = [
    (0.00, 0.18, 0.29, 0.39),
    (0.17, 0.16, 0.38, 0.55),
    (0.34, 0.20, 0.50, 0.62),
    (0.50, 0.36, 0.44, 0.68),
    (0.67, 0.55, 0.40, 0.62),
    (0.84, 0.72, 0.44, 0.40),
    (1.00, 0.85, 0.62, 0.24),
];
const SINGLE_MAX: i64 = 5;
const GROUP: i64 = 5;
const MAX_BANDS: usize = 16;

/// Python's `round`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn hex(r: f64, g: f64, b: f64) -> String {
    let c = |v: f64| round_half_even(clamp01(v) * 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

fn ramp(t: f64) -> String {
    for w in COVERAGE_STOPS.windows(2) {
        let ((p0, r0, g0, b0), (p1, r1, g1, b1)) = (w[0], w[1]);
        if t <= p1 {
            let span = if p1 - p0 == 0.0 { 1.0 } else { p1 - p0 };
            let u = (t - p0) / span;
            return hex(r0 + (r1 - r0) * u, g0 + (g1 - g0) * u, b0 + (b1 - b0) * u);
        }
    }
    let (_, r, g, b) = COVERAGE_STOPS[6];
    hex(r, g, b)
}

/// The mods-per-cell bands (`coverage_bands`): `(low, high)`, `high` `None` on an open last band.
fn bands(worst: i64) -> Vec<(i64, Option<i64>)> {
    if worst < 1 {
        return Vec::new();
    }
    let mut out: Vec<(i64, Option<i64>)> = (1..=worst.min(SINGLE_MAX)).map(|n| (n, Some(n))).collect();
    let mut low = SINGLE_MAX + 1;
    while low <= worst {
        let high = low + GROUP - 1;
        if out.len() + 1 >= MAX_BANDS && high < worst {
            out.push((low, None));
            return out;
        }
        out.push((low, Some(high.min(worst))));
        low = high + 1;
    }
    out
}

fn band_index(count: i64, worst: i64) -> usize {
    let b = bands(worst);
    if b.is_empty() {
        return 0;
    }
    for (i, (low, high)) in b.iter().enumerate() {
        if count >= *low && high.is_none_or(|h| count <= h) {
            return i;
        }
    }
    if count < 1 { 0 } else { b.len() - 1 }
}

/// A count's band colour (`coverage_heat`).
fn heat(count: i64, worst: i64) -> String {
    if count <= 0 {
        return NEUTRAL.to_string();
    }
    let b = bands(worst);
    if b.len() <= 1 {
        let (_, r, g, bl) = COVERAGE_STOPS[0];
        return hex(r, g, bl);
    }
    ramp(clamp01(band_index(count, worst) as f64 / (b.len() - 1) as f64))
}

/// The legend's rows (`coverage_legend_stops`): label, colour, whether its text is light.
fn legend_stops(worst: i64) -> Vec<(String, String, bool)> {
    bands(worst)
        .into_iter()
        .map(|(low, high)| {
            let color = heat(low, worst);
            let ch = |i: usize| f64::from(u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(0)) / 255.0;
            let luminance = 0.299 * ch(1) + 0.587 * ch(3) + 0.114 * ch(5);
            let label = match high {
                None => format!("{low}+"),
                Some(h) if h == low => low.to_string(),
                Some(h) => format!("{low}-{h}"),
            };
            (label, color, luminance < 0.55)
        })
        .collect()
}

// ---------------------------------------------------------------- the page

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn anchor(x: i64, y: i64) -> String {
    format!("e_{x}_{y}").replace('-', "m")
}

fn modattr(mods: &[String]) -> String {
    esc(&format!("|{}|", mods.iter().map(|m| m.to_lowercase()).collect::<Vec<_>>().join("|")))
}

fn in_bounds((x, y): (i64, i64)) -> bool {
    (-CELL_GRID_LIMIT..=CELL_GRID_LIMIT).contains(&x) && (-CELL_GRID_LIMIT..=CELL_GRID_LIMIT).contains(&y)
}

fn mine(mods: &[String], subset: &HashSet<String>) -> bool {
    mods.iter().any(|m| subset.contains(&m.to_lowercase()))
}

fn focus_options(cov: &Coverage) -> String {
    let mut all: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for mods in cov.exterior.iter().map(|(_, m)| m).chain(cov.interior.iter().map(|(_, m)| m)) {
        for m in mods {
            let low = m.to_lowercase();
            if seen.insert(low.clone()) {
                all.push((low, m.clone()));
            }
        }
    }
    all.sort_by(|a, b| (!cov.subset_lower.contains(&a.0), &a.0).cmp(&(!cov.subset_lower.contains(&b.0), &b.0)));
    all.iter()
        .map(|(low, name)| {
            let star = if cov.subset_lower.contains(low) { " ★" } else { "" };
            format!("<option value=\"{}\">{}{star}</option>", esc(low), esc(name))
        })
        .collect()
}

fn svg_grid(ext: &[&((i64, i64), Vec<String>)], subset: &HashSet<String>, worst: i64) -> String {
    if ext.is_empty() {
        return "<p class=\"sub\">No exterior cells touched.</p>".into();
    }
    let minx = ext.iter().map(|e| e.0.0).min().unwrap_or(0);
    let maxx = ext.iter().map(|e| e.0.0).max().unwrap_or(0);
    let miny = ext.iter().map(|e| e.0.1).min().unwrap_or(0);
    let maxy = ext.iter().map(|e| e.0.1).max().unwrap_or(0);
    let width = (maxx - minx + 1) * STEP_PX;
    let height = (maxy - miny + 1) * STEP_PX;
    let mut rects = String::new();
    for ((gx, gy), mods) in ext.iter().map(|e| (&e.0, &e.1)) {
        let px = (gx - minx) * STEP_PX;
        let py = (maxy - gy) * STEP_PX;
        let tip = format!("({gx}, {gy}) - {} mod(s): {}", mods.len(), mods.join(", "));
        let stroke = if mine(mods, subset) { " stroke=\"#ffd24a\" stroke-width=\"1.4\"" } else { "" };
        let _ = write!(
            rects,
            "<rect x=\"{px}\" y=\"{py}\" width=\"{CELL_PX}\" height=\"{CELL_PX}\" fill=\"{}\"{stroke} class=\"cell\" data-t=\"{}\" data-m=\"{}\" onclick=\"jump('{}')\" ondblclick=\"openPreview('{gx},{gy}')\"></rect>",
            heat(mods.len() as i64, worst),
            esc(&tip),
            modattr(mods),
            anchor(*gx, *gy)
        );
    }
    format!(
        "<div class=\"mapwrap\"><svg width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" xmlns=\"http://www.w3.org/2000/svg\">{rects}</svg></div>"
    )
}

fn exterior_rows(ext: &[&((i64, i64), Vec<String>)], subset: &HashSet<String>) -> String {
    let mut rows: Vec<&&((i64, i64), Vec<String>)> = ext.iter().collect();
    rows.sort_by_key(|e| (std::cmp::Reverse(e.1.len()), e.0));
    let out: String = rows
        .iter()
        .map(|e| {
            let ((gx, gy), mods) = (e.0, &e.1);
            let cls = if mine(mods, subset) { " class=\"cust\"" } else { "" };
            format!(
                "<tr id=\"{}\"{cls} data-m=\"{}\"><td><button class=\"op\" title=\"Open in Cell Preview\" onclick=\"openPreview('{gx},{gy}')\">&#9654;</button> ({gx}, {gy})</td><td>{}</td><td>{}</td></tr>",
                anchor(gx, gy),
                modattr(mods),
                mods.len(),
                esc(&mods.join(", "))
            )
        })
        .collect();
    if out.is_empty() { "<tr><td colspan=3 class=sub>None.</td></tr>".into() } else { out }
}

fn interior_rows(int: &[(String, Vec<String>)], subset: &HashSet<String>) -> String {
    let mut rows: Vec<&(String, Vec<String>)> = int.iter().collect();
    rows.sort_by_key(|e| (std::cmp::Reverse(e.1.len()), e.0.to_lowercase()));
    let out: String = rows
        .iter()
        .map(|(name, mods)| {
            let cls = if mine(mods, subset) { " class=\"cust\"" } else { "" };
            format!(
                "<tr{cls} data-m=\"{}\"><td><button class=\"op\" title=\"Open in Cell Preview\" data-c=\"int:{}\" onclick=\"openPreview(this.dataset.c)\">&#9654;</button> {}</td><td>{}</td><td>{}</td></tr>",
                modattr(mods),
                esc(name),
                esc(name),
                mods.len(),
                esc(&mods.join(", "))
            )
        })
        .collect();
    if out.is_empty() { "<tr><td colspan=3 class=sub>None.</td></tr>".into() } else { out }
}

fn legend(worst: i64) -> String {
    let swatches: String = legend_stops(worst)
        .into_iter()
        .map(|(label, color, dark)| format!("<span style=\"background:{color};color:{}\">{label}</span>", if dark { "#fff" } else { "#111" }))
        .collect();
    format!("<div class=\"legend\">Mods per cell: {swatches} &nbsp;(north up; hover a cell for its mods, click it to jump to the list)</div>")
}

/// The page (`generate_cell_map_html`), stamped `stamped`.
pub fn html(cov: &Coverage, title: &str, stamped: chrono::NaiveDateTime) -> String {
    let ext: Vec<&((i64, i64), Vec<String>)> = cov.exterior.iter().filter(|e| in_bounds(e.0)).collect();
    let dropped = cov.exterior.len() - ext.len();
    let worst = ext.iter().map(|e| e.1.len() as i64).max().unwrap_or(1);
    let ext_conflicts = ext.iter().filter(|e| e.1.len() > 1).count();
    let int_conflicts = cov.interior.iter().filter(|e| e.1.len() > 1).count();
    let dropped_note = if dropped > 0 { format!(" {dropped} cell(s) had out-of-range coordinates and were dropped.") } else { String::new() };
    let t = esc(title);
    let subset = &cov.subset_lower;
    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>{t}</title>
<style>{CSS}</style></head><body>
<div id="tt"></div>
<h1>{t}</h1>
<p class="sub">Scanned {scanned} plugin(s). Exterior: {ne} cell(s)
 touched ({ext_conflicts} by 2+ mods). Interior: {ni} cell(s) touched
 ({int_conflicts} by 2+ mods). Cells your custom mods touch are highlighted
 (gold outline / orange text).{dropped_note}</p>
<p class="sub stamp">Generated {stamp}</p>
{legend}
<div class="focusbar">Focus on mod:
 <select id="focus" onchange="setFocus(this.value)"><option value="">- all mods -</option>{focus}</select>
 <button onclick="document.getElementById('focus').value='';setFocus('')">Clear</button>
 <div id="focusinfo" class="sub"></div></div>
<div class="tabs">
 <button id="b0" class="on" onclick="show(0)">Map</button>
 <button id="b1" onclick="show(1)">Exterior list ({ne})</button>
 <button id="b2" onclick="show(2)">Interior list ({ni})</button>
</div>
<div id="t0" class="tab on">{grid}</div>
<div id="t1" class="tab"><input class="f"
  placeholder="Filter exterior cells / mods..." onkeyup="ff('xt')">
 <div class="listwrap"><table class="list" id="xt"><thead><tr><th>Cell (x, y)</th><th>#</th>
 <th>Mods (load order, last wins)</th></tr></thead>
 <tbody>{xrows}</tbody></table></div></div>
<div id="t2" class="tab"><input class="f"
  placeholder="Filter interior cells / mods..." onkeyup="ff('it')">
 <div class="listwrap"><table class="list" id="it"><thead><tr><th>Cell</th><th>#</th>
 <th>Mods (load order, last wins)</th></tr></thead>
 <tbody>{irows}</tbody></table></div></div>
<script>{JS}</script>
</body></html>
"#,
        scanned = cov.scanned,
        ne = ext.len(),
        ni = cov.interior.len(),
        stamp = stamped.format("%Y-%m-%d %H:%M:%S"),
        legend = legend(worst),
        focus = focus_options(cov),
        grid = svg_grid(&ext, subset, worst),
        xrows = exterior_rows(&ext, subset),
        irows = interior_rows(&cov.interior, subset),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_is_the_pythons() {
        // python: [coverage_heat(n, 40) for n in (1, 3, 5, 6, 40)], coverage_legend_stops(1)
        assert_eq!(bands(40).len(), 12);
        assert_eq!(bands(100).last(), Some(&(56, None)));
        assert_eq!(heat(0, 3), NEUTRAL);
        assert_eq!(heat(1, 1), "#2e4a63");
        assert_eq!(legend_stops(1), vec![("1".to_string(), "#2e4a63".to_string(), true)]);
    }
}
