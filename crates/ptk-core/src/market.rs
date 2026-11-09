//! The community marketplace and publishing.
//!
//! The marketplace is a plain JSON index in a GitHub repository
//! (`Hallel133/ptk-marketplace`, file `index.json`). Each entry points at a
//! zip (a plugin or an SDK) or a `.ptk` project, with its SHA-256. Publishing
//! means: package your folder (`ptk publish`), upload the zip (for example
//! to a GitHub release of your own repository) and open a pull request that
//! adds the printed entry to `index.json`. Entries merged after review are
//! marked `reviewed`.
//!
//! Downloads always verify the SHA-256, and installing still shows the
//! declared permissions and asks first.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::engine::EngineClient;
use crate::error::{Error, Result, io_ctx};
use crate::hash::{is_junk_file, is_sha256_hex, relative_path, sha256_file};
use crate::manifest::valid_project_name;
use crate::pack::read_pack_manifest;
use crate::paths;
use crate::plugins;
use crate::sdk::{self, SDK_FILE};
use crate::targets::valid_target_id;

/// The public marketplace repository.
pub const MARKETPLACE_REPO: &str = "https://github.com/Hallel133/ptk-marketplace";
/// Where the index is read from (`PTK_MARKETPLACE_URL` overrides it).
pub const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/Hallel133/ptk-marketplace/main/index.json";
/// Placeholder `download` value printed by [`package`].
pub const DOWNLOAD_PLACEHOLDER: &str = "REPLACE-with-the-URL-of-the-uploaded-file";

pub fn index_url() -> String {
    std::env::var("PTK_MARKETPLACE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_INDEX_URL.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Plugin,
    Sdk,
    Project,
}

impl EntryKind {
    pub fn label(self) -> &'static str {
        match self {
            EntryKind::Plugin => "Plugin",
            EntryKind::Sdk => "SDK",
            EntryKind::Project => "Project",
        }
    }
}

/// One marketplace listing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketEntry {
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub download: String,
    pub sha256: String,
    /// Set by marketplace maintainers when they merge a reviewed entry.
    #[serde(default)]
    pub reviewed: bool,
    /// Engines an import plugin reads, or a project ports.
    #[serde(default)]
    pub engines: Vec<String>,
    /// Targets an export plugin or SDK builds for.
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MarketIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub entries: Vec<MarketEntry>,
}

impl MarketEntry {
    /// Problems that make an entry unusable.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let id_ok = match self.kind {
            EntryKind::Project => valid_project_name(&self.id),
            _ => valid_target_id(&self.id),
        };
        if !id_ok {
            p.push(format!("invalid id {:?}", self.id));
        }
        if !is_sha256_hex(&self.sha256) {
            p.push(format!("{}: bad sha256", self.id));
        }
        let url_ok = ["https://", "file://"]
            .iter()
            .any(|s| self.download.starts_with(s));
        if !url_ok {
            p.push(format!("{}: download must be an https:// URL", self.id));
        }
        p
    }

    /// Searchable text.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || [&self.id, &self.name, &self.description, &self.author]
                .iter()
                .any(|s| s.to_lowercase().contains(&q))
            || self
                .engines
                .iter()
                .chain(&self.targets)
                .any(|s| s.contains(&q))
    }
}

/// Parses an index, dropping (and reporting) broken entries.
pub fn parse_index(bytes: &[u8]) -> Result<(MarketIndex, Vec<String>)> {
    let mut index: MarketIndex = serde_json::from_slice(bytes)
        .map_err(|e| Error::Invalid(format!("marketplace index is not valid: {e}")))?;
    if index.schema_version != 1 {
        return Err(Error::Invalid(format!(
            "marketplace index schema {} is not supported; update Porting Toolkit",
            index.schema_version
        )));
    }
    let mut problems = Vec::new();
    index.entries.retain(|e| {
        let p = e.problems();
        problems.extend(p.iter().cloned());
        p.is_empty()
    });
    Ok((index, problems))
}

/// Downloads a URL to a file with the system `curl` (https and file URLs).
fn curl(url: &str, out: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fsSL", "--max-time", "120", "-o"])
        .arg(out)
        .arg(url)
        .output()
        .map_err(io_ctx("could not run curl"))?;
    if status.status.success() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "download failed: {url}: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        )))
    }
}

