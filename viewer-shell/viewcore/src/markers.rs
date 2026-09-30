//! Wraithguard: the Construction Set markers the cell viewer carries itself - R-Zero's
//! Construction Tools and its two marker fixes (R-Zero aka Reizeron, Nexus Mods 47908;
//! "Do whatever you want, really."). See `data/markers/README.md`. The page asks for one
//! as `__wg/<file>`.

const MARKERS: &[(&str, &[u8])] = &[
    ("marker_arrow.nif", include_bytes!("../data/markers/marker_arrow.nif")),
    ("marker_travel.nif", include_bytes!("../data/markers/marker_travel.nif")),
    ("marker_creature.nif", include_bytes!("../data/markers/marker_creature.nif")),
    ("marker_character.nif", include_bytes!("../data/markers/marker_character.nif")),
    ("marker_ruler.nif", include_bytes!("../data/markers/marker_ruler.nif")),
];

/// The prefix a built-in marker is asked for by.
pub const PREFIX: &str = "__wg/";

/// A built-in marker's bytes, by `__wg/<file>` (or the bare file name), any case.
pub fn get(path: &str) -> Option<&'static [u8]> {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let name = p.strip_prefix(PREFIX).unwrap_or(&p);
    MARKERS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_marker_reads() {
        for (n, b) in super::MARKERS {
            let r = crate::nif::read_for_draw(b, None);
            assert!(r.map(|r| !r.parts.is_empty()).unwrap_or(false), "{n} has nothing to draw");
        }
        assert!(super::get("__WG/Marker_Arrow.nif").is_some());
        assert!(super::get("__wg/nothing.nif").is_none());
    }
}

