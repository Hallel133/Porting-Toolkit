//! The project manifest, `ptk.json`.
//!
//! The format is documented in `docs/project-format.md`. Rules that keep old
//! and new versions of the tool working together:
//!
//! - `schema_version` is required. A manifest from a newer schema is
//!   refused with a clear message instead of being half-read.
//! - Unknown fields are kept (`extra`) and written back on save, so a newer
//!   tool's data survives an older tool editing the file.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::category::{CATEGORIES, category};
use crate::hash::is_sha256_hex;
use crate::targets::TargetRegistry;

/// Texture formats the GPU decodes directly (as opposed to image files).
pub const GPU_TEXTURES: &[&str] = &["astc", "bc7", "etc2"];

/// File name of the manifest inside a project folder or a `.ptk` pack.
pub const MANIFEST_FILE: &str = "ptk.json";

/// Current manifest schema.
pub const SCHEMA_VERSION: u32 = 1;

/// `game.content_license` value for a game the project's author made.
pub const OWN_WORK_LICENSE: &str = "LicenseRef-Own-Work";

/// Licenses that allow game content to be redistributed inside a pack.
/// Anything else (including no license, or `proprietary`) is treated as
/// third-party content that must never be packed.
pub const REDISTRIBUTABLE_LICENSES: &[&str] = &[
    OWN_WORK_LICENSE,
    "0BSD",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
    "CC0-1.0",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "ISC",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "MIT",
    "MPL-2.0",
    "OFL-1.1",
    "Unlicense",
    "Zlib",
];

/// True if an SPDX-style expression only uses redistributable licenses.
/// `A OR B` needs one of them, `A AND B` needs both.
pub fn license_allows_redistribution(expr: &str) -> bool {
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')');
    if expr.is_empty() {
        return false;
    }
    expr.split(" OR ").any(|alt| {
        alt.split(" AND ").all(|id| {
            let id = id.trim().trim_start_matches('(').trim_end_matches(')');
            REDISTRIBUTABLE_LICENSES.contains(&id)
        })
    })
}

/// The `ptk.json` manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// License of the community work in the project (recipes, patches).
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default)]
    pub game: GameInfo,
    /// Target ids from the [`TargetRegistry`].
    #[serde(default)]
    pub targets: Vec<String>,
    /// Advanced options, per asset category id.
    #[serde(default)]
    pub options: BTreeMap<String, CategoryOptions>,
    /// Globs of project files that are third-party content.
    #[serde(default)]
    pub third_party: Vec<String>,
    /// Hashes of the user's original game files (never the files).
    #[serde(default)]
    pub source_files: Vec<SourceFile>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_license() -> String {
    "MIT".into()
}

/// What the project ports.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Engine id detected by `ptk scan` (`unity`, `unreal`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_platform: Option<String>,
    /// License of the game's own files. Unless it is redistributable (an
    /// open-source game, or [`OWN_WORK_LICENSE`]), `ptk pack` refuses to
    /// include any of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_license: Option<String>,
    /// Icon chosen for the port, relative to the project (`art/icon.png`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Poster (cover art) for the port, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// What to do with a category of assets when porting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetAction {
    #[default]
    Keep,
    Drop,
    Recompress,
}

impl fmt::Display for AssetAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AssetAction::Keep => "keep",
            AssetAction::Drop => "drop",
            AssetAction::Recompress => "recompress",
        })
    }
}

impl std::str::FromStr for AssetAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "keep" => Ok(AssetAction::Keep),
            "drop" => Ok(AssetAction::Drop),
            "recompress" => Ok(AssetAction::Recompress),
            _ => Err(format!("unknown action {s:?} (keep, drop, recompress)")),
        }
    }
}

/// Advanced options for one asset category.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryOptions {
    #[serde(default)]
    pub action: AssetAction,
    /// 1-100, for lossy recompression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
    /// Longest edge in pixels, for textures and video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_resolution: Option<u32>,
    /// Preferred output format, e.g. `astc`, `opus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// SHA-256 of one of the user's original game files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Errors when loading or saving a manifest.
#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: not valid JSON: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}: missing \"schema_version\"; is this a Porting Toolkit manifest?")]
    MissingSchema(PathBuf),
    #[error(
        "{path}: schema_version {found} is newer than this ptk understands ({SCHEMA_VERSION}); update Porting Toolkit"
    )]
    TooNew { path: PathBuf, found: u64 },
    #[error("{path}: invalid manifest:\n{}", .issues.iter().map(|i| format!("  - {i}")).collect::<Vec<_>>().join("\n"))]
    Invalid { path: PathBuf, issues: Vec<Issue> },
}

