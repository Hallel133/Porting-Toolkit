//! `ptk doctor`: checks the tools ptk needs and sets up the managed Python
//! environment for the engine (`uv venv` + an editable install of
//! `engine/`) in the per-user data directory.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use ptk_core::engine::{EngineClient, engine_source_dir, which};
use ptk_core::paths;
use ptk_core::targets::TargetRegistry;

use crate::style::Style;

#[derive(PartialEq)]
enum Status {
    Ok,
    Warn,
    Fail,
}

struct Doctor<'a> {
    style: &'a Style,
    problems: usize,
    warnings: usize,
}

impl Doctor<'_> {
    fn report(&mut self, status: Status, name: &str, detail: impl AsRef<str>, hint: Option<&str>) {
        let mark = match status {
            Status::Ok => self.style.ok(),
            Status::Warn => {
                self.warnings += 1;
                self.style.warn()
            }
            Status::Fail => {
                self.problems += 1;
                self.style.fail()
            }
        };
        println!("  {mark} {:<22} {}", name, detail.as_ref());
        if let Some(h) = hint
            && status != Status::Ok
        {
            println!("    {:<22} {}", "", self.style.dim(format!("→ {h}")));
        }
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

/// Finds a tool on PATH or in a list of usual install locations.
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

pub fn run(style: &Style, no_install: bool) -> anyhow::Result<ExitCode> {
    let mut d = Doctor {
        style,
        problems: 0,
        warnings: 0,
    };
    println!("{}", style.heading("ptk doctor"));

    let reg = TargetRegistry::builtin();
    let host = reg
        .host()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "not a built-in target".into());
    d.report(
        Status::Ok,
        "Platform",
        format!(
            "{} {} ({host})",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        None,
    );
    if !cfg!(target_os = "macos") {
        d.report(
            Status::Warn,
            "Support",
            "macOS comes first; Windows and Linux are planned for phase 4",
            None,
        );
    }

    if cfg!(target_os = "macos") {
        match version_of(Path::new("xcode-select"), &["-p"]) {
            Some(p) => d.report(Status::Ok, "Xcode Command Line", p, None),
            None => d.report(
                Status::Fail,
                "Xcode Command Line",
                "not installed",
                Some("xcode-select --install"),
            ),
        }
    }

    match find_tool("clang", &[]).and_then(|(p, _)| version_of(&p, &["--version"])) {
        Some(v) => d.report(Status::Ok, "clang", v, None),
        None => d.report(
            Status::Fail,
            "clang",
            "not found",
            Some(if cfg!(target_os = "macos") {
                "xcode-select --install"
            } else {
                "install clang with your package manager"
            }),
        ),
    }

    let cargo_bin = home().join(".cargo").join("bin");
    match find_tool("rustup", &[cargo_bin.join("rustup")]) {
        Some((p, on_path)) => {
            let v = version_of(&p, &["--version"]).unwrap_or_else(|| "installed".into());
            if on_path {
                d.report(Status::Ok, "rustup", v, None);
            } else {
                d.report(
                    Status::Warn,
                    "rustup",
                    format!("{v} (not on PATH)"),
                    Some("add ~/.cargo/bin to PATH"),
                );
            }
        }
        None => d.report(
            Status::Warn,
            "rustup",
            "not found (only needed to build ptk from source)",
            Some("https://rustup.rs"),
        ),
    }

    let uv = find_tool(
        "uv",
        &[
            home().join(".local/bin/uv"),
            home().join(".cargo/bin/uv"),
            PathBuf::from("/opt/homebrew/bin/uv"),
        ],
    );
    match &uv {
        Some((p, _)) => d.report(
            Status::Ok,
            "uv",
            version_of(p, &["--version"]).unwrap_or_else(|| p.display().to_string()),
            None,
        ),
        None => d.report(
            Status::Fail,
            "uv",
            "not found (manages the engine's Python)",
            Some("brew install uv   or   curl -LsSf https://astral.sh/uv/install.sh | sh"),
        ),
    }

    let source = engine_source_dir();
    match &source {
        Some(s) => d.report(Status::Ok, "Engine source", s.display().to_string(), None),
        None => d.report(
            Status::Fail,
            "Engine source",
            "not found",
            Some("set PTK_ENGINE_DIR to the engine/ folder of a Porting Toolkit checkout"),
        ),
    }

    let env_dir = paths::engine_env_dir();
    let python = paths::engine_env_python();
    if no_install {
        if python.is_file() {
            d.report(
                Status::Ok,
                "Managed environment",
                env_dir.display().to_string(),
                None,
            );
        } else {
            d.report(
                Status::Warn,
                "Managed environment",
                format!("not created ({})", env_dir.display()),
                Some("run `ptk doctor` without --no-install"),
            );
        }
    } else if let (Some((uv, _)), Some(source)) = (&uv, &source) {
        let created = !python.is_file();
        let mut ok = true;
        if created {
            let out = Command::new(uv)
                .args(["venv", "--quiet", "--python", ">=3.11"])
                .arg(&env_dir)
                .output();
            match out {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    ok = false;
                    d.report(
                        Status::Fail,
                        "Managed environment",
                        format!("uv venv failed: {}", tail(&o.stderr)),
                        Some("check the uv output above"),
                    );
                }
                Err(e) => {
                    ok = false;
                    d.report(
                        Status::Fail,
                        "Managed environment",
                        format!("could not run uv: {e}"),
                        None,
                    );
                }
            }
        }
        if ok {
            let out = Command::new(uv)
                .args(["pip", "install", "--quiet", "--python"])
                .arg(&python)
                .arg("--editable")
                .arg(source)
                .output();
            match out {
                Ok(o) if o.status.success() => d.report(
                    Status::Ok,
                    "Managed environment",
                    format!(
                        "{} {}",
                        if created { "created" } else { "up to date" },
                        env_dir.display()
                    ),
                    None,
                ),
                Ok(o) => d.report(
                    Status::Fail,
                    "Managed environment",
                    format!("installing the engine failed: {}", tail(&o.stderr)),
                    Some("rerun with network access, or set PTK_PYTHON to a Python 3.11+"),
                ),
                Err(e) => d.report(
                    Status::Fail,
                    "Managed environment",
                    format!("could not run uv: {e}"),
                    None,
                ),
            }
        }
    }

    match EngineClient::start() {
        Ok(engine) => {
            let i = &engine.info;
            d.report(
                Status::Ok,
                "Engine",
                format!(
                    "{} {}, Python {}, protocol {} ({})",
                    i.name, i.version, i.python, i.protocol, engine.command.origin
                ),
                None,
            );
            let _ = engine.shutdown();
        }
        Err(e) => d.report(
            Status::Fail,
            "Engine",
            e.to_string().lines().next().unwrap_or_default(),
            Some("run `ptk doctor` (needs uv), or set PTK_PYTHON to a Python 3.11+"),
        ),
    }

    d.report(
        Status::Ok,
        "Data folder",
        paths::data_dir().display().to_string(),
        None,
    );

    println!();
    if d.problems == 0 {
        let extra = if d.warnings > 0 {
            format!(" ({} warnings)", d.warnings)
        } else {
            String::new()
        };
        println!("  {} ptk is ready{extra}", style.ok());
        Ok(ExitCode::SUCCESS)
    } else {
        println!("  {} {} problem(s) found", style.fail(), d.problems);
        Ok(ExitCode::FAILURE)
    }
}
