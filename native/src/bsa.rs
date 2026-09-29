//! Morrowind `.bsa` archives, on `tes3::bsa`.
//!
//! The index is parsed by greatness7's reader (`Archive`, which validates the
//! header, tables and hashes); from it we keep each file's normalised name,
//! offset and size, and close the file again. Contents are read on demand with a
//! plain seek, so an open archive never holds the `.bsa` open or mapped - a mod
//! manager can still replace it while Wraithguard runs.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Lower-cased, forward slashes, no leading separator or surrounding spaces:
/// how the toolkit compares an archived path with a referenced one.
pub fn normalise(name: &str) -> String {
    name.trim().replace('\\', "/").trim_start_matches('/').to_lowercase()
}

/// Windows-1252 to text; the five unassigned bytes become U+FFFD, as Python's
/// `decode("cp1252", errors="replace")` does.
pub fn decode_cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
        '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
        '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct Span {
    offset: u64,
    size: u64,
}

/// The index of one archive: names in archive order, and where each file is.
#[derive(Debug)]
pub struct Index {
    names: Vec<String>,
    spans: HashMap<String, Span>,
}

impl Index {
    /// Reads and validates an archive's index.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        // Mapped only while the index is read; dropped (and the file closed) on return.
        // SAFETY: read-only map of a file we only read, released before returning.
        let map = unsafe { memmap2::Mmap::map(&file)? };
        let base = map.as_ptr() as usize;
        let archive = crate::guarded(|| tes3::bsa::Archive::from_slice(&map))?;
        let mut names = Vec::with_capacity(archive.len());
        let mut spans = HashMap::with_capacity(archive.len());
        for entry in archive.entries() {
            let Some(raw) = entry.name() else { continue };
            let name = normalise(&decode_cp1252(raw));
            if name.is_empty() {
                continue;
            }
            let data = entry.as_bytes();
            let span = Span { offset: (data.as_ptr() as usize - base) as u64, size: data.len() as u64 };
            if spans.insert(name.clone(), span).is_none() {
                names.push(name);
            }
        }
        Ok(Self { names, spans })
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.spans.contains_key(&normalise(name))
    }

    /// One file's bytes, or None when the archive does not hold it.
    pub fn read(&self, path: &Path, name: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(span) = self.spans.get(&normalise(name)) else { return Ok(None) };
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(span.offset))?;
        let mut data = vec![0u8; span.size as usize];
        file.read_exact(&mut data).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(io::ErrorKind::InvalidData, format!("{name} runs past the end of the archive"))
            } else {
                e
            }
        })?;
        Ok(Some(data))
    }
}

/// An error for Python: bad data is ValueError, anything else OSError.
fn py_err(path: &Path, e: io::Error) -> PyErr {
    let msg = format!("{}: {}", path.display(), e);
    if e.kind() == io::ErrorKind::InvalidData { PyValueError::new_err(msg) } else { PyOSError::new_err(msg) }
}

/// A Morrowind archive's index; `read` fetches one file.
#[pyclass(frozen, module = "wraithguard_native")]
pub struct Archive {
    path: PathBuf,
    index: Index,
}

#[pymethods]
impl Archive {
    /// Opens `path` and reads its index. Raises ValueError for something that is
    /// not a Morrowind archive, OSError when it cannot be read.
    #[new]
    fn new(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        let index = py.detach(|| Index::open(&path)).map_err(|e| py_err(&path, e))?;
        Ok(Self { path, index })
    }

    fn __len__(&self) -> usize {
        self.index.len()
    }

    fn __contains__(&self, name: &str) -> bool {
        self.index.contains(name)
    }

    /// Every stored path, normalised, in archive order.
    #[getter]
    fn names(&self) -> Vec<String> {
        self.index.names.clone()
    }

    /// One file's bytes (any case, either separator), or None.
    fn read<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let data = py.detach(|| self.index.read(&self.path, name)).map_err(|e| py_err(&self.path, e))?;
        Ok(data.map(|d| PyBytes::new(py, &d)))
    }
}

#[pyfunction(name = "bsa_normalise")]
fn py_normalise(name: &str) -> String {
    normalise(name)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Archive>()?;
    m.add_function(wrap_pyfunction!(py_normalise, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tes3::bsa::Builder::new();
        for (n, d) in files {
            b.insert(n.to_string(), d.to_vec()).unwrap();
        }
        b.save_bytes().unwrap()
    }

    #[test]
    fn reads_what_the_builder_wrote() {
        let dir = std::env::temp_dir().join(format!("wgbsa{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.bsa");
        std::fs::write(&p, build(&[("textures\\A.dds", b"AAA"), ("meshes\\x\\b.nif", b"BB")])).unwrap();
        let ix = Index::open(&p).unwrap();
        assert_eq!(ix.len(), 2);
        assert!(ix.contains("Textures/a.DDS") && ix.contains("\\meshes\\x\\b.nif"));
        assert_eq!(ix.read(&p, "textures/a.dds").unwrap().unwrap(), b"AAA");
        assert_eq!(ix.read(&p, "meshes/x/b.nif").unwrap().unwrap(), b"BB");
        assert!(ix.read(&p, "nope").unwrap().is_none());
        // Closed again: the file can be replaced while the index lives on.
        std::fs::write(&p, b"gone").unwrap();
        assert!(ix.read(&p, "textures/a.dds").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn junk_is_invalid_data() {
        let dir = std::env::temp_dir().join(format!("wgbsaj{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("j.bsa");
        std::fs::write(&p, b"BSA\0\0\0\0\0\0\0\0\0").unwrap();
        assert_eq!(Index::open(&p).unwrap_err().kind(), io::ErrorKind::InvalidData);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cp1252_and_normalise() {
        assert_eq!(decode_cp1252(b"a\x80\x81"), "a\u{20AC}\u{FFFD}");
        assert_eq!(normalise(" \\Meshes\\X.NIF "), "meshes/x.nif");
    }
}
