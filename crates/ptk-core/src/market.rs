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
use crate::hash::{is_junk_file, is_sha256_hex, relative_path, sha256_bytes, sha256_file};
use crate::manifest::{SourceFile, valid_project_name};
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

/// The first index read (see [`index_urls`]).
pub fn index_url() -> String {
    index_urls()
        .into_iter()
        .next()
        .unwrap_or_else(|| DEFAULT_INDEX_URL.into())
}

/// The indexes to read: `PTK_MARKETPLACE_URL` when set, else the sources in
/// the app settings, else the default index.
pub fn index_urls() -> Vec<String> {
    index_urls_from(&crate::settings::AppSettings::load())
}

/// [`index_urls`] for given settings (the app's unsaved ones, for example).
pub fn index_urls_from(settings: &crate::settings::AppSettings) -> Vec<String> {
    if let Some(v) = std::env::var("PTK_MARKETPLACE_URL")
        .ok()
        .filter(|v| !v.is_empty())
    {
        return vec![v];
    }
    if settings.marketplace_sources.is_empty() {
        vec![DEFAULT_INDEX_URL.into()]
    } else {
        settings.marketplace_sources.clone()
    }
}

/// Whether a marketplace source is a URL Porting Toolkit reads.
pub fn valid_source(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("https://") && u.len() > "https://".len() && !u.contains(char::is_whitespace))
        || u.starts_with("file://")
}

/// Merges indexes: for the same id and kind, the newest version wins (the
/// first source on a tie). Entries keep the order they first appear in.
pub fn merge(indexes: Vec<MarketIndex>) -> MarketIndex {
    let mut out: Vec<MarketEntry> = Vec::new();
    for index in indexes {
        for e in index.entries {
            match out.iter_mut().find(|x| x.id == e.id && x.kind == e.kind) {
                Some(x) if version_newer(&e.version, &x.version) => *x = e,
                Some(_) => {}
                None => out.push(e),
            }
        }
    }
    MarketIndex {
        schema_version: 1,
        entries: out,
    }
}

/// Reads every source and merges them ([`merge`]). A source that cannot be
/// read is reported with the skipped entries; it is an error only when no
/// source could be read.
pub fn fetch_all(urls: &[String]) -> Result<(MarketIndex, Vec<String>)> {
    fetch_all_with(urls, fetch_index)
}

fn fetch_all_with(
    urls: &[String],
    fetch: impl Fn(&str) -> Result<(MarketIndex, Vec<String>)>,
) -> Result<(MarketIndex, Vec<String>)> {
    let mut indexes = Vec::new();
    let mut problems = Vec::new();
    let mut errors = Vec::new();
    for url in urls {
        match fetch(url) {
            Ok((index, p)) => {
                indexes.push(index);
                problems.extend(p);
            }
            Err(e) => errors.push(format!("{url}: {e}")),
        }
    }
    if indexes.is_empty() {
        return Err(Error::Invalid(if errors.is_empty() {
            "no marketplace source is set".into()
        } else {
            errors.join("; ")
        }));
    }
    problems.extend(
        errors
            .into_iter()
            .map(|e| format!("source not loaded: {e}")),
    );
    Ok((merge(indexes), problems))
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
            EntryKind::Project => "Game port",
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
    /// Game ports: the game's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// Game ports: an https URL of the poster image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    /// Game ports: where to get the game (a store page). A port never
    /// contains the game; players need their own copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    /// Game ports: a few distinctive files of the game the port was made
    /// for (path, size, SHA-256), so players can check their copy before
    /// downloading. Hashes only, never game content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprint: Vec<SourceFile>,
}

/// How many files a port's fingerprint holds, and the largest one hashed
/// (bigger files take players too long to check).
pub const FINGERPRINT_FILES: usize = 4;
pub const FINGERPRINT_MAX_SIZE: u64 = 1 << 30;

/// The fingerprint of a project's game: its largest recorded files (they
/// change from one game version to the next) up to 1 GB each.
pub fn fingerprint_of(manifest: &crate::manifest::Manifest) -> Vec<SourceFile> {
    let mut files: Vec<&SourceFile> = manifest
        .source_files
        .iter()
        .filter(|f| f.size > 0 && f.size <= FINGERPRINT_MAX_SIZE && !is_junk_file(&f.path))
        .collect();
    files.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
    files.into_iter().take(FINGERPRINT_FILES).cloned().collect()
}

