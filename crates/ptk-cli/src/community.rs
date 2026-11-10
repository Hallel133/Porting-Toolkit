//! `ptk sdk`, `ptk market`, `ptk publish`.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::bail;
use ptk_core::engine::EngineClient;
use ptk_core::market::{self, EntryKind, MARKETPLACE_REPO};
use ptk_core::sdk;

use crate::style::{Style, Table};
use crate::{MarketAction, SdkAction};

fn confirm(question: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        bail!("refusing to install without confirmation; rerun with --yes");
    }
    print!("  {question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn print_sdk(style: &Style, r: &sdk::SdkReport) {
    if let Some(s) = &r.sdk {
        println!("{}", style.heading(&format!("SDK {} {}", s.id, s.version)));
        println!("  {}", s.name);
    }
    for t in &r.targets {
        println!("  target  {} ({} / {})", style.bold(&t.id), t.os, t.arch);
    }
    for p in &r.plugins {
        let id = p
            .plugin
            .as_ref()
            .and_then(|i| i.id.clone())
            .unwrap_or_default();
        println!("  plugin  {} ({} risk)", style.bold(id), p.risk);
        for perm in &p.permissions {
            println!("          • {perm}");
        }
    }
    for e in &r.errors {
        println!("  {} {e}", style.fail());
    }
    for w in &r.warnings {
        println!("  {} {w}", style.warn());
    }
    if r.valid {
        println!("  {} valid", style.ok());
    }
}

pub fn sdk(style: &Style, action: SdkAction) -> anyhow::Result<ExitCode> {
    let mut engine = EngineClient::start()?;
    let code = match action {
        SdkAction::New { dir, id } => {
            let id = id.unwrap_or_else(|| {
                ptk_core::manifest::slugify(&dir.file_name().unwrap_or_default().to_string_lossy())
                    .trim_end_matches("-sdk")
                    .to_owned()
            });
            let files = sdk::create(&dir, &id, &mut engine)?;
            println!(
                "{} Created SDK {} in {}",
                style.ok(),
                style.bold(&id),
                dir.display()
            );
            for f in files {
                println!("  {}", style.cyan(f));
            }
            println!(
                "  Next: {}",
                style.bold(format!("ptk sdk validate {}", dir.display()))
            );
            ExitCode::SUCCESS
        }
        SdkAction::Validate { dir, json } => {
            let r = sdk::validate(&dir, &mut engine)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                print_sdk(style, &r);
            }
            if r.valid {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        SdkAction::Install { dir, yes } => {
            let r = sdk::validate(&dir, &mut engine)?;
            print_sdk(style, &r);
            if !r.valid {
                bail!("not installing an invalid SDK");
            }
            println!(
                "  {} Plugins can do anything your user account can.",
                style.warn()
            );
            if !confirm("Install this SDK?", yes)? {
                return Ok(ExitCode::FAILURE);
            }
            let done = sdk::install(&r)?;
            println!(
                "{} Installed {} targets and {} plugins",
                style.ok(),
                done.targets.len(),
                done.plugins.len()
            );
            ExitCode::SUCCESS
        }
    };
    engine.shutdown()?;
    Ok(code)
}

pub fn market(style: &Style, action: MarketAction) -> anyhow::Result<ExitCode> {
    let url = market::index_url();
    let (index, problems) = market::fetch_index(&url).map_err(|e| {
        anyhow::anyhow!("{e}\nThe marketplace index is {url} (repository {MARKETPLACE_REPO}).")
    })?;
    for p in &problems {
        eprintln!("{} skipped entry: {p}", style.warn());
    }
    match action {
        MarketAction::List { search, json } => {
            let q = search.unwrap_or_default();
            let list: Vec<_> = index.entries.iter().filter(|e| e.matches(&q)).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(ExitCode::SUCCESS);
            }
            println!("{}", style.heading("Marketplace"));
            if list.is_empty() {
                println!(
                    "  Nothing here yet. Publish yours: {}",
                    style.bold("ptk publish <folder>")
                );
            }
            if list.is_empty() {
                return Ok(ExitCode::SUCCESS);
            }
            let mut t = Table::new(&["ID", "KIND", "VERSION", "REVIEWED", "DESCRIPTION"]);
            for e in list {
                t.row(vec![
                    style.bold(&e.id),
                    e.kind.label().into(),
                    e.version.clone(),
                    if e.reviewed {
                        style.green("yes")
                    } else {
                        style.dim("no")
                    },
                    e.description.clone(),
                ]);
            }
            println!("{}", t.render(style));
            Ok(ExitCode::SUCCESS)
        }
        MarketAction::Install { id, yes } => {
            let Some(entry) = index.entries.iter().find(|e| e.id == id) else {
                bail!("no marketplace entry {id:?}");
            };
            let path = market::download(entry)?;
            println!("{} Downloaded {} (SHA-256 verified)", style.ok(), entry.id);
            match entry.kind {
                EntryKind::Project => {
                    println!(
                        "  Unpack it with: {}",
                        style.bold(format!("ptk unpack {}", path.display()))
                    );
                }
                EntryKind::Sdk => {
                    return sdk(style, SdkAction::Install { dir: path, yes });
                }
                EntryKind::Plugin => {
                    return crate::plugin::run(
                        style,
                        crate::PluginAction::Install { dir: path, yes },
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

pub fn publish(style: &Style, path: &Path, out: &Path) -> anyhow::Result<ExitCode> {
    let mut engine = EngineClient::start()?;
    let p = market::package(path, out, &mut engine)?;
    engine.shutdown()?;
    for w in &p.warnings {
        println!("  {} {w}", style.warn());
    }
    println!(
        "{} {} {} is ready to publish",
        style.ok(),
        p.entry.kind.label(),
        style.bold(&p.entry.id)
    );
    println!("  file    {}", p.archive.display());
    println!("  sha256  {}", p.entry.sha256);
    println!("{}", style.heading("Marketplace entry"));
    println!("{}", serde_json::to_string_pretty(&p.entry)?);
    println!("{}", style.heading("Next steps"));
    println!("  1. Upload the file (for example to a GitHub release of your repository).");
    println!("  2. Put its URL in \"download\".");
    println!("  3. Open a pull request adding the entry to index.json in {MARKETPLACE_REPO}");
    println!("     Reviewed entries are marked \"reviewed\" when merged.");
    Ok(ExitCode::SUCCESS)
}
