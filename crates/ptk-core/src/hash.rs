//! SHA-256 hashing of files and folders.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::manifest::SourceFile;

/// Lower-case hex SHA-256 of a byte slice.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Lower-case hex SHA-256 of everything a reader yields.
pub fn sha256_reader(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Lower-case hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    sha256_reader(File::open(path)?)
}

/// True for a 64-character lower-case hex string.
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Operating-system clutter that never belongs to a game or a project:
/// Finder metadata, AppleDouble `._*` files, Windows thumbnails.
pub fn is_junk_file(name: &str) -> bool {
    name == ".DS_Store" || name.starts_with("._") || name == "Thumbs.db" || name == "desktop.ini"
}

/// Path of `path` relative to `root`, with `/` separators.
pub fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for comp in rel.components() {
        match comp {
            std::path::Component::Normal(p) => parts.push(p.to_str()?.to_owned()),
            _ => return None,
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Hashes every regular file under `root` (symbolic links and OS clutter
/// are skipped), sorted by path. Used to record a game's source files.
/// A single file is recorded under its own name.
pub fn hash_tree(root: &Path) -> io::Result<Vec<SourceFile>> {
    if root.is_file() {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok(vec![SourceFile {
            path: name,
            size: std::fs::metadata(root)?.len(),
            sha256: sha256_file(root)?,
        }]);
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_str().is_some_and(is_junk_file) {
            continue;
        }
        let Some(path) = relative_path(root, entry.path()) else {
            continue;
        };
        let size = entry.metadata().map_err(io::Error::other)?.len();
        let sha256 = sha256_file(entry.path())?;
        out.push(SourceFile { path, size, sha256 });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digest() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_sha256_hex(&sha256_bytes(b"")));
        assert!(!is_sha256_hex("ABC"));
    }

    #[test]
    fn tree_skips_junk_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::write(dir.path().join("b/z.bin"), b"z").unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("._a.txt"), b"junk").unwrap();
        std::fs::write(dir.path().join(".DS_Store"), b"junk").unwrap();
        let files = hash_tree(dir.path()).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "b/z.bin"]);
        assert_eq!(files[0].size, 1);
        assert_eq!(files[0].sha256, sha256_bytes(b"a"));
    }
}
