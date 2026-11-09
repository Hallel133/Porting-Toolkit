//! `ptk plugin`: list, new, validate, install.
//!
//! Plugins are Python packages and get full access to the computer once
//! they run, so installing shows the declared permissions and asks first.
//! Phase 1 only copies a validated plugin into place; nothing is executed.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use ptk_core::engine::EngineClient;
use ptk_core::paths;
use ptk_core::targets::TargetRegistry;
use serde::Deserialize;
use serde_json::json;

use crate::style::{Style, Table};
use crate::{PluginAction, PluginKind};

#[derive(Deserialize)]
struct Validation {
    path: String,
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    plugin: Option<PluginInfo>,
    permissions: Vec<String>,
    risk: String,
}

#[derive(Deserialize)]
struct PluginInfo {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    kind: Option<String>,
    description: Option<String>,
    license: Option<String>,
}

#[derive(Deserialize)]
struct Listed {
    id: String,
    version: String,
    kind: String,
    path: String,
    valid: bool,
    risk: String,
    reviewed: bool,
}

fn absolute(p: &Path) -> anyhow::Result<PathBuf> {
    Ok(if p.is_absolute() {
        p.to_owned()
    } else {
        std::env::current_dir()?.join(p)
    })
}

fn known_targets() -> Vec<String> {
    TargetRegistry::load_default().0.ids()
}

fn risk(style: &Style, r: &str) -> String {
    match r {
        "low" => style.green("low"),
        "medium" => style.yellow("medium"),
        "high" => style.red("high"),
        other => style.dim(other),
    }
}

fn validate(
    engine: &mut EngineClient,
    dir: &Path,
) -> anyhow::Result<(Validation, serde_json::Value)> {
    let raw = engine.call(
        "plugin.validate",
        json!({ "path": absolute(dir)?.to_string_lossy(), "known_targets": known_targets() }),
    )?;
    Ok((
        serde_json::from_value(raw.clone()).context("unexpected validation result")?,
        raw,
    ))
}

fn print_validation(style: &Style, v: &Validation) {
    let name = v
        .plugin
        .as_ref()
        .map(|p| {
            format!(
                "{} {} {}",
                p.id.as_deref().unwrap_or("?"),
                p.version.as_deref().unwrap_or("?"),
                style.dim(format!("({} plugin)", p.kind.as_deref().unwrap_or("?")))
            )
        })
        .unwrap_or_else(|| v.path.clone());
    println!("{}", style.heading(&format!("Plugin {name}")));
    if let Some(p) = &v.plugin {
        if let Some(n) = &p.name {
            println!("  {} {n}", style.dim(format!("{:<12}", "Name")));
        }
        if let Some(d) = p.description.as_ref().filter(|d| !d.is_empty()) {
            println!("  {} {d}", style.dim(format!("{:<12}", "About")));
        }
        if let Some(l) = &p.license {
            println!("  {} {l}", style.dim(format!("{:<12}", "License")));
        }
    }
    println!("  {} {}", style.dim(format!("{:<12}", "Path")), v.path);
    if !v.permissions.is_empty() {
        println!(
            "  {} {}",
            style.dim(format!("{:<12}", "Risk")),
            risk(style, &v.risk)
        );
        println!("  {}", style.dim("Permissions"));
        for p in &v.permissions {
            println!("    • {p}");
        }
    }
    for e in &v.errors {
        println!("  {} {e}", style.fail());
    }
    for w in &v.warnings {
        println!("  {} {w}", style.warn());
    }
    if v.valid {
        println!("  {} valid", style.ok());
    }
}

fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<usize> {
    fs::create_dir_all(dst)?;
    let mut n = 0;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if matches!(
            name_s.as_ref(),
            ".git" | "__pycache__" | ".venv" | ".pytest_cache"
        ) || ptk_core::hash::is_junk_file(&name_s)
        {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            bail!(
                "{} is a symbolic link; plugins must not contain links",
                entry.path().display()
            );
        } else if ft.is_dir() {
            n += copy_tree(&entry.path(), &dst.join(&name))?;
        } else if ft.is_file() {
            fs::copy(entry.path(), dst.join(&name))?;
            n += 1;
        }
    }
    Ok(n)
}

