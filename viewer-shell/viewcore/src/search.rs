//! Wraithguard: records found by part of their id, for the Editor's command palette (Static
//! 2026-11-24: "Command palette (Ctrl+P). Find any action, record, cell or tool by typing").
//!
//! Over `World::record_where` (every id-bearing record of the load order, by lowercased id),
//! each word typed must be in the id. Best first: the id itself, then ids that start with the
//! words, then ids where they start a part (after `_`, ` `, `-` or `.`), then any; shorter ids
//! before longer ones, then by name.

use std::collections::HashMap;

/// The ids matching `q` (words, any case), best first, at most `limit`: `(id, tag)`, the tag the
/// id resolves to (the last entry of its list).
pub fn ids(index: &HashMap<String, Vec<(usize, [u8; 4])>>, q: &str, limit: usize) -> Vec<(String, String)> {
    let words: Vec<String> = q.split_whitespace().map(str::to_ascii_lowercase).collect();
    if words.is_empty() || limit == 0 {
        return Vec::new();
    }
    let whole = words.join(" ");
    let mut hits: Vec<(u8, usize, &String, [u8; 4])> = Vec::new();
    for (id, at) in index {
        if !words.iter().all(|w| id.contains(w.as_str())) {
            continue;
        }
        let Some((_, tag)) = at.last() else { continue };
        let rank = if *id == whole {
            0
        } else if id.starts_with(words[0].as_str()) {
            1
        } else if words.iter().all(|w| starts_part(id, w)) {
            2
        } else {
            3
        };
        hits.push((rank, id.len(), id, *tag));
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
    hits.into_iter()
        .take(limit)
        .map(|(_, _, id, tag)| (id.clone(), String::from_utf8_lossy(&tag).into_owned()))
        .collect()
}

/// Where scripts set a variable (Static 2026-11-24: Script Edit's "find every place this variable
/// is set"): `(script id, line number from 1, the line)` for each `set <name> to` and `set
/// <object>.<name> to` (the object quoted or not), comments left out, any case. `texts` is
/// `World::script_texts` (lowercased id -> (id, text)); scripts by id, lines in order.
pub fn writes<'a, I>(texts: I, name: &str) -> Vec<(String, usize, String)>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let want = name.trim().trim_matches('"').to_ascii_lowercase();
    let mut out = Vec::new();
    if want.is_empty() {
        return out;
    }
    for (id, text) in texts {
        for (n, line) in text.lines().enumerate() {
            let code = strip_comment(line).to_ascii_lowercase();
            let t: Vec<&str> = code.split_whitespace().collect();
            let hit = t.windows(3).any(|w| {
                w[0] == "set" && w[2] == "to" && {
                    let var = w[1].rsplit('.').next().unwrap_or(w[1]).trim_matches('"');
                    var == want
                }
            });
            if hit {
                out.push((id.clone(), n + 1, line.trim().to_string()));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()).then(a.1.cmp(&b.1)));
    out
}

/// A line of MWScript up to its comment (`;` outside quotes).
fn strip_comment(line: &str) -> &str {
    let mut q = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => q = !q,
            ';' if !q => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Whether `w` starts the id or one of its parts.
fn starts_part(id: &str, w: &str) -> bool {
    id.match_indices(w).any(|(i, _)| i == 0 || matches!(id.as_bytes()[i - 1], b'_' | b' ' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_matches_come_first() {
        let mut m: HashMap<String, Vec<(usize, [u8; 4])>> = HashMap::new();
        for (id, tag) in [("fargoth", *b"NPC_"), ("fargoth_ring", *b"CLOT"), ("misc_fargoth_note", *b"BOOK"), ("dagoth_ur", *b"NPC_"), ("ex_fargothhut", *b"STAT")] {
            m.insert(id.into(), vec![(0, tag)]);
        }
        let got: Vec<String> = ids(&m, "Fargoth", 10).into_iter().map(|(i, _)| i).collect();
        assert_eq!(got, ["fargoth", "fargoth_ring", "ex_fargothhut", "misc_fargoth_note"]);
        assert_eq!(ids(&m, "fargoth ring", 10), [("fargoth_ring".to_string(), "CLOT".to_string())]);
        assert_eq!(ids(&m, "goth", 2).len(), 2);
        assert!(ids(&m, "  ", 10).is_empty());
    }

    #[test]
    fn where_a_variable_is_set() {
        let a = ("MyScript".to_string(), "begin MyScript\nshort count\nset count to count + 1 ; set gone to 1\nif ( count > 3 )\n  Set \"fargoth\".Count To 0\nendif\nset counter to 2\nend".to_string());
        let b = ("other".to_string(), "begin other\nset MyScript.count to 5\nmessagebox \"set count to 9; really\"\nend".to_string());
        let texts = vec![(&a.0, &a.1), (&b.0, &b.1)];
        let got = writes(texts, "Count");
        let at: Vec<(String, usize)> = got.iter().map(|(i, n, _)| (i.clone(), *n)).collect();
        assert_eq!(at, [("MyScript".to_string(), 3), ("MyScript".to_string(), 5), ("other".to_string(), 2)]);
        assert!(writes(vec![(&a.0, &a.1)], "gone").is_empty(), "a comment is not code");
    }
}
