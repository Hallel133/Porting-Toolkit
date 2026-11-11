//! `.ptk` community project files.
//!
//! A `.ptk` file is a zip archive with:
//!
//! - `ptk.json`: the manifest (always first),
//! - `README.md`, `LICENSE*`, `plugins.lock` if present,
//! - `recipes/`, `patches/` and `extra/`: community-written work,
//! - `SHA256SUMS`: the hash of every other entry, checked on unpack.
//!
//! Nothing else from the project folder is ever packed, and files that are
//! third-party content are refused unless the manifest declares a license
//! that allows redistribution (see [`find_third_party`]). Original game
//! files belong to the user and are only referenced by hash.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::glob::glob_match;
use crate::hash::{is_junk_file, is_sha256_hex, relative_path, sha256_file};
use crate::manifest::{Issue, MANIFEST_FILE, Manifest, ManifestError, is_safe_relative_path};
use crate::targets::TargetRegistry;

/// File extension of community project files.
pub const PACK_EXTENSION: &str = "ptk";
/// Checksums entry inside a pack.
pub const CHECKSUMS_FILE: &str = "SHA256SUMS";
/// Locked plugin versions of a project.
pub const PLUGINS_LOCK: &str = "plugins.lock";
/// Top-level files that are packed when present.
pub const PACK_FILES: &[&str] = &[
    MANIFEST_FILE,
    "README.md",
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    PLUGINS_LOCK,
];
/// Folders that are packed recursively when present.
pub const PACK_DIRS: &[&str] = &["recipes", "patches", "extra", "art"];

/// File extensions of engine archives. Such files are game content, so a
/// pack treats them as third-party even when nobody marked them.
pub const GAME_ARCHIVE_EXTENSIONS: &[&str] = &[
    "assets", "ba2", "bsa", "bundle", "pak", "pck", "resource", "ress", "rgss2a", "rgss3a",
    "rgssad", "rpa", "rpf", "ucas", "unity3d", "utoc", "vpk", "wad",
];
/// File names that are game content in the same way.
pub const GAME_ARCHIVE_NAMES: &[&str] = &[
    "data.win",
    "game.unx",
    "game.ios",
    "game.droid",
    "globalgamemanagers",
];

const MAX_ENTRIES: usize = 100_000;
const MAX_UNPACKED_BYTES: u64 = 4 << 30;
const MAX_METADATA_BYTES: u64 = 16 << 20;

/// Why a file counts as third-party content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum ThirdPartyReason {
    /// Matches a glob in the manifest's `third_party` list.
    Marked(String),
    /// Same SHA-256 as a recorded original game file.
    CopyOfGameFile(String),
    /// Has the name or extension of an engine archive.
    GameArchive,
}

/// A packed-file candidate that is third-party content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ThirdPartyFile {
    pub path: String,
    pub reason: ThirdPartyReason,
}

impl fmt::Display for ThirdPartyFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.reason {
            ThirdPartyReason::Marked(glob) => {
                write!(f, "{} (marked third-party by {glob:?})", self.path)
            }
            ThirdPartyReason::CopyOfGameFile(src) => {
                write!(f, "{} (identical to the game file {src})", self.path)
            }
            ThirdPartyReason::GameArchive => write!(f, "{} (engine archive)", self.path),
        }
    }
}

/// Errors of pack, unpack and pack reading.
#[derive(Debug, Error)]
pub enum PackError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error(
        "refusing to pack third-party content:\n{}\nA .ptk file may only contain community work. Remove these files, or set game.content_license to a license that allows redistribution (an open-source game, or \"LicenseRef-Own-Work\" for your own game).",
        .0.iter().map(|f| format!("  - {f}")).collect::<Vec<_>>().join("\n")
    )]
    ThirdParty(Vec<ThirdPartyFile>),
    #[error("symbolic links cannot be packed: {0}")]
    Symlink(String),
    #[error("invalid pack: {0}")]
    Invalid(String),
    #[error("checksum mismatch for {0}: the pack is damaged or was modified")]
    Checksum(String),
    #[error("{0} already exists")]
    Exists(PathBuf),
    #[error("plugins.lock: {0}")]
    PluginsLock(String),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> PackError + '_ {
    move |source| PackError::Io {
        path: path.to_owned(),
        source,
    }
}

