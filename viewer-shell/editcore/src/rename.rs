//! An id renamed inside script source, for Search & Replace
//! (`wraithguard/mwscript/rename.py`, port step 4.1, slice 5b).
//!
//! The Construction Set's Search & Replace leaves scripts alone: a changed text without a
//! recompile leaves the bytecode Morrowind.exe runs naming the old id. With the compiler
//! the Editor recompiles, so scripts and dialogue results are changed too: every place
//! the id stands as one, bare or quoted, the way the compiler reads one - not inside a
//! longer word, a different quoted id, or a comment.

/// Whether a character is part of an id, as the compiler's character table has it (the
/// "symbol" class: ASCII letters, digits and the underscore).
pub fn is_symbol(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `text` with the id `old` (any case) made `new` wherever it stands as one, and how many
/// places changed. A bare word is replaced when the whole word is `old`; a quoted string
/// when its whole content is (on one line). `new` is quoted where it has to be (a space,
/// a dot, a hyphen...). Comments (`;` to the line's end) are left as written.
pub fn replace_id(text: &str, old: &str, new: &str) -> (String, usize) {
    let low = old.to_lowercase();
    let needs_quotes = new.is_empty() || !new.chars().all(is_symbol);
    let bare_new = if needs_quotes { format!("\"{new}\"") } else { new.to_string() };
    let t: Vec<char> = text.chars().collect();
    let n = t.len();
    let find = |from: usize, c: char| t[from.min(n)..].iter().position(|&x| x == c).map(|p| p + from);
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut i = 0;
    while i < n {
        let ch = t[i];
        if ch == ';' {
            let end = find(i, '\n').unwrap_or(n);
            out.extend(&t[i..end]);
            i = end;
        } else if ch == '"' {
            let end = find(i + 1, '"');
            let line_end = find(i + 1, '\n');
            let Some(end) = end.filter(|&e| line_end.is_none_or(|l| l > e)) else {
                out.push(ch);
                i += 1;
                continue;
            };
            let inner: String = t[i + 1..end].iter().collect();
            if inner.to_lowercase() == low {
                out.push('"');
                out.push_str(new);
                out.push('"');
                count += 1;
            } else {
                out.extend(&t[i..=end]);
            }
            i = end + 1;
        } else if is_symbol(ch) {
            let mut j = i;
            while j < n && is_symbol(t[j]) {
                j += 1;
            }
            let word: String = t[i..j].iter().collect();
            if word.to_lowercase() == low {
                out.push_str(&bare_new);
                count += 1;
            } else {
                out.push_str(&word);
            }
            i = j;
        } else {
            out.push(ch);
            i += 1;
        }
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::replace_id;

    #[test]
    fn words_quotes_and_comments() {
        let text = "Begin s\nplayer->AddItem Gold_001 1 ; gold_001 here stays\nset x to gold_0012\nAddItem \"gold_001\" 2\n";
        let (out, n) = replace_id(text, "gold_001", "ruby");
        assert_eq!(n, 2);
        assert_eq!(out, "Begin s\nplayer->AddItem ruby 1 ; gold_001 here stays\nset x to gold_0012\nAddItem \"ruby\" 2\n");
        assert_eq!(replace_id("additem gold_001 1", "gold_001", "silver bar").0, "additem \"silver bar\" 1");
        assert_eq!(replace_id("say \"unclosed gold_001\nx", "gold_001", "y"), ("say \"unclosed y\nx".to_string(), 1));
    }
}
