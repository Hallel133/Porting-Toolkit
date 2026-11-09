//! Plugin operations shared by the CLI and GUI.
//!
//! Validation and templates live in the Python engine (it owns plugins);
//! installing is a plain copy into the data folder. Phase 1/2 never run
//! plugin code.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::EngineClient;
use crate::error::{Error, Result, io_ctx};
use crate::hash::is_junk_file;
use crate::paths;
use crate::targets::TargetRegistry;

/// Result of `plugin.validate`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Validation {
    pub path: String,
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub plugin: Option<PluginInfo>,
    pub permissions: Vec<String>,
    /// `low`, `medium`, `high` or `unknown`.
    pub risk: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub license: Option<String>,
}

/// One entry of `plugin.list`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListedPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub path: String,
    pub valid: bool,
    pub risk: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub reviewed: bool,
}

/// Result of `plugin.new`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatedPlugin {
    pub path: String,
    pub id: String,
    pub files: Vec<String>,
}

fn absolute(p: &Path) -> Result<PathBuf> {
    if p.is_absolute() {
        Ok(p.to_owned())
    } else {
        Ok(std::env::current_dir()
            .map_err(io_ctx("current folder"))?
            .join(p))
    }
}

fn known_targets() -> Vec<String> {
    TargetRegistry::load_default().0.ids()
}

fn decode<T: serde::de::DeserializeOwned>(raw: Value, what: &str) -> Result<T> {
    serde_json::from_value(raw).map_err(|e| Error::Invalid(format!("unexpected {what}: {e}")))
}

/// Validates a plugin folder (no plugin code runs).
pub fn validate(engine: &mut EngineClient, dir: &Path) -> Result<(Validation, Value)> {
    let raw = engine.call(
        "plugin.validate",
        json!({ "path": absolute(dir)?.to_string_lossy(), "known_targets": known_targets() }),
    )?;
    Ok((decode(raw.clone(), "validation result")?, raw))
}

/// Where `list` looks: installed plugins, plus `./plugins` if present.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![paths::plugins_dir()];
    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join("plugins");
        if local.is_dir() {
            dirs.push(local);
        }
    }
    dirs
}

/// Lists plugins in `dirs`.
pub fn list(engine: &mut EngineClient, dirs: &[PathBuf]) -> Result<(Vec<ListedPlugin>, Value)> {
    let raw = engine.call(
        "plugin.list",
        json!({
            "dirs": dirs.iter().map(|d| d.to_string_lossy()).collect::<Vec<_>>(),
            "known_targets": known_targets(),
        }),
    )?;
    Ok((decode(raw.clone(), "plugin list")?, raw))
}

/// Creates a plugin skeleton (`kind` is `import` or `export`).
pub fn create(
    engine: &mut EngineClient,
    dir: &Path,
    kind: &str,
    id: Option<&str>,
) -> Result<CreatedPlugin> {
    let raw = engine.call(
        "plugin.new",
        json!({ "path": absolute(dir)?.to_string_lossy(), "kind": kind, "id": id }),
    )?;
    decode(raw, "plugin.new result")
}

fn copy_tree(src: &Path, dst: &Path) -> Result<usize> {
    fs::create_dir_all(dst).map_err(io_ctx(dst.display().to_string()))?;
    let mut n = 0;
    for entry in fs::read_dir(src).map_err(io_ctx(src.display().to_string()))? {
        let entry = entry.map_err(io_ctx(src.display().to_string()))?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if matches!(
            name_s.as_ref(),
            ".git" | "__pycache__" | ".venv" | ".pytest_cache"
        ) || is_junk_file(&name_s)
        {
            continue;
        }
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(io_ctx(path.display().to_string()))?;
        if ft.is_symlink() {
            return Err(Error::Invalid(format!(
                "{} is a symbolic link; plugins must not contain links",
                path.display()
            )));
        } else if ft.is_dir() {
            n += copy_tree(&path, &dst.join(&name))?;
        } else if ft.is_file() {
            fs::copy(&path, dst.join(&name)).map_err(io_ctx(path.display().to_string()))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Copies a validated plugin folder to `<data>/plugins/<id>`, replacing an
/// installed copy. Returns the destination and the number of files.
/// Callers must show the permissions and get consent first.
pub fn install(dir: &Path, validation: &Validation) -> Result<(PathBuf, usize)> {
    if !validation.valid {
        return Err(Error::Invalid("not installing an invalid plugin".into()));
    }
    let id = validation
        .plugin
        .as_ref()
        .and_then(|p| p.id.clone())
        .ok_or_else(|| Error::Invalid("plugin has no id".into()))?;
    let root = paths::plugins_dir();
    let dest = root.join(&id);
    let staging = root.join(format!(".{id}.installing"));
    let _ = fs::remove_dir_all(&staging);
    let n = match copy_tree(&absolute(dir)?, &staging) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(io_ctx(dest.display().to_string()))?;
    }
    fs::rename(&staging, &dest).map_err(io_ctx(dest.display().to_string()))?;
    Ok((dest, n))
}