/// Severity of a validation finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Warning,
    Error,
}

/// One validation finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub level: Level,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.level {
            Level::Warning => "warning",
            Level::Error => "error",
        };
        write!(f, "{level}: {}", self.message)
    }
}

/// True for a safe project-relative path: `/` separators, no `..`, `.`,
/// empty or absolute parts, no backslashes or drive letters.
pub fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// True for names like `my-port` or `gta_v-mac`.
pub fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
}

/// Turns any folder name into a valid project name.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out
        .trim_end_matches('-')
        .chars()
        .take(64)
        .collect::<String>();
    if out.is_empty() {
        "project".into()
    } else {
        out
    }
}

impl Manifest {
    /// A new manifest with every category set to `keep`.
    pub fn new(name: &str) -> Self {
        Manifest {
            schema_version: SCHEMA_VERSION,
            name: name.into(),
            version: "0.1.0".into(),
            description: String::new(),
            authors: Vec::new(),
            license: default_license(),
            game: GameInfo::default(),
            targets: Vec::new(),
            options: Self::default_options(),
            third_party: Vec::new(),
            source_files: Vec::new(),
            extra: BTreeMap::new(),
        }
    }

    /// `keep` for every category.
    pub fn default_options() -> BTreeMap<String, CategoryOptions> {
        CATEGORIES
            .iter()
            .map(|c| (c.id.to_owned(), CategoryOptions::default()))
            .collect()
    }

    /// Effective options for a category (missing means `keep`).
    pub fn options_for(&self, category: &str) -> CategoryOptions {
        self.options.get(category).cloned().unwrap_or_default()
    }

    /// Path of the manifest for a project folder or a manifest file path.
    pub fn path_in(project: &Path) -> PathBuf {
        if project.is_dir() {
            project.join(MANIFEST_FILE)
        } else {
            project.to_owned()
        }
    }

    /// Parses manifest JSON, checking the schema version first.
    pub fn from_slice(bytes: &[u8], path: &Path) -> Result<Self, ManifestError> {
        let json = |source| ManifestError::Json {
            path: path.to_owned(),
            source,
        };
        let value: Value = serde_json::from_slice(bytes).map_err(json)?;
        match value.get("schema_version").and_then(Value::as_u64) {
            None => return Err(ManifestError::MissingSchema(path.to_owned())),
            Some(v) if v > u64::from(SCHEMA_VERSION) => {
                return Err(ManifestError::TooNew {
                    path: path.to_owned(),
                    found: v,
                });
            }
            Some(_) => {}
        }
        serde_json::from_value(value).map_err(json)
    }

    /// Loads `ptk.json` from a project folder (or a direct file path).
    pub fn load(project: &Path) -> Result<Self, ManifestError> {
        let path = Self::path_in(project);
        let bytes = std::fs::read(&path).map_err(|source| ManifestError::Io {
            path: path.clone(),
            source,
        })?;
        Self::from_slice(&bytes, &path)
    }