/// Whether a player's copy is the game a port was made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyVerdict {
    /// Every fingerprint file is there, identical.
    Matches,
    /// Some files match: most likely another version of the game.
    OtherVersion,
    /// The files are there but none is identical.
    Different,
    /// None of the files was found in the folder.
    NotFound,
    /// The entry has no fingerprint to compare with.
    Unknown,
}

impl CopyVerdict {
    pub fn label(self) -> &'static str {
        match self {
            CopyVerdict::Matches => "Matches your copy",
            CopyVerdict::OtherVersion => "Another version of the game",
            CopyVerdict::Different => "Different files",
            CopyVerdict::NotFound => "Game files not found",
            CopyVerdict::Unknown => "Cannot be checked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CopyCheck {
    pub verdict: CopyVerdict,
    pub matched: usize,
    pub total: usize,
    /// Fingerprint paths found but different, and not found.
    pub different: Vec<String>,
    pub missing: Vec<String>,
}

/// Compares a game folder with a port's fingerprint. A file is looked up
/// at its path first, then by name and size anywhere in the folder (the
/// player may have picked a parent folder or a differently named one).
pub fn check_copy(entry: &MarketEntry, game: &Path) -> Result<CopyCheck> {
    let root = if game.is_file() {
        game.parent().unwrap_or(game)
    } else {
        game
    };
    let total = entry.fingerprint.len();
    let mut check = CopyCheck {
        verdict: CopyVerdict::Unknown,
        matched: 0,
        total,
        different: Vec::new(),
        missing: Vec::new(),
    };
    if total == 0 {
        return Ok(check);
    }
    let mut by_name: Option<Vec<(String, u64, PathBuf)>> = None;
    for f in &entry.fingerprint {
        if !crate::manifest::is_safe_relative_path(&f.path) {
            check.missing.push(f.path.clone());
            continue;
        }
        let mut candidates = Vec::new();
        let direct = root.join(&f.path);
        if direct.is_file() {
            candidates.push(direct);
        } else {
            let index = by_name.get_or_insert_with(|| {
                WalkDir::new(root)
                    .follow_links(false)
                    .max_depth(8)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter_map(|e| {
                        let size = e.metadata().ok()?.len();
                        Some((
                            e.file_name().to_string_lossy().to_lowercase(),
                            size,
                            e.into_path(),
                        ))
                    })
                    .collect()
            });
            let name = f.path.rsplit('/').next().unwrap_or(&f.path).to_lowercase();
            candidates.extend(
                index
                    .iter()
                    .filter(|(n, size, _)| *n == name && *size == f.size)
                    .map(|(_, _, p)| p.clone()),
            );
            if candidates.is_empty() && index.iter().any(|(n, _, _)| *n == name) {
                check.different.push(f.path.clone());
                continue;
            }
        }
        if candidates.is_empty() {
            check.missing.push(f.path.clone());
            continue;
        }
        let mut same = false;
        for c in candidates.iter().take(4) {
            let meta = fs::metadata(c).map_err(io_ctx(c.display().to_string()))?;
            if meta.len() == f.size
                && sha256_file(c).map_err(io_ctx(c.display().to_string()))? == f.sha256
            {
                same = true;
                break;
            }
        }
        if same {
            check.matched += 1;
        } else {
            check.different.push(f.path.clone());
        }
    }
    check.verdict = if check.matched == total {
        CopyVerdict::Matches
    } else if check.matched > 0 {
        CopyVerdict::OtherVersion
    } else if !check.different.is_empty() {
        CopyVerdict::Different
    } else {
        CopyVerdict::NotFound
    };
    Ok(check)
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
        for f in &self.fingerprint {
            if !crate::manifest::is_safe_relative_path(&f.path) || !is_sha256_hex(&f.sha256) {
                p.push(format!("{}: bad fingerprint file {:?}", self.id, f.path));
            }
        }
        for (field, url) in [("poster", &self.poster), ("store", &self.store)] {
            if let Some(u) = url
                && !(u.starts_with("https://") || (field == "poster" && u.starts_with("file://")))
            {
                p.push(format!("{}: {field} must be an https:// URL", self.id));
            }
        }
        p
    }