/// One file inside a pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackedEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// `plugins.lock`: exact plugin versions a project was made with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginsLock {
    pub version: u32,
    #[serde(default)]
    pub plugin: Vec<LockedPlugin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl PluginsLock {
    /// The text `ptk init` writes.
    pub const TEMPLATE: &'static str = "# Exact plugin versions this project was made with.\n# Filled in by `ptk` when plugins run.\nversion = 1\n\n# [[plugin]]\n# id = \"example-import\"\n# version = \"0.1.0\"\n# sha256 = \"...\"\n";

    pub fn parse(text: &str) -> Result<Self, PackError> {
        let lock: PluginsLock =
            toml::from_str(text).map_err(|e| PackError::PluginsLock(e.to_string()))?;
        if lock.version != 1 {
            return Err(PackError::PluginsLock(format!(
                "version {} is not supported (expected 1)",
                lock.version
            )));
        }
        for p in &lock.plugin {
            if !crate::targets::valid_target_id(&p.id) {
                return Err(PackError::PluginsLock(format!(
                    "invalid plugin id {:?}",
                    p.id
                )));
            }
            if p.sha256.as_deref().is_some_and(|h| !is_sha256_hex(h)) {
                return Err(PackError::PluginsLock(format!("bad sha256 for {:?}", p.id)));
            }
        }
        Ok(lock)
    }

    /// Reads `plugins.lock` from a project folder, if there is one.
    pub fn load(project: &Path) -> Result<Option<Self>, PackError> {
        let path = project.join(PLUGINS_LOCK);
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(&path)(e)),
        }
    }
}

/// True for paths that belong in a pack.
pub fn is_pack_path(path: &str) -> bool {
    if !is_safe_relative_path(path) {
        return false;
    }
    match path.split_once('/') {
        None => PACK_FILES.contains(&path),
        Some((dir, _)) => PACK_DIRS.contains(&dir),
    }
}

/// True for engine archive names (`*.rpf`, `*.pak`, `data.win`, ...).
pub fn looks_like_game_archive(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if GAME_ARCHIVE_NAMES.contains(&name.as_str()) {
        return true;
    }
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| GAME_ARCHIVE_EXTENSIONS.contains(&ext))
}

/// Default output name, `<name>-<version>.ptk`.
pub fn default_pack_name(m: &Manifest) -> String {
    format!("{}-{}.{PACK_EXTENSION}", m.name, m.version)
}

/// Lists the project files that go into a pack, sorted, `ptk.json` first.
pub fn collect_project_files(project: &Path) -> Result<Vec<String>, PackError> {
    let mut files = Vec::new();
    for name in PACK_FILES {
        let path = project.join(name);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_symlink() => return Err(PackError::Symlink((*name).into())),
            Ok(m) if m.is_file() => files.push((*name).to_owned()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&path)(e)),
        }
    }
    if files.first().map(String::as_str) != Some(MANIFEST_FILE) {
        return Err(ManifestError::Io {
            path: project.join(MANIFEST_FILE),
            source: io::Error::new(io::ErrorKind::NotFound, "no ptk.json (run `ptk init`)"),
        }
        .into());
    }
    for dir in PACK_DIRS {
        let root = project.join(dir);
        match fs::symlink_metadata(&root) {
            Ok(m) if m.file_type().is_symlink() => return Err(PackError::Symlink((*dir).into())),
            Ok(m) if m.is_dir() => {}
            _ => continue,
        }
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !(e.file_type().is_dir() && (n == ".git" || n == "__pycache__"))
            });
        for entry in walker {
            let entry = entry.map_err(|e| io_err(&root)(io::Error::other(e)))?;
            let rel = relative_path(project, entry.path()).ok_or_else(|| {
                PackError::Invalid(format!("{} is not a UTF-8 path", entry.path().display()))
            })?;
            let ft = entry.file_type();
            if ft.is_symlink() {
                return Err(PackError::Symlink(rel));
            }
            if ft.is_file() && !is_junk_file(&entry.file_name().to_string_lossy()) {
                files.push(rel);
            }
        }
    }
    files[1..].sort();
    Ok(files)
}

/// Finds third-party content among pack entries.
pub fn find_third_party(manifest: &Manifest, entries: &[PackedEntry]) -> Vec<ThirdPartyFile> {
    let game_hashes: HashMap<&str, &str> = manifest
        .source_files
        .iter()
        .filter(|f| f.size > 0)
        .map(|f| (f.sha256.as_str(), f.path.as_str()))
        .collect();
    let mut out = Vec::new();
    for e in entries {
        if e.path == MANIFEST_FILE {
            continue;
        }
        let reason =
            if let Some(glob) = manifest.third_party.iter().find(|g| glob_match(g, &e.path)) {
                ThirdPartyReason::Marked(glob.clone())
            } else if let Some(src) = (e.size > 0)
                .then(|| game_hashes.get(e.sha256.as_str()))
                .flatten()
            {
                ThirdPartyReason::CopyOfGameFile((*src).to_owned())
            } else if looks_like_game_archive(&e.path) {
                ThirdPartyReason::GameArchive
            } else {
                continue;
            };
        out.push(ThirdPartyFile {
            path: e.path.clone(),
            reason,
        });
    }
    out
}

