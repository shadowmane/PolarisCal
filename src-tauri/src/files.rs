//! Attached files on disk: copying them into the shared attachments folder with a
//! SHA-256 fingerprint, resolving stored relative paths safely, and checking that
//! the folder is usable.
//!
//! Layout under the attachments folder:
//! * `<ASSET-TAG>/<file>` for files attached to an Instrument or System
//! * `<ASSET-TAG>/<YYYY-MM-DD>/<file>` for files attached to a calibration record

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Largest file that can be attached.
pub const MAX_FILE_BYTES: u64 = 250 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    /// Path under the attachments folder, `/`-separated.
    pub relative_path: String,
    /// The original file name, for display.
    pub file_name: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("The attachments folder {0} can't be reached. Check that the share is available.")]
    RootUnavailable(String),
    #[error("{0} is not a file.")]
    NotAFile(String),
    #[error("{name} is too large ({mb} MB). The limit is {limit} MB.", limit = MAX_FILE_BYTES / 1024 / 1024)]
    TooLarge { name: String, mb: u64 },
    #[error("The stored path {0} is not valid.")]
    BadPath(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

fn io_err(context: impl Into<String>) -> impl FnOnce(io::Error) -> FileError {
    let context = context.into();
    move |source| FileError::Io { context, source }
}

/// Makes a name safe as a single Windows path segment.
pub fn sanitize_segment(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    out = out.trim().trim_end_matches(['.', ' ']).to_string();
    if out.chars().count() > 120 {
        out = out
            .chars()
            .take(120)
            .collect::<String>()
            .trim_end()
            .to_string();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if out.is_empty() || out.chars().all(|c| c == '.') {
        out = "_".into();
    } else if RESERVED.contains(&stem.as_str()) {
        out = format!("_{out}");
    }
    out
}

/// Turns a stored relative path into a real path under `root`, refusing anything
/// that could escape it (`..`, absolute paths, drive letters).
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, FileError> {
    let rel = Path::new(relative);
    let mut out = root.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(p) => out.push(p),
            _ => return Err(FileError::BadPath(relative.to_string())),
        }
    }
    if relative.is_empty() || relative.contains(':') {
        return Err(FileError::BadPath(relative.to_string()));
    }
    Ok(out)
}

/// SHA-256 (lowercase hex) and size of a file.
pub fn hash_file(path: &Path) -> io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex(&hasher.finalize()), total))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Opens a new file in `dir` named like `name`, adding " (2)", " (3)", ... if taken.
/// `create_new` makes the check-and-create atomic, so two users can't collide.
fn create_unique(dir: &Path, name: &str) -> Result<(String, PathBuf, File), FileError> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 1..10_000 {
        let candidate = if n == 1 {
            name.to_string()
        } else {
            format!("{stem} ({n}){ext}")
        };
        let path = dir.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((candidate, path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(format!("Couldn't create {}", path.display()))(e)),
        }
    }
    Err(FileError::BadPath(name.to_string()))
}

/// Copies `source` into `root/<folder...>/`, hashing it on the way.
pub fn store_file(root: &Path, folder: &[&str], source: &Path) -> Result<StoredFile, FileError> {
    if !root.is_dir() {
        return Err(FileError::RootUnavailable(root.display().to_string()));
    }
    let display_name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| FileError::NotAFile(source.display().to_string()))?;
    let meta = fs::metadata(source).map_err(io_err(format!("Couldn't read {display_name}")))?;
    if !meta.is_file() {
        return Err(FileError::NotAFile(display_name));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(FileError::TooLarge {
            name: display_name,
            mb: meta.len() / 1024 / 1024,
        });
    }

    let segments: Vec<String> = folder.iter().map(|s| sanitize_segment(s)).collect();
    let dir = segments.iter().fold(root.to_path_buf(), |p, s| p.join(s));
    fs::create_dir_all(&dir).map_err(io_err(format!("Couldn't create {}", dir.display())))?;

    let (stored_name, dest, mut out) = create_unique(&dir, &sanitize_segment(&display_name))?;
    let mut copy = || -> io::Result<(String, u64)> {
        let mut input = File::open(source)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            total += n as u64;
        }
        out.sync_all()?;
        Ok((hex(&hasher.finalize()), total))
    };
    let (sha256, size) = match copy() {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_file(&dest);
            return Err(io_err(format!("Couldn't copy {display_name}"))(e));
        }
    };

    let mut relative: Vec<String> = segments;
    relative.push(stored_name);
    Ok(StoredFile {
        relative_path: relative.join("/"),
        content_type: mime_guess::from_path(&display_name)
            .first()
            .map(|m| m.essence_str().to_string()),
        file_name: display_name,
        size_bytes: i64::try_from(size).unwrap_or(i64::MAX),
        sha256,
    })
}