    /// Searchable text.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || self
                .game
                .as_ref()
                .is_some_and(|g| g.to_lowercase().contains(&q))
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
    /// A game port's poster, written next to the archive for uploading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_file: Option<PathBuf>,
}

/// One file from a `.ptk` (a zip), if present and small.
fn read_pack_member(pack: &Path, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(File::open(pack).ok()?).ok()?;
    let f = z.by_name(name).ok()?;
    if f.size() > crate::art::MAX_BYTES {
        return None;
    }
    let mut out = Vec::new();
    f.take(crate::art::MAX_BYTES).read_to_end(&mut out).ok()?;
    Some(out)
}

/// Downloads a poster into the cache (once) and returns the local file.
pub fn fetch_poster(url: &str) -> Result<PathBuf> {
    if !url.starts_with("https://") && !url.starts_with("file://") {
        return Err(Error::Invalid(format!("poster must be https: {url}")));
    }
    let dir = crate::paths::data_dir().join("cache").join("posters");
    fs::create_dir_all(&dir).map_err(io_ctx(dir.display().to_string()))?;
    let file = dir.join(&sha256_bytes(url.as_bytes())[..24]);
    if !file.is_file() {
        let partial = file.with_extension("partial");
        curl(url, &partial)?;
        let size = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
        if size > crate::art::MAX_BYTES {
            let _ = fs::remove_file(&partial);
            return Err(Error::Invalid(format!("poster {url} is larger than 8 MB")));
        }
        fs::rename(&partial, &file).map_err(io_ctx(file.display().to_string()))?;
    }
    Ok(file)
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
        let mut warnings = vec![
            "Set \"store\" to a page where players can get the game: a port never contains it."
                .to_owned(),
        ];
        if m.source_files.is_empty() {
            warnings.push(
                "No file hashes recorded: players cannot check that their copy matches (ptk scan --record).".into(),
            );
        }
        let mut poster = None;
        let mut poster_file = None;
        if let Some(rel) = &m.game.poster
            && let Some(bytes) = read_pack_member(src, rel)
        {
            let ext = rel.rsplit('.').next().unwrap_or("png");
            let out = out_dir.join(format!("{}-poster.{ext}", m.name));
            fs::write(&out, bytes).map_err(io_ctx(out.display().to_string()))?;
            poster_file = Some(out.clone());
            warnings.push(format!(
                "Upload {} next to the .ptk and put its https URL in \"poster\".",
                out.display()
            ));
            poster = Some("REPLACE-with-the-URL-of-the-uploaded-poster".to_owned());
        } else {
            warnings
                .push("No poster: add one (ptk art --poster IMAGE) so the port stands out.".into());
        }
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
                game: m.game.title.clone(),
                poster,
                store: Some("REPLACE-with-a-page-where-players-get-the-game".into()),
                fingerprint: fingerprint_of(&m),
            },
            poster_file,
            warnings,
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
                game: None,
                poster: None,
                store: None,
                fingerprint: Vec::new(),
            },
            poster_file: None,
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
                game: None,
                poster: None,
                store: None,
                fingerprint: Vec::new(),
            },
            poster_file: None,
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

    #[test]
    fn game_ports_have_https_poster_and_store() {
        let json = r#"{"id":"quest-port","name":"Quest","kind":"project","version":"1.0.0","license":"MIT",
            "download":"https://example.org/quest.ptk","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "game":"Quest","poster":"https://example.org/p.png","store":"https://store.example.org/quest"}"#;
        let mut e: MarketEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.kind.label(), "Game port");
        assert!(e.problems().is_empty(), "{:?}", e.problems());
        assert!(e.matches("quest"));
        e.store = Some("http://store.example.org".into());
        assert!(e.problems().iter().any(|p| p.contains("store")));
    }

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
            game: None,
            poster: None,
            store: None,
            fingerprint: Vec::new(),
        }
    }

    fn source(path: &str, data: &[u8]) -> SourceFile {
        SourceFile {
            path: path.into(),
            size: data.len() as u64,
            sha256: crate::hash::sha256_bytes(data),
        }
    }

    #[test]
    fn sources_merge_newest_first_and_survive_a_failing_one() {
        let mut old = entry();
        old.version = "0.1.0".into();
        let mut new = entry();
        new.version = "0.2.0".into();
        new.download =
            "https://codeberg.org/x/ptk-marketplace/raw/branch/main/unity-import-0.2.0.zip".into();
        let mut other = entry();
        other.id = "scummvm-export".into();
        let a = MarketIndex {
            schema_version: 1,
            entries: vec![old.clone(), other.clone()],
        };
        let b = MarketIndex {
            schema_version: 1,
            entries: vec![new.clone()],
        };
        let merged = merge(vec![a.clone(), b.clone()]);
        assert_eq!(merged.entries.len(), 2);
        assert_eq!(merged.entries[0].version, "0.2.0");
        assert_eq!(
            merge(vec![b.clone(), a.clone()]).entries[0].version,
            "0.2.0"
        );

        let urls = vec![
            "https://a/index.json".to_string(),
            "https://down/index.json".to_string(),
        ];
        let (index, problems) = fetch_all_with(&urls, |u| {
            if u.contains("down") {
                Err(Error::Invalid("HTTP 404".into()))
            } else {
                Ok((a.clone(), vec![]))
            }
        })
        .unwrap();
        assert_eq!(index.entries.len(), 2);
        assert!(problems[0].contains("source not loaded: https://down/index.json"));
        assert!(fetch_all_with(&urls[1..], |_| Err(Error::Invalid("HTTP 404".into()))).is_err());
    }

    #[test]
    fn sources_come_from_settings() {
        assert!(valid_source(
            "https://codeberg.org/x/ptk-marketplace/raw/branch/main/index.json"
        ));
        assert!(valid_source("file:///tmp/index.json"));
        assert!(!valid_source("http://example.org/index.json") && !valid_source("https://"));
        if std::env::var_os("PTK_MARKETPLACE_URL").is_none() {
            let mut s = crate::settings::AppSettings::default();
            assert_eq!(index_urls_from(&s), vec![DEFAULT_INDEX_URL.to_string()]);
            s.marketplace_sources =
                vec!["https://a/index.json".into(), "https://b/index.json".into()];
            assert_eq!(index_urls_from(&s).len(), 2);
        }
    }

    #[test]
    fn fingerprint_takes_the_largest_files() {
        let mut m = crate::manifest::Manifest::new("moon-port");
        m.source_files = vec![
            source("a.txt", b"a"),
            source("Data/big.pak", &[7; 500]),
            source("Game.exe", &[1; 300]),
            source("Data/mid.bin", &[2; 200]),
            source("Data/small.bin", &[3; 100]),
            source(".DS_Store", &[0; 900]),
            source("empty", b""),
        ];
        let fp: Vec<_> = fingerprint_of(&m).into_iter().map(|f| f.path).collect();
        assert_eq!(
            fp,
            ["Data/big.pak", "Game.exe", "Data/mid.bin", "Data/small.bin"]
        );
    }

    #[test]
    fn copy_check_verdicts() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("Moon Game");
        fs::create_dir_all(game.join("Data")).unwrap();
        fs::write(game.join("Game.exe"), [1; 300]).unwrap();
        fs::write(game.join("Data/big.pak"), [7; 500]).unwrap();
        let mut e = entry();
        e.kind = EntryKind::Project;
        e.id = "moon-port".into();
        e.fingerprint = vec![
            source("Game.exe", &[1; 300]),
            source("Data/big.pak", &[7; 500]),
        ];
        assert!(e.problems().is_empty(), "{:?}", e.problems());
        let c = check_copy(&e, &game).unwrap();
        assert_eq!(
            (c.verdict, c.matched, c.total),
            (CopyVerdict::Matches, 2, 2)
        );
        // The player picked the parent folder: found by name and size.
        assert_eq!(
            check_copy(&e, dir.path()).unwrap().verdict,
            CopyVerdict::Matches
        );
        // Another version: one file differs.
        fs::write(game.join("Data/big.pak"), [8; 500]).unwrap();
        let c = check_copy(&e, &game).unwrap();
        assert_eq!(c.verdict, CopyVerdict::OtherVersion);
        assert_eq!(c.different, ["Data/big.pak"]);
        fs::write(game.join("Game.exe"), [9; 301]).unwrap();
        assert_eq!(
            check_copy(&e, &game).unwrap().verdict,
            CopyVerdict::Different
        );
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            check_copy(&e, &empty).unwrap().verdict,
            CopyVerdict::NotFound
        );
        e.fingerprint.clear();
        assert_eq!(check_copy(&e, &game).unwrap().verdict, CopyVerdict::Unknown);
        e.fingerprint = vec![source("../escape", b"x")];
        assert!(e.problems().iter().any(|p| p.contains("fingerprint")));
    }

    #[test]
    fn versions_and_updates() {
        assert!(version_newer("1.10.0", "1.9.2"));
        assert!(version_newer("2.0", "1.99.99"));
        assert!(version_newer("v1.0.1", "1.0"));
        assert!(!version_newer("1.0.0", "1.0"));
        assert!(!version_newer("0.9", "1.0"));
        let mut newer = entry();
        newer.version = "0.2.0".into();
        let index = MarketIndex {
            schema_version: 1,
            entries: vec![newer],
        };
        let items = vec![
            InstalledItem {
                kind: EntryKind::Plugin,
                id: "unity-import".into(),
                version: "0.1.0".into(),
                path: PathBuf::from("/p"),
            },
            InstalledItem {
                kind: EntryKind::Sdk,
                id: "unity-import".into(),
                version: "0.0.1".into(),
                path: PathBuf::from("/s"),
            },
        ];
        let up = outdated(&index, &items);
        assert_eq!(up.len(), 1, "only the same kind counts");
        assert_eq!(
            (up[0].installed.as_str(), up[0].available.as_str()),
            ("0.1.0", "0.2.0")
        );
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

/// Something from the marketplace that is installed (or a port project).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstalledItem {
    pub kind: EntryKind,
    pub id: String,
    pub version: String,
    pub path: PathBuf,
}