pub fn run(style: &Style, action: PluginAction) -> anyhow::Result<ExitCode> {
    let mut engine = EngineClient::start()?;
    let code = match action {
        PluginAction::List { json } => {
            let mut dirs = vec![paths::plugins_dir()];
            let local = std::env::current_dir()?.join("plugins");
            if local.is_dir() {
                dirs.push(local);
            }
            let raw = engine.call(
                "plugin.list",
                json!({
                    "dirs": dirs.iter().map(|d| d.to_string_lossy()).collect::<Vec<_>>(),
                    "known_targets": known_targets(),
                }),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&raw)?);
            } else {
                let list: Vec<Listed> = serde_json::from_value(raw)?;
                println!("{}", style.heading("Plugins"));
                if list.is_empty() {
                    println!(
                        "  No plugins installed in {}.",
                        paths::plugins_dir().display()
                    );
                    println!(
                        "  Create one: {}",
                        style.bold("ptk plugin new my-plugin --kind import")
                    );
                } else {
                    let mut t = Table::new(&[
                        "ID", "VERSION", "KIND", "RISK", "STATUS", "REVIEWED", "PATH",
                    ]);
                    for p in &list {
                        t.row(vec![
                            style.bold(&p.id),
                            p.version.clone(),
                            p.kind.clone(),
                            risk(style, &p.risk),
                            if p.valid {
                                style.green("valid")
                            } else {
                                style.red("invalid")
                            },
                            if p.reviewed {
                                style.green("yes")
                            } else {
                                style.dim("no")
                            },
                            style.dim(&p.path),
                        ]);
                    }
                    println!("{}", t.render(style));
                }
            }
            ExitCode::SUCCESS
        }
        PluginAction::New { dir, kind, id } => {
            #[derive(Deserialize)]
            struct Created {
                path: String,
                id: String,
                files: Vec<String>,
            }
            let raw = engine.call(
                "plugin.new",
                json!({ "path": absolute(&dir)?.to_string_lossy(), "kind": kind.as_str(), "id": id }),
            )?;
            let c: Created = serde_json::from_value(raw)?;
            println!(
                "{} Created {} plugin {} in {}",
                style.ok(),
                kind.as_str(),
                style.bold(&c.id),
                c.path
            );
            for f in &c.files {
                println!("  {}", style.cyan(f));
            }
            println!();
            println!(
                "  Edit plugin.toml, then: {}",
                style.bold(format!("ptk plugin validate {}", dir.display()))
            );
            if matches!(kind, PluginKind::Export) {
                println!(
                    "  {}",
                    style.dim(
                        "Export plugins list the target ids they build for (see `ptk targets`)."
                    )
                );
            }
            ExitCode::SUCCESS
        }
        PluginAction::Validate { dir, json } => {
            let (v, raw) = validate(&mut engine, &dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&raw)?);
            } else {
                print_validation(style, &v);
            }
            if v.valid {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        PluginAction::Install { dir, yes } => {
            let (v, _) = validate(&mut engine, &dir)?;
            print_validation(style, &v);
            if !v.valid {
                bail!("not installing an invalid plugin");
            }
            let id = v
                .plugin
                .as_ref()
                .and_then(|p| p.id.clone())
                .context("plugin has no id")?;
            let dest = paths::plugins_dir().join(&id);
            println!();
            println!(
                "  {} Python plugins can do anything your user account can. The permissions",
                style.warn()
            );
            println!(
                "    above are what the author declares; this plugin has {} been reviewed.",
                style.bold("not")
            );
            if dest.exists() {
                println!("    It replaces the installed copy in {}.", dest.display());
            }
            if !yes {
                if !std::io::stdin().is_terminal() {
                    bail!("refusing to install without confirmation; rerun with --yes");
                }
                print!("  Install {id}? [y/N] ");
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().lock().read_line(&mut answer)?;
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("  Not installed.");
                    return Ok(ExitCode::FAILURE);
                }
            }
            let staging = paths::plugins_dir().join(format!(".{id}.installing"));
            let _ = fs::remove_dir_all(&staging);
            let n = copy_tree(&absolute(&dir)?, &staging);
            let n = match n {
                Ok(n) => n,
                Err(e) => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(e);
                }
            };
            if dest.exists() {
                fs::remove_dir_all(&dest)?;
            }
            fs::rename(&staging, &dest)?;
            println!(
                "{} Installed {id} ({n} files) in {}",
                style.ok(),
                dest.display()
            );
            println!(
                "  {}",
                style.dim("Plugins are not run yet: running them arrives in phase 3.")
            );
            ExitCode::SUCCESS
        }
    };
    engine.shutdown()?;
    Ok(code)
}
