//! A path as Python's `str(Path(text))` gives it, since the report prints the paths it was
//! given that way: repeated separators and `.` parts dropped, no trailing separator, and on
//! Windows `/` turned into `\`. `..` is kept, as pathlib keeps it.

/// `str(Path(text))`.
pub fn py_path(text: &str) -> String {
    if cfg!(windows) { windows(text) } else { posix(text) }
}

fn join(prefix: &str, parts: &[&str], sep: &str) -> String {
    let body = parts.join(sep);
    if prefix.is_empty() && body.is_empty() { ".".to_string() } else { format!("{prefix}{body}") }
}

fn posix(text: &str) -> String {
    // POSIX keeps exactly two leading slashes; one, or three and more, become one.
    let prefix = if text.starts_with("//") && !text.starts_with("///") {
        "//"
    } else if text.starts_with('/') {
        "/"
    } else {
        ""
    };
    let parts: Vec<&str> = text.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    join(prefix, &parts, "/")
}

fn windows(text: &str) -> String {
    let t = text.replace('/', "\\");
    let (drive, rest) = if t.len() >= 2 && t.as_bytes()[1] == b':' && t.as_bytes()[0].is_ascii_alphabetic() {
        (t[..2].to_string(), &t[2..])
    } else if let Some(unc) = t.strip_prefix("\\\\") {
        // \\server\share is the drive of a UNC path
        let mut it = unc.splitn(3, '\\');
        let (server, share) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
        let tail = it.next().unwrap_or("");
        return join(&format!("\\\\{server}\\{share}\\"), &tail.split('\\').filter(|p| !p.is_empty() && *p != ".").collect::<Vec<_>>(), "\\");
    } else {
        (String::new(), t.as_str())
    };
    let root = if rest.starts_with('\\') { "\\" } else { "" };
    let parts: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty() && *p != ".").collect();
    join(&format!("{drive}{root}"), &parts, "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_pathlib_prints_them() {
        assert_eq!(posix("a//b/./c/"), "a/b/c");
        assert_eq!(posix("//x/y"), "//x/y");
        assert_eq!(posix("///x"), "/x");
        assert_eq!(posix(""), ".");
        assert_eq!(posix("./"), ".");
        assert_eq!(posix("../a"), "../a");
        assert_eq!(windows("C:/Games//Morrowind/"), "C:\\Games\\Morrowind");
        assert_eq!(windows("C:"), "C:");
        assert_eq!(windows("C:/"), "C:\\");
        assert_eq!(windows("\\\\srv\\share\\x\\"), "\\\\srv\\share\\x");
        assert_eq!(windows("rel/./x"), "rel\\x");
    }
}
