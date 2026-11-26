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
    if let MarketAction::Submit {
        path,
        store,
        output,
        dry_run,
        yes,
    } = action
    {
        return submit(style, &path, store.as_deref(), &output, dry_run, yes);
    }
    if let MarketAction::Sources { add, remove, reset } = action {
        return sources(style, add, remove, reset);
    }
    let urls = market::index_urls();
    let (index, problems) = market::fetch_all(&urls).map_err(|e| {
        anyhow::anyhow!(
            "{e}\nThe marketplace is read from {} (repository {MARKETPLACE_REPO}); `ptk market sources --add URL` adds a mirror.",
            urls.join(", ")
        )
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
        MarketAction::Check { id, game, json } => {
            let Some(entry) = index.entries.iter().find(|e| e.id == id) else {
                bail!("no marketplace entry {id:?}");
            };
            let check = market::check_copy(entry, &game)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&check)?);
            } else {
                let mark = match check.verdict {
                    market::CopyVerdict::Matches => style.ok(),
                    market::CopyVerdict::OtherVersion | market::CopyVerdict::Unknown => {
                        style.warn()
                    }
                    _ => style.fail(),
                };
                println!(
                    "{mark} {} ({} of {} files identical)",
                    check.verdict.label(),
                    check.matched,
                    check.total
                );
                for p in &check.different {
                    println!("  {} {p}", style.dim("different:"));
                }
                for p in &check.missing {
                    println!("  {} {p}", style.dim("not found:"));
                }
                if check.verdict == market::CopyVerdict::Unknown {
                    println!(
                        "  {}",
                        style.dim(
                            "This entry has no fingerprint (published before checks existed)."
                        )
                    );
                }
            }
            Ok(if check.verdict == market::CopyVerdict::Matches {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        MarketAction::Outdated { projects, json } => {
            let folder =
                projects.unwrap_or_else(|| ptk_core::settings::AppSettings::load().projects_dir());
            let items = market::installed(&market::projects_in(&folder));
            let updates = market::outdated(&index, &items);
            if json {
                println!("{}", serde_json::to_string_pretty(&updates)?);
                return Ok(ExitCode::SUCCESS);
            }
            if updates.is_empty() {
                println!(
                    "{} Everything is up to date ({} installed items checked)",
                    style.ok(),
                    items.len()
                );
                return Ok(ExitCode::SUCCESS);
            }
            let mut t = Table::new(&["ID", "KIND", "INSTALLED", "AVAILABLE"]);
            for u in &updates {
                t.row(vec![
                    style.bold(&u.id),
                    u.kind.label().into(),
                    u.installed.clone(),
                    style.green(&u.available),
                ]);
            }
            println!("{}", t.render(style));
            println!(
                "  {}",
                style.dim("Update a plugin or SDK with: ptk market install <id>. A port update is a new project folder.")
            );
            Ok(ExitCode::SUCCESS)
        }
        MarketAction::Submit { .. } | MarketAction::Sources { .. } => unreachable!("handled above"),
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

/// `ptk market submit`: package, then release + pull request with gh.
fn submit(
    style: &Style,
    path: &Path,
    store: Option<&str>,
    out: &Path,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<ExitCode> {
    use ptk_core::submit::{self, GhCli};
    let mut gh = GhCli::find().ok_or_else(|| {
        anyhow::anyhow!("the GitHub CLI (gh) is needed: brew install gh, then gh auth login")
    })?;
    let mut engine = EngineClient::start()?;
    let p = market::package(path, out, &mut engine)?;
    engine.shutdown()?;
    let plan = submit::plan(&p, p.poster_file.as_deref(), store, &mut gh)?;
    println!("{}", style.heading("Publishing to the marketplace"));
    println!(
        "  {:<9} {} {} ({})",
        "entry",
        plan.entry.kind.label(),
        style.bold(&plan.entry.id),
        plan.entry.version
    );
    println!("  {:<9} {}", "as", plan.login);
    println!(
        "  {:<9} {} in {}{}",
        "release",
        plan.tag,
        plan.work_repo,
        if plan.fork { " (your fork)" } else { "" }
    );
    for a in &plan.assets {
        println!("  {:<9} {}", "upload", a.display());
    }
    println!("  {:<9} {} → {}", "pull req.", plan.branch, plan.upstream);
    println!(
        "{}",
        style.dim("  The release and the pull request are public.")
    );
    if dry_run {
        println!("{}", serde_json::to_string_pretty(&plan.entry)?);
        println!("{} Dry run: nothing was published.", style.ok());
        return Ok(ExitCode::SUCCESS);
    }
    if !yes {
        use std::io::{BufRead, Write};
        print!("Publish it? [y/N] ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if !matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Nothing was published.");
            return Ok(ExitCode::FAILURE);
        }
    }
    let url = submit::submit(&plan, &mut gh, |step| println!("  {}", style.dim(step)))?;
    println!("{} Pull request opened: {}", style.ok(), style.bold(&url));
    println!(
        "  {}",
        style.dim("It appears in the marketplace once a maintainer merges it.")
    );
    Ok(ExitCode::SUCCESS)
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
    println!(
        "  Publish it in one step: {}",
        style.bold(format!("ptk market submit {}", path.display()))
    );
    println!("  (or upload the file yourself, put its URL in \"download\" and open a pull request");
    println!("  adding the entry to index.json in {MARKETPLACE_REPO})");
    Ok(ExitCode::SUCCESS)
}

/// `ptk market sources`: list, add or remove marketplace indexes.
fn sources(
    style: &Style,
    add: Option<String>,
    remove: Option<String>,
    reset: bool,
) -> anyhow::Result<ExitCode> {
    let mut settings = ptk_core::settings::AppSettings::load();
    let before = settings.marketplace_sources.clone();
    if reset {
        settings.marketplace_sources.clear();
    }
    if let Some(url) = remove {
        let url = url.trim().to_string();
        if !settings.marketplace_sources.contains(&url) {
            anyhow::bail!("{url} is not a marketplace source");
        }
        settings.marketplace_sources.retain(|s| *s != url);
    }
    if let Some(url) = add {
        let url = url.trim().to_string();
        if !market::valid_source(&url) {
            anyhow::bail!("{url}: a source is an https:// (or file://) URL of an index.json");
        }
        if settings.marketplace_sources.is_empty() {
            settings
                .marketplace_sources
                .push(market::DEFAULT_INDEX_URL.into());
        }
        if !settings.marketplace_sources.contains(&url) {
            settings.marketplace_sources.push(url);
        }
    }
    if settings.marketplace_sources != before {
        settings.save()?;
        println!("{} saved", style.ok());
    }
    println!("{}", style.heading("Marketplace sources"));
    for url in market::index_urls_from(&settings) {
        println!("  {url}");
    }
    if std::env::var_os("PTK_MARKETPLACE_URL").is_some() {
        println!(
            "  {}",
            style.dim("PTK_MARKETPLACE_URL is set: only that index is read.")
        );
    }
    Ok(ExitCode::SUCCESS)
}