/// Deletes a file copied by [`store_file`] when its database record couldn't be saved.
pub fn discard(root: &Path, relative: &str) {
    if let Ok(path) = resolve(root, relative) {
        let _ = fs::remove_file(path);
    }
}

/// Confirms the folder exists and this computer can write to it.
pub fn check_writable(root: &Path) -> Result<(), FileError> {
    if !root.is_dir() {
        return Err(FileError::RootUnavailable(root.display().to_string()));
    }
    let probe = root.join(format!(".polariscal-write-test-{}", std::process::id()));
    fs::write(&probe, b"ok").map_err(io_err(format!(
        "PolarisCal can't write to {}",
        root.display()
    )))?;
    let _ = fs::remove_file(probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_windows_names() {
        assert_eq!(sanitize_segment("cal: report?.pdf"), "cal_ report_.pdf");
        assert_eq!(sanitize_segment("  trailing dots... "), "trailing dots");
        assert_eq!(sanitize_segment("CON"), "_CON");
        assert_eq!(sanitize_segment("aux.txt"), "_aux.txt");
        assert_eq!(sanitize_segment(".."), "_");
        assert_eq!(sanitize_segment("DMM/001"), "DMM_001");
        assert_eq!(sanitize_segment(&"x".repeat(300)).len(), 120);
    }

    #[test]
    fn resolve_refuses_escapes() {
        let root = Path::new(r"C:\share");
        assert_eq!(
            resolve(root, "DMM-1/cert.pdf").unwrap(),
            root.join("DMM-1").join("cert.pdf")
        );
        for bad in [
            "../secret.txt",
            "a/../../b",
            r"C:\Windows\x",
            "/etc/passwd",
            "",
            "a:b",
        ] {
            assert!(resolve(root, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn stores_hashes_and_deduplicates_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("share");
        fs::create_dir(&root).unwrap();
        let src = dir.path().join("Cert 2026.pdf");
        fs::write(&src, b"hello").unwrap();

        let first = store_file(&root, &["DMM-1", "2026-03-01"], &src).unwrap();
        assert_eq!(first.relative_path, "DMM-1/2026-03-01/Cert 2026.pdf");
        assert_eq!(first.file_name, "Cert 2026.pdf");
        assert_eq!(first.size_bytes, 5);
        // sha256("hello")
        assert_eq!(
            first.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(first.content_type.as_deref(), Some("application/pdf"));

        let second = store_file(&root, &["DMM-1", "2026-03-01"], &src).unwrap();
        assert_eq!(second.relative_path, "DMM-1/2026-03-01/Cert 2026 (2).pdf");

        let on_disk = resolve(&root, &second.relative_path).unwrap();
        assert_eq!(hash_file(&on_disk).unwrap(), (first.sha256.clone(), 5));

        discard(&root, &second.relative_path);
        assert!(!on_disk.exists());
    }

    #[test]
    fn missing_root_and_non_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"x").unwrap();
        let missing = dir.path().join("not-there");
        assert!(matches!(
            store_file(&missing, &["X"], &src),
            Err(FileError::RootUnavailable(_))
        ));
        assert!(matches!(
            check_writable(&missing),
            Err(FileError::RootUnavailable(_))
        ));
        assert!(matches!(
            store_file(dir.path(), &["X"], dir.path()),
            Err(FileError::NotAFile(_))
        ));
        assert!(check_writable(dir.path()).is_ok());
    }
}
