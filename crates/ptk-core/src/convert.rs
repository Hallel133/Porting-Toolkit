//! Running import and export plugins on a project.
//!
//! The engine finds a plugin for the project's engine (and, when
//! exporting, the target) and runs it in its own process. Built-in plugins
//! ship in `engine/plugins`; installed plugins with the same id replace
//! them. Each run is recorded in the project's `plugins.lock`.
//!
//! The game itself never enters the project: plugins read it from where it
//! is (remembered per machine in `.ptk-local.json`) and write their result
//! to the output folder you choose.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::EngineClient;
use crate::error::{Error, Result, io_ctx};
use crate::manifest::Manifest;
use crate::pack::{LockedPlugin, PluginsLock};
use crate::paths;
use crate::plugins::Validation;
use crate::targets::{Target, TargetRegistry};

/// Per-machine project state (never packed): where the game is.
pub const LOCAL_FILE: &str = ".ptk-local.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<PathBuf>,
    /// The build you imported (a .zip, .dmg, ...), if it was unpacked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,
    /// The game's own icon, found by the import plugin (a cached copy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_icon: Option<PathBuf>,
}

impl LocalState {
    pub fn load(project: &Path) -> Self {
        fs::read_to_string(project.join(LOCAL_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, project: &Path) -> Result<()> {
        let path = project.join(LOCAL_FILE);
        let text = serde_json::to_string_pretty(self).expect("state serializes") + "\n";
        fs::write(&path, text).map_err(io_ctx(path.display().to_string()))
    }

    /// Remembers the game folder, keeping the other fields.
    pub fn remember_game(project: &Path, game: &Path, source: Option<&Path>) -> Result<()> {
        let mut s = Self::load(project);
        s.game = Some(game.canonicalize().unwrap_or_else(|_| game.to_owned()));
        if let Some(src) = source {
            s.source = Some(src.canonicalize().unwrap_or_else(|_| src.to_owned()));
        }
        s.save(project)
    }
}

/// A plugin the engine can run for a job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    #[serde(flatten)]
    pub validation: Validation,
    #[serde(default)]
    pub builtin: bool,
}

impl Candidate {
    pub fn id(&self) -> &str {
        self.validation
            .plugin
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .unwrap_or("?")
    }

    pub fn name(&self) -> &str {
        self.validation
            .plugin
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("?")
    }
}

/// Something a running plugin reported.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Progress {
        fraction: Option<f64>,
        message: String,
    },
    Log {
        warning: bool,
        message: String,
    },
}

/// What a plugin run produced.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    #[serde(default)]
    pub plugin_id: String,
    #[serde(default)]
    pub plugin_name: String,
    #[serde(default)]
    pub plugin_version: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub outputs: Vec<PathBuf>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// The main thing to open (an .app or index.html).
    #[serde(default)]
    pub open: Option<PathBuf>,
    /// A folder to serve over HTTP (web builds).
    #[serde(default)]
    pub serve: Option<PathBuf>,
    /// The target, for exports.
    #[serde(default)]
    pub target: Option<String>,
    /// The game's real title, when an import plugin found it.
    #[serde(default)]
    pub title: Option<String>,
    /// A cached copy of the game's own icon (import plugins).
    #[serde(default)]
    pub icon: Option<PathBuf>,
}

fn search_dirs() -> Vec<String> {
    crate::plugins::default_dirs()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect()
}

/// Plugins that can do `kind` (`import` or `export`) for an engine and
/// target, best first.
pub fn candidates(
    engine: &mut EngineClient,
    kind: &str,
    engine_id: Option<&str>,
    target: Option<&Target>,
) -> Result<Vec<Candidate>> {
    let registry = TargetRegistry::load_default().0;
    let raw = engine.call(
        "plugin.find",
        json!({
            "kind": kind,
            "engine": engine_id,
            "target": target.map(|t| t.id.clone()),
            "dirs": search_dirs(),
            "known_targets": registry.ids(),
            "prefer": target.and_then(|t| t.export_backend.clone()),
        }),
    )?;
    serde_json::from_value(raw).map_err(|e| Error::Invalid(format!("unexpected plugin list: {e}")))
}

/// Options for [`export`].
#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    /// Game folder; defaults to the one remembered for the project.
    pub game: Option<PathBuf>,
    /// Output folder; the build goes into `<output>/<target id>`.
    pub output: Option<PathBuf>,
    /// A specific plugin id instead of the best match.
    pub plugin: Option<String>,
    /// `--set key=value` settings passed to the plugin.
    pub settings: BTreeMap<String, String>,
}