/// Reads the marketplace index.
pub fn fetch_index(url: &str) -> Result<(MarketIndex, Vec<String>)> {
    let dir = paths::data_dir().join("downloads");
    fs::create_dir_all(&dir).map_err(io_ctx(dir.display().to_string()))?;
    let tmp = dir.join(format!(".index-{}.json", std::process::id()));
    let result = curl(url, &tmp).and_then(|_| {
        let bytes = fs::read(&tmp).map_err(io_ctx(tmp.display().to_string()))?;
        parse_index(&bytes)
    });
    let _ = fs::remove_file(&tmp);
    result
}

/// Downloads an entry, checks its SHA-256 and unpacks zips. Returns the
/// folder (plugin or SDK) or the `.ptk` file.
pub fn download(entry: &MarketEntry) -> Result<PathBuf> {
    if let Some(p) = entry.problems().first() {
        return Err(Error::Invalid(p.clone()));
    }
    let dir = paths::data_dir().join("downloads");
    fs::create_dir_all(&dir).map_err(io_ctx(dir.display().to_string()))?;
    let ext = if entry.kind == EntryKind::Project {
        "ptk"
    } else {
        "zip"
    };
    let file = dir.join(format!("{}-{}.{ext}", entry.id, entry.version));
    curl(&entry.download, &file)?;
    let actual = sha256_file(&file).map_err(io_ctx(file.display().to_string()))?;
    if actual != entry.sha256 {
        let _ = fs::remove_file(&file);
        return Err(Error::Invalid(format!(
            "{}: the download does not match its SHA-256; not installing it",
            entry.id
        )));
    }
    if entry.kind == EntryKind::Project {
        return Ok(file);
    }
    let out = dir.join(format!("{}-{}", entry.id, entry.version));
    let _ = fs::remove_dir_all(&out);
    crate::import::extract_zip(&file, &out)?;
    // A zip of one folder unpacks to that folder.
    let children: Vec<PathBuf> = fs::read_dir(&out)
        .map_err(io_ctx(out.display().to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    Ok(match children.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => out,
    })
}

/// Zips a folder (sorted, fixed timestamps; skips `.git`, caches and OS
/// clutter). Entries are prefixed with the folder name.
pub fn zip_dir(src: &Path, out: &Path, prefix: &str) -> Result<()> {
    let file = File::create(out).map_err(io_ctx(out.display().to_string()))?;
    let mut zip = ZipWriter::new(io::BufWriter::new(file));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let walker = WalkDir::new(src)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && matches!(
                    n.as_ref(),
                    ".git" | "__pycache__" | ".venv" | ".pytest_cache"
                ))
        });
    let zerr = |e: zip::result::ZipError| Error::Invalid(format!("zip: {e}"));
    for entry in walker {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        if !entry.file_type().is_file() || is_junk_file(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Some(rel) = relative_path(src, entry.path()) else {
            continue;
        };
        zip.start_file(format!("{prefix}/{rel}"), opts)
            .map_err(zerr)?;
        let mut f = File::open(entry.path()).map_err(io_ctx(entry.path().display().to_string()))?;
        io::copy(&mut f, &mut zip).map_err(io_ctx(out.display().to_string()))?;
    }
    let mut w = zip.finish().map_err(zerr)?;
    w.flush().map_err(io_ctx(out.display().to_string()))?;
    Ok(())
}

/// What [`package`] produced.
#[derive(Clone, Debug, Serialize)]
pub struct Published {
    pub kind: EntryKind,
    pub archive: PathBuf,
    pub entry: MarketEntry,
    pub warnings: Vec<String>,
}

