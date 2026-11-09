//! SDKs: a bundle that teaches Porting Toolkit about new platforms.
//!
//! An SDK is a folder with:
//!
//! ```text
//! my-sdk/
//! ├── ptk-sdk.toml        [sdk] id, name, version, license, description
//! ├── targets/*.toml      [[target]] definitions (see targets.rs)
//! └── plugins/<id>/       export (or import) plugins for those targets
//! ```
//!
//! Installing an SDK copies its target files into `<data>/targets/` and
//! installs its plugins (after showing their permissions). This is how the
//! NoVA SDK, for example, can add `nova-*` targets and their exporter.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::EngineClient;
use crate::error::{Error, Result, io_ctx};
use crate::paths;
use crate::plugins::{self, Validation};
use crate::targets::{Target, TargetRegistry, valid_target_id};

pub const SDK_FILE: &str = "ptk-sdk.toml";
pub const SDK_SCHEMA: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SdkFile {
    pub schema_version: u32,
    pub sdk: SdkInfo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SdkInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// Result of [`validate`].
#[derive(Clone, Debug, Serialize)]
pub struct SdkReport {
    pub path: PathBuf,
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub sdk: Option<SdkInfo>,
    pub targets: Vec<Target>,
    pub plugins: Vec<Validation>,
}

fn sorted_children(dir: &Path, want_dirs: bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir() == want_dirs)
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Checks an SDK folder. Plugin folders are validated by the engine.
pub fn validate(dir: &Path, engine: &mut EngineClient) -> Result<SdkReport> {
    let mut report = SdkReport {
        path: dir.to_owned(),
        valid: false,
        errors: Vec::new(),
        warnings: Vec::new(),
        sdk: None,
        targets: Vec::new(),
        plugins: Vec::new(),
    };
    let file = dir.join(SDK_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(t) => t,
        Err(_) => {
            report
                .errors
                .push(format!("no {SDK_FILE} in {}", dir.display()));
            return Ok(report);
        }
    };
    let parsed: SdkFile = match toml::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            report.errors.push(format!("{SDK_FILE}: {e}"));
            return Ok(report);
        }
    };
    if parsed.schema_version != SDK_SCHEMA {
        report
            .errors
            .push(format!("schema_version must be {SDK_SCHEMA}"));
    }
    let info = parsed.sdk;
    if !valid_target_id(&info.id) {
        report
            .errors
            .push("sdk.id must be lower-case letters, digits and single dashes".into());
    }
    if info.name.trim().is_empty() || info.version.trim().is_empty() {
        report
            .errors
            .push("sdk.name and sdk.version are required".into());
    }
    if info.license.trim().is_empty() {
        report.errors.push("sdk.license is required".into());
    }

    // Targets load into a fresh registry, so clashes with built-in targets
    // or between files are reported.
    let mut registry = TargetRegistry::builtin();
    for f in sorted_children(&dir.join("targets"), false) {
        if f.extension().is_some_and(|e| e == "toml") {
            if let Err(e) = registry.load_file(&f) {
                report.errors.push(e.to_string());
            }
        }
    }
    report.targets = registry
        .iter()
        .filter(|t| !matches!(t.source, crate::targets::TargetSource::Builtin))
        .cloned()
        .collect();
    for t in &report.targets {
        if !t.id.starts_with(&format!("{}-", info.id)) && t.id != info.id {
            report.warnings.push(format!(
                "target {:?} does not start with the SDK id {:?}",
                t.id, info.id
            ));
        }
    }

    let known = registry.ids();
    for p in sorted_children(&dir.join("plugins"), true) {
        let raw = engine.call(
            "plugin.validate",
            serde_json::json!({ "path": p.to_string_lossy(), "known_targets": known }),
        )?;
        let v: Validation = serde_json::from_value(raw)
            .map_err(|e| Error::Invalid(format!("unexpected validation result: {e}")))?;
        if !v.valid {
            report.errors.push(format!(
                "plugin {}: {}",
                p.file_name().unwrap_or_default().to_string_lossy(),
                v.errors.join("; ")
            ));
        }
        report.plugins.push(v);
    }
    let plugin_ids: Vec<String> = report
        .plugins
        .iter()
        .filter_map(|v| v.plugin.as_ref().and_then(|p| p.id.clone()))
        .collect();
    for t in &report.targets {
        if let Some(b) = &t.export_backend
            && !plugin_ids.contains(b)
        {
            report.warnings.push(format!(
                "target {:?} names export plugin {b:?}, which is not in plugins/",
                t.id
            ));
        }
    }
    if report.targets.is_empty() && report.plugins.is_empty() {
        report.errors.push(
            "an SDK needs at least one target (targets/*.toml) or plugin (plugins/*/)".into(),
        );
    }
    report.sdk = Some(info);
    report.valid = report.errors.is_empty();
    Ok(report)
}

