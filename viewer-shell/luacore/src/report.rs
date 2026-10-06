//! A load-order Lua scan as plain text (was `wraithguard/lua/report.py`'s `render`, which
//! stays as the fallback and the reference this is held to, byte for byte).

/// One finding: script path ("" for the load order), severity, code, line, message.
pub type Row = (String, String, String, usize, String);

/// One script for the closing list: path, flags, interface, engine handlers, missing.
pub type ScriptLine = (String, Vec<String>, Option<String>, Vec<String>, bool);

/// What the header says.
pub struct Header {
    pub openmw: String,
    pub revision: i64,
    pub omwscripts: usize,
    pub scripts: usize,
    pub data_dirs: usize,
    pub api_source: String,
    pub omwaddons: usize,
    pub lual_files: usize,
}

fn order(sev: &str) -> u8 {
    match sev {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        _ => 3,
    }
}

fn mark(sev: &str) -> &str {
    match sev {
        "error" => "ERROR",
        "warn" => "warn ",
        "info" => "info ",
        other => other,
    }
}

/// Every finding, most serious first: by severity, then path (without case), then line.
pub fn sort_findings(found: &mut [Row]) {
    found.sort_by(|a, b| {
        (order(&a.1), a.0.to_lowercase(), a.3).cmp(&(order(&b.1), b.0.to_lowercase(), b.3))
    });
}

/// The report. `found` is every finding (sorted here); `info` keeps the notes.
pub fn render(h: &Header, mut found: Vec<Row>, scripts: &[ScriptLine], info: bool) -> String {
    sort_findings(&mut found);
    let count = |sev: &str| found.iter().filter(|f| f.1 == sev).count();
    let revision = if h.revision != 0 {
        format!("Lua API {}", h.revision)
    } else {
        "Lua API revision not known".to_string()
    };
    let mut lines = vec![
        format!("OpenMW Lua scripts - checked against OpenMW {} ({revision})", h.openmw),
        format!(
            "{} .omwscripts file(s), {} script(s), {} data folder(s)",
            h.omwscripts, h.scripts, h.data_dirs
        ),
        format!("{} error(s), {} warning(s), {} note(s)", count("error"), count("warn"), count("info")),
    ];
    if !h.api_source.is_empty() {
        lines.push(h.api_source.clone());
    }
    if h.omwaddons > 0 {
        lines.push(format!(
            "LUAL records read from {} .omwaddon file(s); {} register scripts.",
            h.omwaddons, h.lual_files
        ));
    }
    lines.push(String::new());
    for (path, sev, code, line, message) in &found {
        if sev == "info" && !info {
            continue;
        }
        let mut place = if path.is_empty() { "(load order)".to_string() } else { path.clone() };
        if *line != 0 {
            place.push_str(&format!(":{line}"));
        }
        lines.push(format!("{} {code:<22} {place}", mark(sev)));
        lines.push(format!("      {message}"));
    }
    lines.push(String::new());
    lines.push("Scripts, in load order:".into());
    for (path, flags, interface, handlers, missing) in scripts {
        let mut tail = String::new();
        if let Some(i) = interface.as_deref().filter(|i| !i.is_empty()) {
            tail.push_str(&format!("  interface {i}"));
        }
        if !handlers.is_empty() {
            let mut hs = handlers.clone();
            hs.sort();
            tail.push_str(&format!("  [{}]", hs.join(", ")));
        }
        if *missing {
            tail.push_str("  (missing)");
        }
        lines.push(format!("  {path}  ({}){tail}", flags.join(", ")));
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_text() {
        let h = Header {
            openmw: "0.51.0".into(),
            revision: 129,
            omwscripts: 1,
            scripts: 1,
            data_dirs: 2,
            api_source: String::new(),
            omwaddons: 0,
            lual_files: 0,
        };
        let row = |p: &str, s: &str, c: &str, l: usize| (p.to_string(), s.to_string(), c.to_string(), l, "m".to_string());
        let found = vec![row("b.lua", "info", "I", 0), row("", "error", "E", 0), row("a.lua", "warn", "W", 3)];
        let scripts = vec![("a.lua".to_string(), vec!["NPC".to_string()], None, vec!["onUpdate".to_string()], false)];
        let text = render(&h, found, &scripts, false);
        let want = [
            "OpenMW Lua scripts - checked against OpenMW 0.51.0 (Lua API 129)".to_string(),
            "1 .omwscripts file(s), 1 script(s), 2 data folder(s)".into(),
            "1 error(s), 1 warning(s), 1 note(s)".into(),
            String::new(),
            format!("ERROR {:<22} (load order)", "E"),
            "      m".into(),
            format!("warn  {:<22} a.lua:3", "W"),
            "      m".into(),
            String::new(),
            "Scripts, in load order:".into(),
            "  a.lua  (NPC)  [onUpdate]".into(),
        ];
        assert_eq!(text, want.join("\n") + "\n");
    }
}
