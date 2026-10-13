//! Reading every mesh in an install and reporting what is in them.
//!
//! Round 18bh. Robin asked for this generic rather than a switch for the one record we
//! happened to be chasing: "I think having a more generic scanner might be good in that
//! case, rather than one for this specific purpose only." So the census answers "does
//! anything here use X" without anybody having to ask about X in particular, and the
//! problem list is the half that matters most — every mesh that would not read, that the
//! reader had to guess its way through, or that read but came back with nothing in it.
//! That last column is what would have found §G2 with nobody suspecting triangle strips.
//!
//! The work lives here rather than in the CLI so it can be tested against bytes rather
//! than against somebody's disk.

use std::collections::{BTreeMap, BTreeSet};

/// What one sweep of an install's meshes found.
#[derive(Debug, Default, Clone)]
pub struct Census {
    /// Record type -> how many *meshes* carry at least one of it.
    pub records: BTreeMap<String, usize>,
    /// How many meshes the reader understood.
    pub read: usize,
    /// How many meshes were offered to it.
    pub found: usize,
    /// The meshes matching `--mesh-has`, in the order they were offered.
    pub carrying: Vec<String>,
    /// The meshes the typed model does not read to the end. Round 18gh: there used to be
    /// a column beside this for meshes whose records were found by scanning forward; the
    /// model measures every block or refuses the file, so a mesh is read or it is here.
    pub unreadable: Vec<String>,
    /// The meshes that read but hold no geometry.
    pub empty: Vec<String>,
}

impl Census {
    /// The census rows, commonest first, ties broken by name so the output is stable.
    pub fn rows(&self) -> Vec<(&str, usize)> {
        let mut rows: Vec<(&str, usize)> =
            self.records.iter().map(|(t, n)| (t.as_str(), *n)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        rows
    }

    /// Whether anything at all went wrong.
    pub fn problems(&self) -> bool {
        !self.unreadable.is_empty() || !self.empty.is_empty()
    }
}

/// Reads one mesh into a census that is already under way.
///
/// `want` is the lower-cased record names asked for; empty means nobody asked.
pub fn one(c: &mut Census, name: &str, bytes: &[u8], want: &[String]) {
    c.found += 1;
    let Some(types) = crate::nif::record_types(bytes) else {
        c.unreadable.push(name.to_string());
        return;
    };
    c.read += 1;
    /* Counted once per mesh, not once per record: "how many meshes carry one" is the
       question somebody asks, and a file with two hundred NiTriShapes would otherwise
       drown every other row. */
    let seen: BTreeSet<&str> = types.iter().map(|t| t.as_str()).collect();
    for t in &seen {
        *c.records.entry((*t).to_string()).or_insert(0) += 1;
    }
    if !want.is_empty() && want.iter().any(|w| seen.iter().any(|t| t.to_lowercase() == *w)) {
        c.carrying.push(name.to_string());
    }
    // Read, but nothing came out of it: no geometry means it blocks nothing and grows
    // nothing, which is exactly how a mesh goes missing in silence.
    if crate::nif::parse(bytes).map(|g| g.tris.is_empty()).unwrap_or(true) {
        c.empty.push(name.to_string());
    }
}

/// Lower-cases the wanted record names once, so the sweep does not do it per mesh.
pub fn wanted(names: &[String]) -> Vec<String> {
    names.iter().map(|s| s.to_lowercase()).collect()
}

/// The census as the tool's own TOML — comments and all.
///
/// Robin, on why not JSON: "I think having a --toml might work as well if so, and allow for
/// automated commenting that helps a human read it too." Which is the house style already —
/// `cfgtoml::write_rules` opens with a header and annotates its fields — so a report that
/// did not explain itself would be the odd one out.
pub fn census_toml(c: &Census) -> String {
    let mut o = String::new();
    o.push_str("# Gardenfell: every NIF record type in this install's overlay.\n");
    o.push_str("# The number beside each is how many *meshes* carry at least one of it —\n");
    o.push_str("# not how many records there are, because one mesh with two hundred shapes\n");
    o.push_str("# would otherwise drown every other row.\n");
    o.push_str("# Read with the same reader the scatter uses, so a type missing from this\n");
    o.push_str("# list is a type nothing in your load order uses.\n\n");
    let _ = std::fmt::Write::write_fmt(
        &mut o,
        format_args!(
            "[meshes]\nread  = {}    # meshes the reader understood\nfound = {}    # .nif files in the overlay\n\n[records]\n",
            c.read, c.found
        ),
    );
    for (t, n) in c.rows() {
        let _ = std::fmt::Write::write_fmt(
            &mut o,
            format_args!("{:<28} = {}\n", format!("\"{t}\""), n),
        );
    }
    o
}