/// Creates an SDK skeleton: one target and an export plugin for it.
pub fn create(dir: &Path, id: &str, engine: &mut EngineClient) -> Result<Vec<String>> {
    if !valid_target_id(id) {
        return Err(Error::Invalid(format!(
            "{id:?} is not a valid SDK id (lower-case letters, digits, dashes)"
        )));
    }
    if dir.exists()
        && fs::read_dir(dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(Error::Invalid(format!(
            "{} already exists and is not empty",
            dir.display()
        )));
    }
    let target = format!("{id}-device");
    let exporter = format!("{id}-export");
    let name = id.replace('-', " ");
    fs::create_dir_all(dir.join("targets")).map_err(io_ctx(dir.display().to_string()))?;
    let files = [
        (
            SDK_FILE.to_owned(),
            format!(
                "schema_version = 1\n\n[sdk]\nid = \"{id}\"\nname = \"{name} SDK\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\ndescription = \"Adds {name} targets to Porting Toolkit.\"\nauthors = []\n"
            ),
        ),
        (
            format!("targets/{id}.toml"),
            format!(
                "# Platforms this SDK adds. Ids should start with \"{id}-\".\n[[target]]\nid = \"{target}\"\nname = \"{name} device\"\nos = \"{id}\"\narch = \"riscv64\"\nstatus = \"experimental\"\ndescription = \"Describe the device.\"\nexport_backend = \"{exporter}\"\n"
            ),
        ),
        (
            "README.md".to_owned(),
            format!(
                "# {name} SDK for Porting Toolkit\n\n```bash\nptk sdk validate .\nptk publish .        # zip + sha256 + marketplace entry\n```\n"
            ),
        ),
    ];
    for (rel, text) in &files {
        let p = dir.join(rel);
        fs::write(&p, text).map_err(io_ctx(p.display().to_string()))?;
    }
    let created = plugins::create(
        engine,
        &dir.join("plugins").join(&exporter),
        "export",
        Some(&exporter),
    )?;
    let toml_path = PathBuf::from(&created.path).join("plugin.toml");
    let text = fs::read_to_string(&toml_path).map_err(io_ctx(toml_path.display().to_string()))?;
    fs::write(
        &toml_path,
        text.replace(
            "targets = [\"macos-arm64\"]",
            &format!("targets = [\"{target}\"]"),
        ),
    )
    .map_err(io_ctx(toml_path.display().to_string()))?;
    let mut out: Vec<String> = files.iter().map(|(r, _)| r.clone()).collect();
    out.extend(
        created
            .files
            .iter()
            .map(|f| format!("plugins/{exporter}/{f}")),
    );
    Ok(out)
}

/// Where installed SDK manifests are kept.
pub fn sdks_dir() -> PathBuf {
    paths::data_dir().join("sdks")
}

/// What [`install`] did.
#[derive(Clone, Debug, Serialize)]
pub struct Installed {
    pub targets: Vec<PathBuf>,
    pub plugins: Vec<PathBuf>,
}

/// Installs a validated SDK. Callers must show the plugins' permissions and
/// get consent first.
pub fn install(report: &SdkReport) -> Result<Installed> {
    if !report.valid {
        return Err(Error::Invalid("not installing an invalid SDK".into()));
    }
    let info = report.sdk.as_ref().expect("valid SDK has info");
    let tdir = paths::targets_dir();
    fs::create_dir_all(&tdir).map_err(io_ctx(tdir.display().to_string()))?;
    let mut targets = Vec::new();
    for f in sorted_children(&report.path.join("targets"), false) {
        if f.extension().is_some_and(|e| e == "toml") {
            let dest = tdir.join(format!(
                "{}--{}",
                info.id,
                f.file_name().unwrap_or_default().to_string_lossy()
            ));
            fs::copy(&f, &dest).map_err(io_ctx(dest.display().to_string()))?;
            targets.push(dest);
        }
    }
    let mut installed_plugins = Vec::new();
    for v in &report.plugins {
        let (dest, _) = plugins::install(Path::new(&v.path), v)?;
        installed_plugins.push(dest);
    }
    let keep = sdks_dir().join(&info.id);
    fs::create_dir_all(&keep).map_err(io_ctx(keep.display().to_string()))?;
    fs::copy(report.path.join(SDK_FILE), keep.join(SDK_FILE))
        .map_err(io_ctx(keep.display().to_string()))?;
    Ok(Installed {
        targets,
        plugins: installed_plugins,
    })
}

/// SDKs installed in the data folder.
pub fn installed() -> Vec<SdkInfo> {
    sorted_children(&sdks_dir(), true)
        .into_iter()
        .filter_map(|d| fs::read_to_string(d.join(SDK_FILE)).ok())
        .filter_map(|t| toml::from_str::<SdkFile>(&t).ok())
        .map(|f| f.sdk)
        .collect()
}