/// A newer version in the index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Update {
    pub kind: EntryKind,
    pub id: String,
    pub installed: String,
    pub available: String,
    pub path: PathBuf,
}

/// `1.10.0` > `1.9.2`: numeric parts compared as numbers.
pub fn version_newer(available: &str, installed: &str) -> bool {
    fn key(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (key(available), key(installed));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// Installed plugins and SDKs, plus the given project folders (ports).
pub fn installed(project_dirs: &[PathBuf]) -> Vec<InstalledItem> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(paths::plugins_dir()) {
        let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        dirs.sort();
        for dir in dirs {
            let Ok(text) = fs::read_to_string(dir.join("plugin.toml")) else {
                continue;
            };
            let Ok(value) = text.parse::<toml::Table>() else {
                continue;
            };
            let plugin = value.get("plugin").and_then(|v| v.as_table());
            let get = |k: &str| {
                plugin
                    .and_then(|p| p.get(k))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            };
            if let (Some(id), Some(version)) = (get("id"), get("version")) {
                out.push(InstalledItem {
                    kind: EntryKind::Plugin,
                    id,
                    version,
                    path: dir,
                });
            }
        }
    }
    for info in sdk::installed() {
        out.push(InstalledItem {
            kind: EntryKind::Sdk,
            path: sdks_dir_of(&info.id),
            id: info.id,
            version: info.version,
        });
    }
    for dir in project_dirs {
        if let Ok(m) = crate::manifest::Manifest::load(dir) {
            out.push(InstalledItem {
                kind: EntryKind::Project,
                id: m.name,
                version: m.version,
                path: dir.clone(),
            });
        }
    }
    out
}

fn sdks_dir_of(id: &str) -> PathBuf {
    sdk::sdks_dir().join(id)
}

/// Port projects in a folder (each subfolder with a ptk.json).
pub fn projects_in(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join(crate::manifest::MANIFEST_FILE).is_file())
        .collect();
    out.sort();
    out
}

/// Installed items whose index entry (same kind and id) is newer.
pub fn outdated(index: &MarketIndex, items: &[InstalledItem]) -> Vec<Update> {
    let mut out = Vec::new();
    for item in items {
        if let Some(e) = index
            .entries
            .iter()
            .find(|e| e.kind == item.kind && e.id == item.id)
            && version_newer(&e.version, &item.version)
        {
            out.push(Update {
                kind: item.kind,
                id: item.id.clone(),
                installed: item.version.clone(),
                available: e.version.clone(),
                path: item.path.clone(),
            });
        }
    }
    out
}
