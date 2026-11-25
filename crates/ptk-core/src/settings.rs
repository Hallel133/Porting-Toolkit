//! App-wide defaults, used when a project is created.
//!
//! Stored in `<data>/settings.json`. Each project keeps its own copy of the
//! advanced options, so changing a default never changes existing projects.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Result, io_ctx};
use crate::manifest::CategoryOptions;
use crate::paths;

pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    /// Targets of new projects (empty: this computer).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_targets: Vec<String>,
    /// Advanced options of new projects, per asset category; the `format`
    /// of each category is the default algorithm (e.g. ASTC for textures).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_options: BTreeMap<String, CategoryOptions>,
    /// Where new projects go (default: `~/Porting Toolkit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_dir: Option<PathBuf>,
    /// The plugin copy (folder) chosen for an id when several versions are
    /// present; otherwise the newest runs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_versions: BTreeMap<String, PathBuf>,
    /// Unknown fields from newer versions survive a save.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

pub fn path() -> PathBuf {
    paths::data_dir().join(SETTINGS_FILE)
}

impl AppSettings {
    /// The saved settings (defaults when missing or unreadable).
    pub fn load() -> Self {
        fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let mut out = self.clone();
        out.default_options = self.new_project_options();
        let p = path();
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).map_err(io_ctx(dir.display().to_string()))?;
        }
        let text = serde_json::to_string_pretty(&out).expect("settings serialize") + "\n";
        fs::write(&p, text).map_err(io_ctx(p.display().to_string()))
    }

    /// Folder for new projects.
    pub fn projects_dir(&self) -> PathBuf {
        self.projects_dir.clone().unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Porting Toolkit")
        })
    }

    /// Options for a new project: these defaults (only categories that
    /// differ from "keep").
    pub fn new_project_options(&self) -> BTreeMap<String, CategoryOptions> {
        self.default_options
            .iter()
            .filter(|(_, o)| **o != CategoryOptions::default())
            .map(|(k, o)| (k.clone(), o.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_survive_and_keep_is_dropped() {
        let s: AppSettings =
            serde_json::from_str(r#"{"default_targets":["web"],"future":1,"default_options":{"textures":{"action":"recompress","format":"astc"},"audio":{"action":"keep"}}}"#)
                .unwrap();
        assert_eq!(s.default_targets, vec!["web"]);
        assert_eq!(s.extra["future"], 1);
        let opts = s.new_project_options();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts["textures"].format.as_deref(), Some("astc"));
        let back = serde_json::to_value(&s).unwrap();
        assert_eq!(back["future"], 1);
    }
}
