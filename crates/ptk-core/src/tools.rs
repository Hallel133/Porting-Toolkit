//! The compression tools the asset pipeline runs (FFmpeg, cwebp, oxipng, ...).
//!
//! The engine finds them (`tools.status`): an installed copy first, then a
//! pinned download it made before. `tools.install` downloads the pinned
//! build of a tool and checks its SHA-256. See `engine/ptk_engine/tools.py`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::engine::EngineClient;
use crate::error::Result;
use crate::paths;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FoundTool {
    pub id: String,
    pub path: String,
    /// `setting`, `system` or `downloaded`.
    pub source: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub license: String,
    #[serde(default)]
    pub found: Option<FoundTool>,
    /// A pinned download exists for this computer.
    #[serde(default)]
    pub installable: bool,
    /// How to get it (`brew install ffmpeg`, `ptk tools install oxipng`).
    #[serde(default)]
    pub hint: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolsStatus {
    pub platform: String,
    pub tools: Vec<ToolStatus>,
}

/// Where downloaded tools live: `<data>/cache/tools`.
pub fn tools_dir() -> PathBuf {
    paths::data_dir().join("cache").join("tools")
}

pub fn status(engine: &mut EngineClient) -> Result<ToolsStatus> {
    Ok(engine.call_as("tools.status", json!({"tools_dir": tools_dir()}))?)
}

pub fn install(engine: &mut EngineClient, id: &str, dir: &Path) -> Result<FoundTool> {
    Ok(engine.call_as("tools.install", json!({"id": id, "tools_dir": dir}))?)
}