/// Options for [`pack`].
#[derive(Clone, Copy, Debug, Default)]
pub struct PackOptions {
    /// Replace an existing output file.
    pub overwrite: bool,
}

/// Result of [`pack`].
#[derive(Debug)]
pub struct PackReport {
    pub output: PathBuf,
    pub manifest: Manifest,
    pub entries: Vec<PackedEntry>,
    /// Third-party files that were packed because the content license allows it.
    pub licensed_third_party: Vec<ThirdPartyFile>,
    pub warnings: Vec<Issue>,
    pub bytes: u64,
}

/// Packs a project folder into a `.ptk` file.
pub fn pack(
    project: &Path,
    output: &Path,
    targets: &TargetRegistry,
    options: PackOptions,
) -> Result<PackReport, PackError> {
    let manifest = Manifest::load(project)?;
    let warnings = manifest.check(&project.join(MANIFEST_FILE), targets)?;
    PluginsLock::load(project)?;

    let mut entries = Vec::new();
    for rel in collect_project_files(project)? {
        let path = project.join(&rel);
        let size = fs::metadata(&path).map_err(io_err(&path))?.len();
        let sha256 = sha256_file(&path).map_err(io_err(&path))?;
        entries.push(PackedEntry {
            path: rel,
            size,
            sha256,
        });
    }

    let third_party = find_third_party(&manifest, &entries);
    if !third_party.is_empty() && !manifest.allows_redistribution() {
        return Err(PackError::ThirdParty(third_party));
    }

    if output.exists() && !options.overwrite {
        return Err(PackError::Exists(output.to_owned()));
    }
    let partial = output.with_extension("ptk.partial");
    let result = write_zip(project, &partial, &entries);
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    fs::rename(&partial, output).map_err(io_err(output))?;
    let bytes = fs::metadata(output).map_err(io_err(output))?.len();
    Ok(PackReport {
        output: output.to_owned(),
        manifest,
        entries,
        licensed_third_party: third_party,
        warnings,
        bytes,
    })
}

fn write_zip(project: &Path, out: &Path, entries: &[PackedEntry]) -> Result<(), PackError> {
    let file = File::create(out).map_err(io_err(out))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    // Fixed timestamps and permissions make packs reproducible.
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for e in entries {
        let path = project.join(&e.path);
        zip.start_file(e.path.as_str(), options)?;
        let mut src = File::open(&path).map_err(io_err(&path))?;
        io::copy(&mut src, &mut zip).map_err(io_err(&path))?;
    }
    zip.start_file(CHECKSUMS_FILE, options)?;
    for e in entries {
        writeln!(zip, "{}  {}", e.sha256, e.path).map_err(io_err(out))?;
    }
    let mut inner = zip.finish()?;
    inner.flush().map_err(io_err(out))?;
    inner
        .into_inner()
        .map_err(|e| io_err(out)(e.into_error()))?
        .sync_all()
        .map_err(io_err(out))?;
    Ok(())
}

fn open_archive(pack: &Path) -> Result<ZipArchive<File>, PackError> {
    let file = File::open(pack).map_err(io_err(pack))?;
    let archive = ZipArchive::new(file)?;
    if archive.len() > MAX_ENTRIES {
        return Err(PackError::Invalid(format!(
            "more than {MAX_ENTRIES} entries"
        )));
    }
    Ok(archive)
}

fn read_small(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<Vec<u8>>, PackError> {
    let mut entry = match archive.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut buf = Vec::new();
    (&mut entry)
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| PackError::Invalid(format!("{name}: {e}")))?;
    if buf.len() as u64 > MAX_METADATA_BYTES {
        return Err(PackError::Invalid(format!("{name} is too large")));
    }
    Ok(Some(buf))
}

/// Reads only the manifest of a `.ptk` file.
pub fn read_pack_manifest(pack: &Path) -> Result<Manifest, PackError> {
    let mut archive = open_archive(pack)?;
    let bytes = read_small(&mut archive, MANIFEST_FILE)?
        .ok_or_else(|| PackError::Invalid("no ptk.json in the pack".into()))?;
    Ok(Manifest::from_slice(&bytes, &pack.join(MANIFEST_FILE))?)
}

/// Loads a manifest from a project folder, a `ptk.json` file or a `.ptk` pack.
pub fn load_any_manifest(path: &Path) -> Result<Manifest, PackError> {
    let is_pack = path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(PACK_EXTENSION));
    if is_pack {
        read_pack_manifest(path)
    } else {
        Ok(Manifest::load(path)?)
    }
}