/// The game folder for a project: the given one or the remembered one.
pub fn game_for(project: &Path, game: Option<&Path>) -> Result<PathBuf> {
    let game = match game {
        Some(g) => g.to_owned(),
        None => LocalState::load(project).game.ok_or_else(|| {
            Error::Invalid(
                "this project does not know where the game is: import or scan it first, or pass --game".into(),
            )
        })?,
    };
    if !game.exists() {
        return Err(Error::Invalid(format!(
            "the game is not at {} any more (moved or on a drive that is not connected?)",
            game.display()
        )));
    }
    Ok(game)
}

/// Default output folder: `<project>/build`.
pub fn default_output(project: &Path) -> PathBuf {
    LocalState::load(project)
        .output
        .unwrap_or_else(|| absolute(&project.join("build")))
}

fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_owned()
    } else {
        std::env::current_dir()
            .map(|d| d.join(p))
            .unwrap_or_else(|_| p.to_owned())
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    engine: &mut EngineClient,
    candidate: &Candidate,
    kind: &str,
    manifest: &Manifest,
    folders: Value,
    target: Option<&Target>,
    settings: &BTreeMap<String, String>,
    on_event: &mut dyn FnMut(Event),
) -> Result<RunResult> {
    let registry = TargetRegistry::load_default().0;
    let raw = engine.call_with(
        "plugin.run",
        json!({
            "plugin": candidate.validation.path,
            "kind": kind,
            "engine": manifest.game.engine,
            "target": target.map(|t| json!({"id": t.id, "name": t.name, "os": t.os, "arch": t.arch})),
            "manifest": serde_json::to_value(manifest).expect("manifest serializes"),
            "settings": settings,
            "folders": folders,
            "cache_root": paths::data_dir().join("cache"),
            "known_targets": registry.ids(),
        }),
        &mut |method, params| match method {
            "progress" => on_event(Event::Progress {
                fraction: params.get("fraction").and_then(Value::as_f64),
                message: params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }),
            "log" => on_event(Event::Log {
                warning: params.get("level").and_then(Value::as_str) == Some("warning"),
                message: params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }),
            _ => {}
        },
    )?;
    let plugin = &raw["plugin"];
    let mut result: RunResult = serde_json::from_value(raw["result"].clone())
        .map_err(|e| Error::Invalid(format!("unexpected plugin result: {e}")))?;
    result.plugin_id = plugin["id"].as_str().unwrap_or_default().to_owned();
    result.plugin_name = plugin["name"].as_str().unwrap_or_default().to_owned();
    result.plugin_version = plugin["version"].as_str().unwrap_or_default().to_owned();
    result.builtin = plugin["builtin"].as_bool().unwrap_or(false);
    result.target = target.map(|t| t.id.clone());
    Ok(result)
}

/// Records the plugin version in `plugins.lock` (replacing an older entry).
pub fn lock_plugin(project: &Path, result: &RunResult) -> Result<()> {
    let path = project.join("plugins.lock");
    let mut lock = fs::read_to_string(&path)
        .ok()
        .and_then(|t| PluginsLock::parse(&t).ok())
        .unwrap_or(PluginsLock {
            version: 1,
            plugin: Vec::new(),
        });
    lock.plugin.retain(|p| p.id != result.plugin_id);
    lock.plugin.push(LockedPlugin {
        id: result.plugin_id.clone(),
        version: result.plugin_version.clone(),
        sha256: None,
        source: Some(
            if result.builtin {
                "built-in"
            } else {
                "installed"
            }
            .into(),
        ),
    });
    lock.plugin.sort_by(|a, b| a.id.cmp(&b.id));
    let text = format!(
        "# Exact plugin versions this project was made with (written by ptk).\n{}",
        toml::to_string(&lock).expect("lock serializes")
    );
    fs::write(&path, text).map_err(io_ctx(path.display().to_string()))
}

fn pick(cands: Vec<Candidate>, wanted: Option<&str>, what: &str) -> Result<Candidate> {
    match wanted {
        Some(id) => cands.into_iter().find(|c| c.id() == id).ok_or_else(|| {
            Error::Invalid(format!(
                "plugin {id:?} cannot {what} (see `ptk plugin list`)"
            ))
        }),
        None => cands.into_iter().next().ok_or_else(|| {
            Error::Invalid(format!(
                "no plugin can {what}; look in the marketplace (`ptk market list`)"
            ))
        }),
    }
}

