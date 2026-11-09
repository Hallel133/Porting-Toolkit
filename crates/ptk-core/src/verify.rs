//! `ptk verify`: checks a user's own game files against the SHA-256 hashes
//! recorded in a manifest, so a community project is applied only to the
//! exact game version it was made for.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::hash::sha256_file;
use crate::manifest::{Manifest, is_safe_relative_path};

/// A file whose size or hash differs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Mismatch {
    pub path: String,
    pub expected_sha256: String,
    /// `None` when the size already differs, so the file was not hashed.
    pub actual_sha256: Option<String>,
    pub expected_size: u64,
    pub actual_size: u64,
}

/// Result of [`verify_game`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
    pub checked: usize,
    pub ok: usize,
    pub mismatched: Vec<Mismatch>,
    pub missing: Vec<String>,
    /// Manifest entries with unsafe paths, never read.
    pub skipped: Vec<String>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.checked > 0
            && self.mismatched.is_empty()
            && self.missing.is_empty()
            && self.skipped.is_empty()
    }
}

/// Compares the files under `game_dir` with `manifest.source_files`.
/// `progress(done, total)` is called after each file.
pub fn verify_game(
    game_dir: &Path,
    manifest: &Manifest,
    mut progress: impl FnMut(usize, usize),
) -> io::Result<VerifyReport> {
    // A single executable is checked in the folder that holds it.
    let game_dir = if game_dir.is_file() {
        game_dir.parent().unwrap_or(game_dir)
    } else {
        game_dir
    };
    if !game_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a folder", game_dir.display()),
        ));
    }
    let total = manifest.source_files.len();
    let mut report = VerifyReport::default();
    for (i, f) in manifest.source_files.iter().enumerate() {
        report.checked += 1;
        if !is_safe_relative_path(&f.path) {
            report.skipped.push(f.path.clone());
            progress(i + 1, total);
            continue;
        }
        let path = game_dir.join(&f.path);
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => m,
            Ok(_) => {
                report.missing.push(f.path.clone());
                progress(i + 1, total);
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                report.missing.push(f.path.clone());
                progress(i + 1, total);
                continue;
            }
            Err(e) => return Err(e),
        };
        let actual_size = meta.len();
        let actual_sha256 = if actual_size == f.size {
            Some(sha256_file(&path)?)
        } else {
            None
        };
        if actual_sha256.as_deref() == Some(f.sha256.as_str()) {
            report.ok += 1;
        } else {
            report.mismatched.push(Mismatch {
                path: f.path.clone(),
                expected_sha256: f.sha256.clone(),
                actual_sha256,
                expected_size: f.size,
                actual_size,
            });
        }
        progress(i + 1, total);
    }
    Ok(report)
}