/// Validates a plugin folder, an SDK folder or a `.ptk` file and prepares
/// it for the marketplace: a zip (plugins, SDKs) in `out_dir`, its SHA-256,
/// and the index entry to submit.
pub fn package(src: &Path, out_dir: &Path, engine: &mut EngineClient) -> Result<Published> {
    fs::create_dir_all(out_dir).map_err(io_ctx(out_dir.display().to_string()))?;
    let is_ptk = src.is_file()
        && src
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ptk"));
    if is_ptk {
        let m = read_pack_manifest(src)?;
        let sha256 = sha256_file(src).map_err(io_ctx(src.display().to_string()))?;
        return Ok(Published {
            kind: EntryKind::Project,
            archive: src.to_owned(),
            entry: MarketEntry {
                id: m.name.clone(),
                name: m.game.title.clone().unwrap_or(m.name.clone()),
                kind: EntryKind::Project,
                version: m.version.clone(),
                description: m.description.clone(),
                author: m.authors.join(", "),
                license: m.license.clone(),
                homepage: None,
                download: DOWNLOAD_PLACEHOLDER.into(),
                sha256,
                reviewed: false,
                engines: m.game.engine.clone().into_iter().collect(),
                targets: m.targets.clone(),
            },
            warnings: Vec::new(),
        });
    }
    if src.join(SDK_FILE).is_file() {
        let report = sdk::validate(src, engine)?;
        if !report.valid {
            return Err(Error::Invalid(format!(
                "the SDK is not valid:\n  - {}",
                report.errors.join("\n  - ")
            )));
        }
        let info = report.sdk.clone().expect("valid SDK");
        let archive = out_dir.join(format!("{}-{}.zip", info.id, info.version));
        zip_dir(src, &archive, &info.id)?;
        let sha256 = sha256_file(&archive).map_err(io_ctx(archive.display().to_string()))?;
        return Ok(Published {
            kind: EntryKind::Sdk,
            archive,
            entry: MarketEntry {
                id: info.id.clone(),
                name: info.name.clone(),
                kind: EntryKind::Sdk,
                version: info.version.clone(),
                description: info.description.clone(),
                author: info.authors.join(", "),
                license: info.license.clone(),
                homepage: info.homepage.clone(),
                download: DOWNLOAD_PLACEHOLDER.into(),
                sha256,
                reviewed: false,
                engines: Vec::new(),
                targets: report.targets.iter().map(|t| t.id.clone()).collect(),
            },
            warnings: report.warnings,
        });
    }
    if src.join("plugin.toml").is_file() {
        let (v, raw) = plugins::validate(engine, src)?;
        if !v.valid {
            return Err(Error::Invalid(format!(
                "the plugin is not valid:\n  - {}",
                v.errors.join("\n  - ")
            )));
        }
        let info = v.plugin.clone().expect("valid plugin");
        let id = info.id.clone().unwrap_or_default();
        let version = info.version.clone().unwrap_or_default();
        let archive = out_dir.join(format!("{id}-{version}.zip"));
        zip_dir(src, &archive, &id)?;
        let sha256 = sha256_file(&archive).map_err(io_ctx(archive.display().to_string()))?;
        let list = |k: &str| -> Vec<String> {
            raw["plugin"][k]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        return Ok(Published {
            kind: EntryKind::Plugin,
            archive,
            entry: MarketEntry {
                id: id.clone(),
                name: info.name.clone().unwrap_or(id),
                kind: EntryKind::Plugin,
                version,
                description: info.description.clone().unwrap_or_default(),
                author: String::new(),
                license: info.license.clone().unwrap_or_default(),
                homepage: None,
                download: DOWNLOAD_PLACEHOLDER.into(),
                sha256,
                reviewed: false,
                engines: list("inputs"),
                targets: list("targets"),
            },
            warnings: v.warnings,
        });
    }
    Err(Error::Invalid(format!(
        "{} is not a plugin folder (plugin.toml), an SDK folder ({SDK_FILE}) or a .ptk file",
        src.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> MarketEntry {
        MarketEntry {
            id: "unity-import".into(),
            name: "Unity import".into(),
            kind: EntryKind::Plugin,
            version: "0.1.0".into(),
            description: "Reads Unity games".into(),
            author: "someone".into(),
            license: "MIT".into(),
            homepage: None,
            download: "https://example.org/unity-import-0.1.0.zip".into(),
            sha256: crate::hash::sha256_bytes(b"x"),
            reviewed: true,
            engines: vec!["unity".into()],
            targets: vec![],
        }
    }

    #[test]
    fn index_parsing_drops_broken_entries() {
        let mut bad = entry();
        bad.id = "Bad Id".into();
        let mut http = entry();
        http.id = "plain-http".into();
        http.download = "http://example.org/x.zip".into();
        let index = MarketIndex {
            schema_version: 1,
            entries: vec![entry(), bad, http],
        };
        let (parsed, problems) = parse_index(&serde_json::to_vec(&index).unwrap()).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(problems.len(), 2);
        assert!(parse_index(br#"{"schema_version": 9}"#).is_err());
    }

    #[test]
    fn search() {
        let e = entry();
        assert!(e.matches(""));
        assert!(e.matches("UNITY"));
        assert!(e.matches("reads"));
        assert!(!e.matches("godot"));
    }
}
