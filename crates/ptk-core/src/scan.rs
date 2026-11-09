//! Scanning a game folder (through the engine) and recording it into a project.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::EngineClient;
use crate::error::{Error, Result, io_ctx};
use crate::hash::hash_tree;
use crate::manifest::Manifest;

/// What the engine's `scan` returns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScanResult {
    pub path: String,
    #[serde(default)]
    pub name: String,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    #[serde(default)]
    pub symlinks_skipped: u64,
    #[serde(default)]
    pub unreadable: u64,
    #[serde(default)]
    pub truncated: bool,
    pub engines: Vec<Detected>,
    pub primary: Option<Detected>,
    pub platforms: Vec<String>,
    pub categories: Vec<CategoryStat>,
    pub largest: Vec<LargeFile>,
}

/// A detected engine or format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detected {
    pub id: String,
    pub name: String,
    /// `engine` or `format`.
    pub kind: String,
    pub confidence: f64,
    /// `high`, `medium` or `low`.
    pub label: String,
    pub variant: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryStat {
    pub id: String,
    pub files: u64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
    pub category: String,
}

impl ScanResult {
    /// Categories, largest first.
    pub fn categories_by_size(&self) -> Vec<&CategoryStat> {
        let mut cats: Vec<&CategoryStat> = self.categories.iter().collect();
        cats.sort_by(|a, b| b.size.cmp(&a.size).then(b.files.cmp(&a.files)));
        cats
    }

    /// Share of a category: by size, or by file count when every file is empty.
    pub fn share(&self, c: &CategoryStat) -> f64 {
        if self.total_size > 0 {
            c.size as f64 / self.total_size as f64
        } else {
            c.files as f64 / self.file_count.max(1) as f64
        }
    }
}

/// Scans a game folder. `progress(files)` receives the engine's progress.
/// Returns the typed result and the raw JSON (for `--json`).
pub fn scan(
    engine: &mut EngineClient,
    game_dir: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<(ScanResult, Value)> {
    let abs = game_dir
        .canonicalize()
        .map_err(io_ctx(format!("{} does not exist", game_dir.display())))?;
    let raw = engine.call_with(
        "scan",
        json!({ "path": abs.to_string_lossy() }),
        &mut |method, params: &Value| {
            if method == "progress"
                && let Some(n) = params["files"].as_u64()
            {
                progress(n);
            }
        },
    )?;
    let result = serde_json::from_value(raw.clone())
        .map_err(|e| Error::Invalid(format!("unexpected scan result: {e}")))?;
    Ok((result, raw))
}

/// What [`record`] wrote.
#[derive(Clone, Debug)]
pub struct RecordReport {
    pub manifest_path: PathBuf,
    pub files: usize,
    pub engine: Option<String>,
}

/// Records a scanned game into a project: engine, title (if unset), source
/// platform (if there is exactly one), and SHA-256 hashes of every file.
pub fn record(project: &Path, game_dir: &Path, scan: &ScanResult) -> Result<RecordReport> {
    let mut m = Manifest::load(project)?;
    let files = hash_tree(game_dir).map_err(io_ctx(format!("hashing {}", game_dir.display())))?;
    if let Some(p) = scan.primary.as_ref().filter(|p| p.kind == "engine") {
        m.game.engine = Some(p.id.clone());
    }
    if m.game.title.is_none() {
        m.game.title = game_dir
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    }
    if let [platform] = scan.platforms.as_slice() {
        m.game.source_platform = Some(platform.clone());
    }
    let count = files.len();
    m.source_files = files;
    let manifest_path = m.save(project)?;
    Ok(RecordReport {
        manifest_path,
        files: count,
        engine: m.game.engine,
    })
}