    /// Pretty JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("manifest serializes");
        s.push('\n');
        s
    }

    /// Writes `ptk.json` into a project folder (or to a direct file path),
    /// through a temporary file so a crash never leaves half a manifest.
    pub fn save(&self, project: &Path) -> Result<PathBuf, ManifestError> {
        let path = Self::path_in(project);
        let io = |source| ManifestError::Io {
            path: path.clone(),
            source,
        };
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json()).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(io)?;
        Ok(path)
    }

    /// True if the game's own content may be redistributed in a pack.
    pub fn allows_redistribution(&self) -> bool {
        self.game
            .content_license
            .as_deref()
            .is_some_and(license_allows_redistribution)
    }

    /// Checks the manifest. Errors block packing; warnings are shown.
    pub fn validate(&self, targets: &TargetRegistry) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut error = |m: String| {
            issues.push(Issue {
                level: Level::Error,
                message: m,
            })
        };
        if !valid_project_name(&self.name) {
            error(format!(
                "name {:?} must be 1-64 letters, digits, '-', '_' or '.'",
                self.name
            ));
        }
        if self.version.trim().is_empty() {
            error("version must not be empty".into());
        }
        if self.license.trim().is_empty() {
            error("license must not be empty (it covers the community work)".into());
        }
        for (cat, opts) in &self.options {
            if let Some(q) = opts.quality
                && !(1..=100).contains(&q)
            {
                error(format!("options.{cat}.quality must be 1-100, not {q}"));
            }
            if let Some(r) = opts.max_resolution
                && !(16..=16384).contains(&r)
            {
                error(format!(
                    "options.{cat}.max_resolution must be 16-16384, not {r}"
                ));
            }
            if let Some(f) = &opts.format
                && category(cat).is_some()
                && !crate::category::is_format(cat, f)
            {
                let known: Vec<&str> = crate::category::formats(cat)
                    .iter()
                    .map(|(id, _)| *id)
                    .collect();
                error(if known.is_empty() {
                    format!("options.{cat}.format: {} has no formats to pick", cat)
                } else {
                    format!(
                        "options.{cat}.format must be one of {}, not {f:?}",
                        known.join(", ")
                    )
                });
            }
        }
        for (kind, art) in [("icon", &self.game.icon), ("poster", &self.game.poster)] {
            if let Some(a) = art
                && !(is_safe_relative_path(a) && a.starts_with("art/"))
            {
                error(format!("game.{kind} must be a file in art/, not {a:?}"));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for f in &self.source_files {
            if !is_safe_relative_path(&f.path) {
                error(format!("source_files: unsafe path {:?}", f.path));
            }
            if !is_sha256_hex(&f.sha256) {
                error(format!("source_files: bad sha256 for {:?}", f.path));
            }
            if !seen.insert(f.path.as_str()) {
                error(format!("source_files: duplicate path {:?}", f.path));
            }
        }

        let mut warning = |m: String| {
            issues.push(Issue {
                level: Level::Warning,
                message: m,
            })
        };
        for t in &self.targets {
            if targets.get(t).is_none() {
                warning(format!("unknown target {t:?} (see `ptk targets`)"));
            }
        }
        for (cat, opts) in &self.options {
            match category(cat) {
                None => warning(format!("options: unknown asset category {cat:?}")),
                Some(c)
                    if !c.lossy && (opts.quality.is_some() || opts.max_resolution.is_some()) =>
                {
                    warning(format!(
                        "options.{cat}: quality/max_resolution have no effect on {}",
                        c.name.to_lowercase()
                    ))
                }
                Some(_) => {}
            }
            // GPU texture formats only work where the GPU decodes them.
            if let Some(f) = opts.format.as_deref().filter(|f| GPU_TEXTURES.contains(f))
                && cat == "textures"
            {
                let known: Vec<_> = self.targets.iter().filter_map(|t| targets.get(t)).collect();
                if !known.is_empty()
                    && !known
                        .iter()
                        .any(|t| t.media.textures.iter().any(|x| x == f))
                {
                    warning(format!(
                        "options.textures.format {f:?} is not supported by any of this project's targets; \
                         \"Automatic\" picks one that is"
                    ));
                }
            }
        }
        issues
    }

    /// [`Manifest::validate`], turned into an error if anything is an error.
    pub fn check(
        &self,
        path: &Path,
        targets: &TargetRegistry,
    ) -> Result<Vec<Issue>, ManifestError> {
        let issues = self.validate(targets);
        if issues.iter().any(|i| i.level == Level::Error) {
            return Err(ManifestError::Invalid {
                path: path.to_owned(),
                issues: issues
                    .into_iter()
                    .filter(|i| i.level == Level::Error)
                    .collect(),
            });
        }
        Ok(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        let mut m = Manifest::new("demo-port");
        m.description = "A demo".into();
        m.authors = vec!["Hallel133".into()];
        m.game.title = Some("Demo Game".into());
        m.game.engine = Some("unity".into());
        m.targets = vec!["macos-arm64".into()];
        m.options.insert(
            "textures".into(),
            CategoryOptions {
                action: AssetAction::Recompress,
                quality: Some(80),
                max_resolution: Some(2048),
                format: Some("astc".into()),
                extra: BTreeMap::new(),
            },
        );
        m.source_files.push(SourceFile {
            path: "Demo_Data/globalgamemanagers".into(),
            size: 3,
            sha256: crate::hash::sha256_bytes(b"abc"),
        });
        m
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let back = Manifest::from_slice(m.to_json().as_bytes(), Path::new("ptk.json")).unwrap();
        assert_eq!(m, back);
        assert!(m.validate(&TargetRegistry::builtin()).is_empty());
    }

    #[test]
    fn round_trip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let m = sample();
        m.save(dir.path()).unwrap();
        assert_eq!(Manifest::load(dir.path()).unwrap(), m);
    }

    #[test]
    fn unknown_fields_survive() {
        let json = r#"{
            "schema_version": 1, "name": "x", "version": "1.0.0",
            "future_field": {"a": 1},
            "game": {"title": "T", "future_game_field": true},
            "options": {"textures": {"action": "drop", "future_opt": 2}}
        }"#;
        let m = Manifest::from_slice(json.as_bytes(), Path::new("ptk.json")).unwrap();
        let out: Value = serde_json::from_str(&m.to_json()).unwrap();
        assert_eq!(out["future_field"]["a"], 1);
        assert_eq!(out["game"]["future_game_field"], true);
        assert_eq!(out["options"]["textures"]["future_opt"], 2);
        assert_eq!(out["options"]["textures"]["action"], "drop");
    }

    #[test]
    fn schema_version_is_required_and_bounded() {
        let p = Path::new("ptk.json");
        assert!(matches!(
            Manifest::from_slice(br#"{"name":"x","version":"1"}"#, p),
            Err(ManifestError::MissingSchema(_))
        ));
        assert!(matches!(
            Manifest::from_slice(br#"{"schema_version":99,"name":"x","version":"1"}"#, p),
            Err(ManifestError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn validation() {
        let reg = TargetRegistry::builtin();
        let mut m = sample();
        m.targets.push("nova-x".into());
        m.options.get_mut("textures").unwrap().quality = Some(0);
        m.options.insert(
            "scripts".into(),
            CategoryOptions {
                quality: Some(50),
                ..Default::default()
            },
        );
        m.source_files.push(SourceFile {
            path: "../escape".into(),
            size: 0,
            sha256: "nope".into(),
        });
        let issues = m.validate(&reg);
        let errors: Vec<_> = issues.iter().filter(|i| i.level == Level::Error).collect();
        let warnings: Vec<_> = issues
            .iter()
            .filter(|i| i.level == Level::Warning)
            .collect();
        assert_eq!(errors.len(), 3, "{issues:?}");
        assert_eq!(warnings.len(), 2, "{issues:?}");
        assert!(m.check(Path::new("ptk.json"), &reg).is_err());
    }

    #[test]
    fn format_validation() {
        let reg = TargetRegistry::builtin();
        let mut m = sample();
        assert!(m.validate(&reg).is_empty(), "{:?}", m.validate(&reg));
        // ASTC is fine on Apple Silicon, not on an Intel-only project.
        m.targets = vec!["macos-x64".into()];
        let issues = m.validate(&reg);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].level, Level::Warning);
        m.options.get_mut("textures").unwrap().format = Some("jpeg2000".into());
        let issues = m.validate(&reg);
        assert!(
            issues
                .iter()
                .any(|i| i.level == Level::Error && i.message.contains("astc, bc7")),
            "{issues:?}"
        );
        m.options.get_mut("textures").unwrap().format = Some("bc7".into());
        assert!(m.validate(&reg).is_empty());
    }

    #[test]
    fn licenses() {
        assert!(license_allows_redistribution("MIT"));
        assert!(license_allows_redistribution("MIT OR Apache-2.0"));
        assert!(license_allows_redistribution(OWN_WORK_LICENSE));
        assert!(!license_allows_redistribution("proprietary"));
        assert!(!license_allows_redistribution(
            "MIT AND LicenseRef-Proprietary"
        ));
        assert!(!license_allows_redistribution(""));
        let mut m = sample();
        assert!(!m.allows_redistribution());
        m.game.content_license = Some("GPL-3.0-or-later".into());
        assert!(m.allows_redistribution());
    }

    #[test]
    fn names() {
        assert_eq!(slugify("My Great Game!"), "my-great-game");
        assert_eq!(slugify("***"), "project");
        assert!(valid_project_name("demo"));
        assert!(!valid_project_name("-demo"));
        assert!(is_safe_relative_path("a/b.txt"));
        assert!(!is_safe_relative_path("a//b"));
        assert!(!is_safe_relative_path("C:/x"));
    }
}
