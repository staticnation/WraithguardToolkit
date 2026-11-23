//! The one place a name read from an archive or a plugin becomes a path under a folder the user
//! chose (security audit 2026-11-20, F1). A BSA entry is whatever its maker typed: `meshes\..\..\x.nif`
//! passes a `meshes/` prefix and a `.nif` suffix and `Path::join` keeps the `..`, so the write lands
//! outside the output folder. Everything that writes under a name it did not make goes through
//! [`safe_rel`] or [`join_inside`].

use std::path::{Component, Path, PathBuf};

/// `name` as a relative path made of plain parts only, `/` and `\` both splitting it, or `None` if
/// it could leave the folder it is joined to or means something else on a platform: a `..` part, an
/// absolute or drive-prefixed name, a `:` (a Windows drive, or an alternate data stream), a control
/// character or one Windows will not take in a file name, a part ending in a dot or a space (which
/// Windows drops, so `..` can hide as `.. `), or nothing at all.
pub fn safe_rel(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.ends_with('.') || part.ends_with(' ') {
            return None;
        }
        if part.chars().any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*')) {
            return None;
        }
        out.push(part);
    }
    // Only plain parts: no root, prefix, `.` or `..` can have got in.
    (out.components().count() > 0 && out.components().all(|c| matches!(c, Component::Normal(_)))).then_some(out)
}

/// `root` joined with `name`, refused (with why) unless [`safe_rel`] takes the name and the result
/// is still inside `root`.
///
/// # Errors
/// The name is not a safe relative path, or would end outside `root`.
pub fn join_inside(root: &Path, name: &str) -> Result<PathBuf, String> {
    let rel = safe_rel(name).ok_or_else(|| format!("{name:?} is not a safe relative path (it has a .., a drive, an absolute part or a character a file name cannot have), so it was not written"))?;
    let dest = root.join(rel);
    if !dest.starts_with(root) {
        return Err(format!("{name:?} would be written outside {}, so it was not written", root.display()));
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_pass_and_the_rest_do_not() {
        assert_eq!(safe_rel("meshes/a\\b/c.nif").unwrap(), PathBuf::from("meshes").join("a").join("b").join("c.nif"));
        assert_eq!(safe_rel("./meshes//x.nif").unwrap(), PathBuf::from("meshes").join("x.nif"));
        for bad in ["", ".", "meshes/../x.nif", "meshes\\..\\..\\x.nif", "../x.nif", "..", "meshes/.. /x.nif", "meshes/x.nif.", "C:\\x.nif", "c:x.nif", "meshes/x.nif:stream", "meshes/a\u{0}b.nif", "meshes/a*.nif", "meshes/CON?.nif"] {
            assert!(safe_rel(bad).is_none(), "{bad:?}");
        }
        // A leading slash is only an empty part: the name stays under the folder it is joined to.
        assert_eq!(safe_rel("/etc/x.nif").unwrap(), PathBuf::from("etc").join("x.nif"));
    }

    #[test]
    fn a_join_stays_inside_or_is_refused() {
        let root = Path::new("out");
        assert_eq!(join_inside(root, "meshes/x.nif").unwrap(), root.join("meshes").join("x.nif"));
        assert!(join_inside(root, "meshes/../../escape.nif").unwrap_err().contains("not a safe relative path"));
    }
}
