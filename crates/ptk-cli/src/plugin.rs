//! `ptk plugin`: list, new, validate, install.
//!
//! Plugins are Python packages and get full access to the computer once
//! they run, so installing shows the declared permissions and asks first.
//! `ptk export` and `ptk import` run them.

use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

use anyhow::bail;
use ptk_core::engine::EngineClient;
use ptk_core::paths;
use ptk_core::plugins::{self, Validation};

use crate::style::{Style, Table};
use crate::{PluginAction, PluginKind};

fn risk(style: &Style, r: &str) -> String {
    match r {
        "low" => style.green("low"),
        "medium" => style.yellow("medium"),
        "high" => style.red("high"),
        other => style.dim(other),
    }
}

fn print_validation(style: &Style, v: &Validation) {
    let label = |k: &str| style.dim(format!("{k:<12}"));
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
            println!("  {} {n}", label("Name"));
        }
        if let Some(d) = p.description.as_ref().filter(|d| !d.is_empty()) {
            println!("  {} {d}", label("About"));
        }
        if let Some(l) = &p.license {
            println!("  {} {l}", label("License"));
        }
    }
    println!("  {} {}", label("Path"), v.path);
    if !v.permissions.is_empty() {
        println!("  {} {}", label("Risk"), risk(style, &v.risk));
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

pub fn run(style: &Style, action: PluginAction) -> anyhow::Result<ExitCode> {
    let mut engine = EngineClient::start()?;
    let code = match action {
        PluginAction::List { json } => {
            let (list, raw) = plugins::list(&mut engine, &plugins::default_dirs())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&raw)?);
            } else {
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
            let c = plugins::create(&mut engine, &dir, kind.as_str(), id.as_deref())?;
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
            let (v, raw) = plugins::validate(&mut engine, &dir)?;
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
            let (v, _) = plugins::validate(&mut engine, &dir)?;
            print_validation(style, &v);
            if !v.valid {
                bail!("not installing an invalid plugin");
            }
            let id = v
                .plugin
                .as_ref()
                .and_then(|p| p.id.clone())
                .unwrap_or_default();
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
            let (dest, n) = plugins::install(&dir, &v)?;
            println!(
                "{} Installed {id} ({n} files) in {}",
                style.ok(),
                dest.display()
            );
            ExitCode::SUCCESS
        }
    };
    engine.shutdown()?;
    Ok(code)
}