/// Exports a project for one target.
pub fn export(
    engine: &mut EngineClient,
    project: &Path,
    target_id: &str,
    opts: &ExportOptions,
    on_event: &mut dyn FnMut(Event),
) -> Result<RunResult> {
    let manifest = Manifest::load(project)?;
    let registry = TargetRegistry::load_default().0;
    let target = registry
        .get(target_id)
        .ok_or_else(|| Error::Invalid(format!("unknown target {target_id:?} (see `ptk targets`)")))?
        .clone();
    let game = game_for(project, opts.game.as_deref())?;
    let engine_id = manifest.game.engine.clone();
    let what = format!(
        "export {} games to {}",
        engine_id.as_deref().unwrap_or("these"),
        target.id
    );
    let cands = candidates(engine, "export", engine_id.as_deref(), Some(&target))?;
    let plugin = pick(cands, opts.plugin.as_deref(), &what)?;
    let output = opts
        .output
        .clone()
        .unwrap_or_else(|| default_output(project))
        .join(&target.id);
    let project_abs = project.canonicalize().unwrap_or_else(|_| absolute(project));
    let folders =
        json!({"$GAME": absolute(&game), "$PROJECT": project_abs, "$OUTPUT": absolute(&output)});
    let result = run(
        engine,
        &plugin,
        "export",
        &manifest,
        folders,
        Some(&target),
        &opts.settings,
        on_event,
    )?;
    lock_plugin(project, &result)?;
    if let Some(out) = &opts.output {
        let mut s = LocalState::load(project);
        s.output = Some(out.canonicalize().unwrap_or_else(|_| absolute(out)));
        s.save(project)?;
    }
    Ok(result)
}

/// Runs the import plugin for the project's engine, if there is one.
/// `Ok(None)` means no import plugin handles this engine.
///
/// `file_title` is the title ptk guessed from the build's file name; when
/// the project still has it, the game's real title (if the plugin found
/// one) replaces it.
pub fn import(
    engine: &mut EngineClient,
    project: &Path,
    game: &Path,
    file_title: Option<&str>,
    on_event: &mut dyn FnMut(Event),
) -> Result<Option<RunResult>> {
    let manifest = Manifest::load(project)?;
    let Some(engine_id) = manifest.game.engine.clone() else {
        return Ok(None);
    };
    let cands = candidates(engine, "import", Some(&engine_id), None)?;
    let Some(plugin) = cands.into_iter().next() else {
        return Ok(None);
    };
    let project_abs = project.canonicalize().unwrap_or_else(|_| absolute(project));
    let folders = json!({"$GAME": absolute(game), "$PROJECT": project_abs});
    let result = run(
        engine,
        &plugin,
        "import",
        &manifest,
        folders,
        None,
        &BTreeMap::new(),
        on_event,
    )?;
    lock_plugin(project, &result)?;
    apply_import(project, game, file_title, &result)?;
    Ok(Some(result))
}

/// Uses what the import plugin found: the game's real title (unless you
/// named the game yourself) and its icon (kept per machine).
fn apply_import(
    project: &Path,
    game: &Path,
    file_title: Option<&str>,
    result: &RunResult,
) -> Result<()> {
    if let Some(title) = result.title.as_ref().filter(|t| !t.trim().is_empty()) {
        let mut m = Manifest::load(project)?;
        let guessed = [
            Some(crate::import::title_of(game)),
            file_title.map(str::to_owned),
        ];
        let current = m.game.title.clone().unwrap_or_default();
        if current.is_empty() || guessed.iter().flatten().any(|g| *g == current) {
            m.game.title = Some(title.trim().to_owned());
            m.save(project)?;
        }
    }
    if let Some(icon) = result.icon.as_ref().filter(|i| i.is_file()) {
        let mut s = LocalState::load(project);
        s.game_icon = Some(icon.clone());
        s.save(project)?;
    }
    Ok(())
}

/// Parses `key=value` settings.
pub fn parse_settings(items: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for item in items {
        let (k, v) = item
            .split_once('=')
            .ok_or_else(|| Error::Invalid(format!("--set {item:?} should look like key=value")))?;
        let k = k.trim();
        if k.is_empty() {
            return Err(Error::Invalid(format!("--set {item:?} has no key")));
        }
        out.insert(k.to_owned(), v.trim().to_owned());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parse() {
        let s = parse_settings(&["a=1".into(), " b = two words ".into()]).unwrap();
        assert_eq!(s["a"], "1");
        assert_eq!(s["b"], "two words");
        assert!(parse_settings(&["nope".into()]).is_err());
        assert!(parse_settings(&["=x".into()]).is_err());
    }

    #[test]
    fn titles_lose_platform_suffixes() {
        use crate::import::title_of;
        assert_eq!(title_of(Path::new("PtkDemo-windows.zip")), "PtkDemo");
        assert_eq!(title_of(Path::new("My Game (Mac).dmg")), "My Game");
        assert_eq!(title_of(Path::new("Quest_Win64_x64.zip")), "Quest");
        assert_eq!(title_of(Path::new("Windows.exe")), "Windows");
        assert_eq!(title_of(Path::new("Darwin.app")), "Darwin");
    }

    #[test]
    fn local_state_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(LocalState::load(tmp.path()), LocalState::default());
        LocalState::remember_game(tmp.path(), tmp.path(), None).unwrap();
        assert!(LocalState::load(tmp.path()).game.is_some());
        assert!(game_for(tmp.path(), None).is_ok());
        assert!(game_for(tmp.path(), Some(&tmp.path().join("missing"))).is_err());
    }
}
