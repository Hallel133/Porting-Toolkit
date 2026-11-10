//! Registry of platforms a project can be ported to.
//!
//! Built-in targets cover the roadmap (macOS first, then Windows/Linux and
//! the web). The registry is open: extra targets are loaded from `*.toml`
//! files in [`crate::paths::targets_dir`], which is how an SDK (for example
//! the NoVA SDK, adding `nova-*` targets) plugs in its own platforms and
//! names the export plugin that builds for them:
//!
//! ```toml
//! [[target]]
//! id = "nova-example"
//! name = "NoVA example board"
//! os = "nova"
//! arch = "riscv64"
//! status = "experimental"
//! export_backend = "nova-export"
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// How far along support for a target is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetStatus {
    /// In focus now (macOS in phase 1).
    Active,
    /// Provided by an external definition, may change.
    Experimental,
    /// On the roadmap, not usable yet.
    Planned,
}

impl fmt::Display for TargetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TargetStatus::Active => "active",
            TargetStatus::Experimental => "experimental",
            TargetStatus::Planned => "planned",
        })
    }
}

/// Where a target definition came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "path")]
pub enum TargetSource {
    #[default]
    Builtin,
    File(PathBuf),
}

/// One platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    pub status: TargetStatus,
    #[serde(default)]
    pub description: String,
    /// Plugin id of the export plugin that builds for this target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_backend: Option<String>,
    #[serde(skip_deserializing, default)]
    pub source: TargetSource,
}

/// Errors when registering or loading targets.
#[derive(Debug, Error)]
pub enum TargetError {
    #[error("invalid target id {0:?}: use lower-case letters, digits and single dashes")]
    InvalidId(String),
    #[error("target {0:?} is already registered")]
    Duplicate(String),
    #[error("{path}: {message}")]
    File { path: PathBuf, message: String },
}

/// True for ids like `macos-arm64` or `nova-example`.
pub fn valid_target_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// The set of known targets.
#[derive(Clone, Debug)]
pub struct TargetRegistry {
    targets: Vec<Target>,
}

fn builtin(id: &str, name: &str, os: &str, arch: &str, status: TargetStatus, desc: &str) -> Target {
    Target {
        id: id.into(),
        name: name.into(),
        os: os.into(),
        arch: arch.into(),
        status,
        description: desc.into(),
        export_backend: None,
        source: TargetSource::Builtin,
    }
}

impl TargetRegistry {
    /// The built-in targets only.
    pub fn builtin() -> Self {
        use TargetStatus::*;
        TargetRegistry {
            targets: vec![
                builtin(
                    "macos-arm64",
                    "macOS (Apple Silicon)",
                    "macos",
                    "arm64",
                    Active,
                    "Apple M-series Macs.",
                ),
                builtin(
                    "macos-x64",
                    "macOS (Intel)",
                    "macos",
                    "x86_64",
                    Active,
                    "Intel Macs.",
                ),
                builtin(
                    "web",
                    "Web (HTML5 / WebAssembly)",
                    "web",
                    "wasm32",
                    Active,
                    "Runs in a browser; upload the folder to any static host.",
                ),
                builtin(
                    "windows-x64",
                    "Windows x64",
                    "windows",
                    "x86_64",
                    Planned,
                    "Phase 4.",
                ),
                builtin(
                    "linux-x64",
                    "Linux x64",
                    "linux",
                    "x86_64",
                    Planned,
                    "Phase 4.",
                ),
            ],
        }
    }

    /// Built-in targets plus every `*.toml` definition in `dir`. Files that
    /// fail to load are reported and skipped, so one bad file cannot break
    /// the tool.
    pub fn load(dir: &Path) -> (Self, Vec<TargetError>) {
        let mut registry = Self::builtin();
        let mut errors = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (registry, errors);
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for path in files {
            if let Err(e) = registry.load_file(&path) {
                errors.push(e);
            }
        }
        (registry, errors)
    }

    /// Built-in targets plus the user's target definitions.
    pub fn load_default() -> (Self, Vec<TargetError>) {
        Self::load(&crate::paths::targets_dir())
    }

    /// Loads a `[[target]]` definition file.
    pub fn load_file(&mut self, path: &Path) -> Result<(), TargetError> {
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            target: Vec<Target>,
        }
        let err = |message: String| TargetError::File {
            path: path.to_owned(),
            message,
        };
        let text = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
        let file: File = toml::from_str(&text).map_err(|e| err(e.to_string()))?;
        for mut target in file.target {
            target.source = TargetSource::File(path.to_owned());
            self.register(target).map_err(|e| err(e.to_string()))?;
        }
        Ok(())
    }

    /// Adds a target. Ids must be valid and unique.
    pub fn register(&mut self, target: Target) -> Result<(), TargetError> {
        if !valid_target_id(&target.id) {
            return Err(TargetError::InvalidId(target.id));
        }
        if self.get(&target.id).is_some() {
            return Err(TargetError::Duplicate(target.id));
        }
        self.targets.push(target);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Target> {
        self.targets.iter().find(|t| t.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Target> {
        self.targets.iter()
    }

    pub fn ids(&self) -> Vec<String> {
        self.targets.iter().map(|t| t.id.clone()).collect()
    }

    /// The target matching the machine `ptk` runs on, if it is built in.
    pub fn host(&self) -> Option<&Target> {
        let id = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", "x86_64") => "macos-x64",
            ("windows", "x86_64") => "windows-x64",
            ("linux", "x86_64") => "linux-x64",
            _ => return None,
        };
        self.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_and_unique() {
        let reg = TargetRegistry::builtin();
        let ids = reg.ids();
        assert_eq!(
            ids,
            [
                "macos-arm64",
                "macos-x64",
                "web",
                "windows-x64",
                "linux-x64"
            ]
        );
        assert!(ids.iter().all(|id| valid_target_id(id)));
    }

    #[test]
    fn ids() {
        assert!(valid_target_id("nova-v1"));
        assert!(!valid_target_id("Nova"));
        assert!(!valid_target_id("nova--x"));
        assert!(!valid_target_id("-nova"));
        assert!(!valid_target_id(""));
    }

    #[test]
    fn external_definitions_extend_the_registry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("nova.toml"),
            r#"
[[target]]
id = "nova-example"
name = "NoVA example board"
os = "nova"
arch = "riscv64"
status = "experimental"
export_backend = "nova-export"
"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("bad.toml"), "[[target]]\nid = \"Bad\"\n").unwrap();
        let (reg, errors) = TargetRegistry::load(dir.path());
        let nova = reg.get("nova-example").unwrap();
        assert_eq!(nova.status, TargetStatus::Experimental);
        assert_eq!(nova.export_backend.as_deref(), Some("nova-export"));
        assert!(matches!(nova.source, TargetSource::File(_)));
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn duplicates_are_rejected() {
        let mut reg = TargetRegistry::builtin();
        let dup = reg.get("web").unwrap().clone();
        assert!(matches!(reg.register(dup), Err(TargetError::Duplicate(_))));
    }
}