fn parse_checksums(text: &str) -> Result<BTreeMap<String, String>, PackError> {
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| PackError::Invalid(format!("bad {CHECKSUMS_FILE} line {line:?}")))?;
        if !is_sha256_hex(hash) || !is_pack_path(path) {
            return Err(PackError::Invalid(format!(
                "bad {CHECKSUMS_FILE} line {line:?}"
            )));
        }
        if sums.insert(path.to_owned(), hash.to_owned()).is_some() {
            return Err(PackError::Invalid(format!("{path} is listed twice")));
        }
    }
    if !sums.contains_key(MANIFEST_FILE) {
        return Err(PackError::Invalid(format!(
            "{CHECKSUMS_FILE} does not list ptk.json"
        )));
    }
    Ok(sums)
}

/// Removes a half-extracted folder unless disarmed.
struct Staging(Option<PathBuf>);

impl Drop for Staging {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Result of [`unpack`].
#[derive(Debug)]
pub struct UnpackReport {
    pub dest: PathBuf,
    pub manifest: Manifest,
    pub entries: Vec<PackedEntry>,
}

/// Unpacks and checks a `.ptk` file into `dest`, which must not exist or
/// be an empty folder. Every entry must be listed in `SHA256SUMS` with the
/// right hash; unexpected paths, links and oversized packs are refused.
/// Nothing is left behind on failure.
pub fn unpack(pack: &Path, dest: &Path) -> Result<UnpackReport, PackError> {
    if dest.exists() {
        let empty = dest.is_dir() && fs::read_dir(dest).map_err(io_err(dest))?.next().is_none();
        if !empty {
            return Err(PackError::Exists(dest.to_owned()));
        }
    }
    let mut archive = open_archive(pack)?;
    let sums_bytes = read_small(&mut archive, CHECKSUMS_FILE)?
        .ok_or_else(|| PackError::Invalid(format!("no {CHECKSUMS_FILE} in the pack")))?;
    let sums = parse_checksums(&String::from_utf8_lossy(&sums_bytes))?;

    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&parent).map_err(io_err(&parent))?;
    let dest_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let staging_dir = parent.join(format!(".{dest_name}.unpacking-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging_dir);
    fs::create_dir_all(&staging_dir).map_err(io_err(&staging_dir))?;
    let mut staging = Staging(Some(staging_dir.clone()));

    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name()?.into_owned();
        if file.is_dir() {
            continue;
        }
        if name == CHECKSUMS_FILE {
            continue;
        }
        if file.is_symlink() {
            return Err(PackError::Symlink(name));
        }
        if !is_pack_path(&name) {
            return Err(PackError::Invalid(format!("unexpected entry {name:?}")));
        }
        let expected = sums.get(&name).ok_or_else(|| {
            PackError::Invalid(format!("{name} is not listed in {CHECKSUMS_FILE}"))
        })?;
        if !seen.insert(name.clone()) {
            return Err(PackError::Invalid(format!("{name} appears twice")));
        }
        let out_path = staging_dir.join(&name);
        if let Some(dir) = out_path.parent() {
            fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let mut out = BufWriter::new(File::create(&out_path).map_err(io_err(&out_path))?);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut size = 0u64;
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| PackError::Invalid(format!("{name}: {e}")))?;
            if n == 0 {
                break;
            }
            size += n as u64;
            total += n as u64;
            if total > MAX_UNPACKED_BYTES {
                return Err(PackError::Invalid("unpacked size exceeds 4 GiB".into()));
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(io_err(&out_path))?;
        }
        out.flush().map_err(io_err(&out_path))?;
        let actual = hex::encode(hasher.finalize());
        if &actual != expected {
            return Err(PackError::Checksum(name));
        }
        entries.push(PackedEntry {
            path: name,
            size,
            sha256: actual,
        });
    }
    if let Some(missing) = sums.keys().find(|k| !seen.contains(*k)) {
        return Err(PackError::Invalid(format!(
            "{missing} is listed in {CHECKSUMS_FILE} but missing"
        )));
    }
    let manifest = Manifest::load(&staging_dir)?;
    manifest.check(&staging_dir.join(MANIFEST_FILE), &TargetRegistry::builtin())?;
    if staging_dir.join(PLUGINS_LOCK).exists() {
        PluginsLock::load(&staging_dir)?;
    }

    if dest.exists() {
        fs::remove_dir(dest).map_err(io_err(dest))?;
    }
    fs::rename(&staging_dir, dest).map_err(io_err(dest))?;
    staging.0 = None;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(UnpackReport {
        dest: dest.to_owned(),
        manifest,
        entries,
    })
}
