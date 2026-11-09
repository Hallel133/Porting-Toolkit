//! Environment checks behind `ptk doctor` (and the GUI's Doctor view).
//!
//! Checks the tools ptk needs and, when asked, creates or updates the managed
//! Python environment for the engine (`uv venv` + an editable install of
//! `engine/`) in the per-user data directory.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::engine::{EngineClient, engine_source_dir, which};
use crate::paths;
use crate::targets::TargetRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    /// What to do about a warning or failure.
    pub hint: Option<String>,
}

fn check(status: Status, name: &str, detail: impl Into<String>, hint: Option<&str>) -> Check {
    Check {
        name: name.into(),
        status,
        detail: detail.into(),
        hint: hint.filter(|_| status != Status::Ok).map(str::to_owned),
    }
}

/// First line of a command's output, if it ran successfully.
fn version_of(program: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    String::from_utf8_lossy(&text)
        .lines()
        .next()
        .map(|l| l.trim().to_owned())
}

/// A tool on PATH, or in one of the usual install locations (`false`).
fn find_tool(name: &str, extra: &[PathBuf]) -> Option<(PathBuf, bool)> {
    if let Some(p) = which(name) {
        return Some((p, true));
    }
    extra
        .iter()
        .find(|p| p.is_file())
        .map(|p| (p.clone(), false))
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" | ")
}

/// Runs every check. With `install`, also creates or updates the managed
/// engine environment.
pub fn run(install: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let reg = TargetRegistry::builtin();
    let host = reg
        .host()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "not a built-in target".into());
    out.push(check(
        Status::Ok,
        "Platform",
        format!(
            "{} {} ({host})",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        None,
    ));
    if !cfg!(target_os = "macos") {
        out.push(check(
            Status::Warn,
            "Support",
            "macOS comes first; Windows and Linux are planned for phase 4",
            None,
        ));
    }

    if cfg!(target_os = "macos") {
        out.push(match version_of(Path::new("xcode-select"), &["-p"]) {
            Some(p) => check(Status::Ok, "Xcode Command Line", p, None),
            None => check(
                Status::Fail,
                "Xcode Command Line",
                "not installed",
                Some("xcode-select --install"),
            ),
        });
    }

    out.push(
        match find_tool("clang", &[]).and_then(|(p, _)| version_of(&p, &["--version"])) {
            Some(v) => check(Status::Ok, "clang", v, None),
            None => check(
                Status::Fail,
                "clang",
                "not found",
                Some(if cfg!(target_os = "macos") {
                    "xcode-select --install"
                } else {
                    "install clang with your package manager"
                }),
            ),
        },
    );

    let cargo_bin = home().join(".cargo").join("bin");
    out.push(match find_tool("rustup", &[cargo_bin.join("rustup")]) {
        Some((p, on_path)) => {
            let v = version_of(&p, &["--version"]).unwrap_or_else(|| "installed".into());
            if on_path {
                check(Status::Ok, "rustup", v, None)
            } else {
                check(
                    Status::Warn,
                    "rustup",
                    format!("{v} (not on PATH)"),
                    Some("add ~/.cargo/bin to PATH"),
                )
            }
        }
        None => check(
            Status::Warn,
            "rustup",
            "not found (only needed to build ptk from source)",
            Some("https://rustup.rs"),
        ),
    });

    let uv = find_tool(
        "uv",
        &[
            home().join(".local/bin/uv"),
            home().join(".cargo/bin/uv"),
            PathBuf::from("/opt/homebrew/bin/uv"),
        ],
    );
    out.push(match &uv {
        Some((p, _)) => check(
            Status::Ok,
            "uv",
            version_of(p, &["--version"]).unwrap_or_else(|| p.display().to_string()),
            None,
        ),
        None => check(
            Status::Fail,
            "uv",
            "not found (manages the engine's Python)",
            Some("brew install uv   or   curl -LsSf https://astral.sh/uv/install.sh | sh"),
        ),
    });

    let source = engine_source_dir();
    out.push(match &source {
        Some(s) => check(Status::Ok, "Engine source", s.display().to_string(), None),
        None => check(
            Status::Fail,
            "Engine source",
            "not found",
            Some("set PTK_ENGINE_DIR to the engine/ folder of a Porting Toolkit checkout"),
        ),
    });

    let env_dir = paths::engine_env_dir();
    let python = paths::engine_env_python();
    if !install {
        out.push(if python.is_file() {
            check(
                Status::Ok,
                "Managed environment",
                env_dir.display().to_string(),
                None,
            )
        } else {
            check(
                Status::Warn,
                "Managed environment",
                format!("not created ({})", env_dir.display()),
                Some("run `ptk doctor` without --no-install"),
            )
        });
    } else if let (Some((uv, _)), Some(source)) = (&uv, &source) {
        out.push(install_env(uv, source, &env_dir, &python));
    }

    out.push(match EngineClient::start() {
        Ok(engine) => {
            let i = &engine.info;
            let c = check(
                Status::Ok,
                "Engine",
                format!(
                    "{} {}, Python {}, protocol {} ({})",
                    i.name, i.version, i.python, i.protocol, engine.command.origin
                ),
                None,
            );
            let _ = engine.shutdown();
            c
        }
        Err(e) => check(
            Status::Fail,
            "Engine",
            e.to_string().lines().next().unwrap_or_default(),
            Some("run `ptk doctor` (needs uv), or set PTK_PYTHON to a Python 3.11+"),
        ),
    });

    out.push(check(
        Status::Ok,
        "Data folder",
        paths::data_dir().display().to_string(),
        None,
    ));
    out
}

fn install_env(uv: &Path, source: &Path, env_dir: &Path, python: &Path) -> Check {
    const NAME: &str = "Managed environment";
    let created = !python.is_file();
    if created {
        match Command::new(uv)
            .args(["venv", "--quiet", "--python", ">=3.11"])
            .arg(env_dir)
            .output()
        {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                return check(
                    Status::Fail,
                    NAME,
                    format!("uv venv failed: {}", tail(&o.stderr)),
                    Some("check that uv can find or download Python 3.11+"),
                );
            }
            Err(e) => return check(Status::Fail, NAME, format!("could not run uv: {e}"), None),
        }
    }
    match Command::new(uv)
        .args(["pip", "install", "--quiet", "--python"])
        .arg(python)
        .arg("--editable")
        .arg(source)
        .output()
    {
        Ok(o) if o.status.success() => check(
            Status::Ok,
            NAME,
            format!(
                "{} {}",
                if created { "created" } else { "up to date" },
                env_dir.display()
            ),
            None,
        ),
        Ok(o) => check(
            Status::Fail,
            NAME,
            format!("installing the engine failed: {}", tail(&o.stderr)),
            Some("rerun with network access, or set PTK_PYTHON to a Python 3.11+"),
        ),
        Err(e) => check(Status::Fail, NAME, format!("could not run uv: {e}"), None),
    }
}
