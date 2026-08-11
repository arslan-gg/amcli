//! Opening and saving whatever a `.archimate` path actually turns out to be.
//!
//! Archi writes a plain XML file most of the time, but as soon as a model
//! embeds an image it silently switches to a ZIP holding `model.xml` plus the
//! image blobs. Third-party tools that assume XML break on exactly those files,
//! so the sniff happens once, here, and the rest of the crate never thinks about
//! it again.

use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::ModelError;

/// The shape of the file on disk, and everything needed to write it back.
#[derive(Debug)]
pub enum Container {
    /// A plain XML document.
    Plain,
    /// A ZIP archive whose `model.xml` entry is the document. The other entries
    /// are carried through untouched.
    Zip {
        /// Raw archive bytes, so non-model entries can be copied verbatim
        /// rather than recompressed — recompressing would change entry order,
        /// compression levels and timestamps, and break byte identity.
        original: Vec<u8>,
        /// Name of the entry holding the model, normally `model.xml`.
        entry: String,
    },
}

/// The conventional entry name; `IArchiveManager.isArchiveFile` looks for it.
const MODEL_ENTRY: &str = "model.xml";

impl Container {
    /// Read a file and hand back its XML bytes plus how to put them back.
    pub fn open(path: &Path) -> Result<(Container, Vec<u8>), ModelError> {
        let bytes = std::fs::read(path)
            .map_err(|e| ModelError::Io { path: path.to_path_buf(), source: e })?;
        Self::from_bytes(bytes, path)
    }

    pub fn from_bytes(bytes: Vec<u8>, path: &Path) -> Result<(Container, Vec<u8>), ModelError> {
        if !bytes.starts_with(b"PK") {
            return Ok((Container::Plain, bytes));
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(&bytes[..])).map_err(|e| {
            ModelError::Archive { path: path.to_path_buf(), message: e.to_string() }
        })?;

        let entry = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
            .find(|n| n == MODEL_ENTRY)
            .ok_or_else(|| ModelError::Archive {
                path: path.to_path_buf(),
                message: format!("archive has no `{MODEL_ENTRY}` entry"),
            })?;

        let mut xml = Vec::new();
        zip.by_name(&entry)
            .map_err(|e| ModelError::Archive { path: path.to_path_buf(), message: e.to_string() })?
            .read_to_end(&mut xml)
            .map_err(|e| ModelError::Io { path: path.to_path_buf(), source: e })?;

        Ok((Container::Zip { original: bytes, entry }, xml))
    }

    /// Wrap freshly written XML back into whatever shape the file had.
    pub fn wrap(&self, xml: Vec<u8>) -> Result<Vec<u8>, ModelError> {
        match self {
            Container::Plain => Ok(xml),
            Container::Zip { original, entry } => rezip(original, entry, xml),
        }
    }

    /// The bytes exactly as read. Returning these when nothing changed is what
    /// makes an untouched zipped model round-trip byte for byte — recompressing
    /// `model.xml` would otherwise alter the archive even though the model did
    /// not.
    pub fn original(&self) -> Option<&[u8]> {
        match self {
            Container::Plain => None,
            Container::Zip { original, .. } => Some(original),
        }
    }

    pub fn is_zip(&self) -> bool {
        matches!(self, Container::Zip { .. })
    }
}

/// Rebuild the archive, copying every entry except the model verbatim.
fn rezip(original: &[u8], entry: &str, xml: Vec<u8>) -> Result<Vec<u8>, ModelError> {
    let mut src = zip::ZipArchive::new(Cursor::new(original))
        .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));

    for i in 0..src.len() {
        let file = src
            .by_index_raw(i)
            .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
        if file.name() == entry {
            continue;
        }
        // Raw copy: the compressed bytes, method and timestamps all survive.
        out.raw_copy_file(file)
            .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
    }

    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    out.start_file(entry, opts)
        .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
    out.write_all(&xml).map_err(|e| ModelError::Io { path: PathBuf::new(), source: e })?;

    let cursor = out
        .finish()
        .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
    Ok(cursor.into_inner())
}

/// Write bytes so that a crash can never leave a truncated model behind:
/// temp file in the same directory, flushed to the platter, then renamed over
/// the target, then the directory itself flushed.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ModelError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.amcli.tmp.{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("model"),
        std::process::id()
    ));

    let io = |e: std::io::Error, p: &Path| ModelError::Io { path: p.to_path_buf(), source: e };

    let mut f = std::fs::File::create(&tmp).map_err(|e| io(e, &tmp))?;
    f.write_all(bytes).map_err(|e| io(e, &tmp))?;
    full_sync(&f).map_err(|e| io(e, &tmp))?;
    drop(f);

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e, path));
    }

    // Without this the rename itself can be lost on power failure.
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// On macOS a plain `fsync` returns once the data reaches the drive's cache, not
/// the platter. `F_FULLFSYNC` is the one that actually flushes.
#[cfg(target_os = "macos")]
fn full_sync(f: &std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: fcntl with F_FULLFSYNC takes no pointer arguments; the fd is valid
    // for the lifetime of the borrow.
    let rc = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) };
    if rc == -1 {
        // Not every filesystem supports it (network mounts, some containers);
        // a normal sync is still better than nothing.
        return f.sync_all();
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn full_sync(f: &std::fs::File) -> std::io::Result<()> {
    f.sync_all()
}

/// Read the entries of a zipped model, for tooling that wants to inspect the
/// embedded images without unpacking the file by hand.
pub fn zip_entries(bytes: &[u8]) -> Result<Vec<(String, u64)>, ModelError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| ModelError::Archive { path: PathBuf::new(), message: e.to_string() })?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            out.push((f.name().to_string(), f.size()));
        }
    }
    Ok(out)
}

/// Seek-and-read bound used by the zip crate; kept here so the trait imports do
/// not leak into the rest of the crate.
trait _ReadSeek: Read + Seek {}
