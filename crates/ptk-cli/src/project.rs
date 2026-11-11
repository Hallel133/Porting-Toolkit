//! Project commands: about, init, info, options, targets, pack, unpack, verify.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use ptk_core::category::{CATEGORIES, category};
use ptk_core::manifest::{CategoryOptions, Level, MANIFEST_FILE, Manifest};
use ptk_core::pack::{PackOptions, default_pack_name, load_any_manifest, read_pack_manifest};
use ptk_core::project::InitOptions;
use ptk_core::targets::{TargetRegistry, TargetSource, TargetStatus};
use ptk_core::{paths, verify::verify_game};

use crate::OptionsAction;
use crate::banner;
use crate::style::{Style, Table, human_size};

fn registry(style: &Style) -> TargetRegistry {
    let (reg, errors) = TargetRegistry::load_default();
    for e in errors {
        eprintln!("{} {e}", style.warn());
    }
    reg
}

fn print_issues(style: &Style, issues: &[ptk_core::manifest::Issue]) {
    for i in issues {
        let mark = if i.level == Level::Error {
            style.fail()
        } else {
            style.warn()
        };
        println!("  {mark} {}", i.message);
    }
}

pub fn about(style: &Style) -> anyhow::Result<ExitCode> {
    print!("{}", banner::banner(style));
    println!("{}", style.heading("About"));
    println!("  A community tool to manage, convert, export and import game projects");
    println!("  between platforms. macOS first; Windows and Linux later.");
    println!();
    println!("  Version     {}", ptk_core::VERSION);
    println!("  License     MIT");
    println!("  Source      {}", ptk_core::REPOSITORY);
    println!("  Data        {}", paths::data_dir().display());
    println!("{}", style.heading("Roadmap"));
    let phases = [
        (
            "1",
            "macOS + CLI infrastructure, banner, project format",
            false,
        ),
        ("2", "Desktop GUI", false),
        (
            "3",
            "Import/export plugins (Godot, HTML5 → macOS, web), then AI-assisted options",
            true,
        ),
        (
            "4",
            "Windows/Linux and the full community marketplace",
            false,
        ),
    ];
    for (n, text, now) in phases {
        let marker = if now {
            style.green("● now ")
        } else {
            style.dim("○ next")
        };
        println!("  {marker}  Phase {n}: {text}");
    }
    println!("{}", style.heading("Principles"));
    println!("  • A tool, not a distribution channel: you bring your own game.");
    println!("  • .ptk project files hold recipes, settings, community patches and");
    println!("    SHA-256 hashes. Never original game files or decompiled game code.");
    println!("  • You choose what is kept, dropped or recompressed before porting.");
    Ok(ExitCode::SUCCESS)
}

pub fn init(
    style: &Style,
    dir: &Path,
    name: Option<String>,
    title: Option<String>,
    targets: Vec<String>,
) -> anyhow::Result<ExitCode> {
    let reg = registry(style);
    let m = ptk_core::project::init(
        dir,
        {
            let mut o = InitOptions::with_defaults(title);
            o.name = name;
            if !targets.is_empty() {
                o.targets = targets;
            }
            o
        },
        &reg,
    )?;
    let name = m.name;
    println!(
        "{} Created project {} in {}",
        style.ok(),
        style.bold(&name),
        dir.display()
    );
    println!();
    for rel in ptk_core::project::CREATED {
        println!("  {}", style.cyan(rel));
    }
    let d = dir.display();
    println!("{}", style.heading("Next steps"));
    println!(
        "  1. {}",
        style.bold(format!("ptk scan /path/to/your/game --record {d}"))
    );
    println!(
        "  2. {}",
        style.bold(format!(
            "ptk options set textures --action recompress --quality 80 -p {d}"
        ))
    );
    println!("  3. {}", style.bold(format!("ptk pack {d}")));
    Ok(ExitCode::SUCCESS)
}

fn options_table(style: &Style, m: &Manifest) -> Table {
    let mut t = Table::new(&["CATEGORY", "ACTION", ">QUALITY", ">MAX RES", "FORMAT"]);
    let dash = || style.dim("-");
    for c in CATEGORIES {
        let o = m.options_for(c.id);
        let action = match o.action {
            ptk_core::manifest::AssetAction::Keep => style.green("keep"),
            ptk_core::manifest::AssetAction::Drop => style.red("drop"),
            ptk_core::manifest::AssetAction::Recompress => style.yellow("recompress"),
        };
        t.row(vec![
            c.id.to_owned(),
            action,
            o.quality.map(|q| q.to_string()).unwrap_or_else(dash),
            o.max_resolution
                .map(|r| format!("{r} px"))
                .unwrap_or_else(dash),
            o.format.clone().unwrap_or_else(dash),
        ]);
    }
    for (id, _) in m.options.iter().filter(|(id, _)| category(id).is_none()) {
        t.row(vec![
            style.yellow(id),
            style.dim("(unknown category)"),
            dash(),
            dash(),
            dash(),
        ]);
    }
    t
}

pub fn info(style: &Style, project: &Path, json: bool) -> anyhow::Result<ExitCode> {
    let m = load_any_manifest(project)?;
    let reg = registry(style);
    if json {
        println!("{}", m.to_json().trim_end());
        return Ok(ExitCode::SUCCESS);
    }
    let is_dir = project.is_dir();
    let none = || style.dim("(not set)");
    println!("{}", style.heading(&format!("{} {}", m.name, m.version)));
    let row = |k: &str, v: String| println!("  {} {v}", style.dim(format!("{k:<10}")));
    row("Source", project.display().to_string());
    row("Schema", m.schema_version.to_string());
    if !m.description.is_empty() {
        row("About", m.description.clone());
    }
    if !m.authors.is_empty() {
        row("Authors", m.authors.join(", "));
    }
    row(
        "License",
        format!("{} {}", m.license, style.dim("(community work)")),
    );
    row("Game", m.game.title.clone().unwrap_or_else(none));
    row("Engine", m.game.engine.clone().unwrap_or_else(none));
    if let Some(p) = &m.game.source_platform {
        row("Platform", p.clone());
    }
    let content = match &m.game.content_license {
        Some(l) if m.allows_redistribution() => format!("{l} {}", style.green("(may be packed)")),
        Some(l) => format!("{l} {}", style.dim("(game files are never packed)")),
        None => style.dim("not declared (game files are never packed)"),
    };
    row("Content", content);
    let targets = if m.targets.is_empty() {
        none()
    } else {
        m.targets
            .iter()
            .map(|t| match reg.get(t) {
                Some(tt) => format!("{t} {}", style.dim(format!("({}, {})", tt.name, tt.status))),
                None => format!("{t} {}", style.yellow("(unknown)")),
            })
            .collect::<Vec<_>>()
            .join("\n             ")
    };
    row("Targets", targets);
    let total: u64 = m.source_files.iter().map(|f| f.size).sum();
    row(
        "Hashes",
        if m.source_files.is_empty() {
            style.dim("none recorded (ptk scan GAME --record PROJECT)")
        } else {
            format!("{} game files, {}", m.source_files.len(), human_size(total))
        },
    );
    if is_dir {
        let c = ptk_core::project::contents(project)?;
        row(
            "Contents",
            format!(
                "{} recipes, {} patches, {} locked plugins",
                c.recipes, c.patches, c.locked_plugins
            ),
        );
    }
    if !m.third_party.is_empty() {
        row("3rd-party", m.third_party.join(", "));
    }

    println!(
        "{}",
        style.heading("Advanced options (applied before porting)")
    );
    println!("{}", options_table(style, &m).render(style));
    println!("  {}", style.dim("Change with: ptk options set CATEGORY --action keep|drop|recompress [--quality N] [--max-resolution PX]"));

    let issues = m.validate(&reg);
    if !issues.is_empty() {
        println!("{}", style.heading("Issues"));
        print_issues(style, &issues);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn options(
    style: &Style,
    project: &Path,
    action: Option<OptionsAction>,
) -> anyhow::Result<ExitCode> {
    let mut m = Manifest::load(project)?;
    if let Some(OptionsAction::Set {
        category: cat,
        action,
        quality,
        max_resolution,
        format,
        reset,
    }) = action
    {
        let Some(c) = category(&cat) else {
            let ids: Vec<_> = CATEGORIES.iter().map(|c| c.id).collect();
            bail!("unknown category {cat:?}; use one of: {}", ids.join(", "));
        };
        let mut o = if reset {
            CategoryOptions::default()
        } else {
            m.options_for(c.id)
        };
        if let Some(a) = action {
            o.action = a.parse().map_err(anyhow::Error::msg)?;
        }
        if quality.is_some() {
            o.quality = quality;
        }
        if max_resolution.is_some() {
            o.max_resolution = max_resolution;
        }
        if format.is_some() {
            o.format = format.filter(|f| !f.is_empty());
        }
        m.options.insert(c.id.to_owned(), o);
        let reg = registry(style);
        let path = project.join(MANIFEST_FILE);
        let warnings = m.check(&path, &reg)?;
        m.save(project)?;
        println!(
            "{} Updated {} in {}",
            style.ok(),
            style.bold(c.id),
            path.display()
        );
        print_issues(style, &warnings);
    }
    println!(
        "{}",
        style.heading("Advanced options (applied before porting)")
    );
    println!("{}", options_table(style, &m).render(style));
    Ok(ExitCode::SUCCESS)
}

pub fn targets(style: &Style, json: bool) -> anyhow::Result<ExitCode> {
    let reg = registry(style);
    if json {
        let all: Vec<_> = reg.iter().collect();
        println!("{}", serde_json::to_string_pretty(&all)?);
        return Ok(ExitCode::SUCCESS);
    }
    let host = reg.host().map(|t| t.id.clone());
    let mut t = Table::new(&["ID", "NAME", "OS / ARCH", "STATUS", "NOTES"]);
    for target in reg.iter() {
        let status = match target.status {
            TargetStatus::Active => style.green("active"),
            TargetStatus::Experimental => style.yellow("experimental"),
            TargetStatus::Planned => style.dim("planned"),
        };
        let mut notes = target.description.clone();
        if host.as_deref() == Some(&target.id) {
            notes = format!("{} {notes}", style.cyan("(this machine)"));
        }
        if let TargetSource::File(p) = &target.source {
            notes = format!("{notes} {}", style.dim(format!("[{}]", p.display())));
        }
        t.row(vec![
            style.bold(&target.id),
            target.name.clone(),
            format!("{} / {}", target.os, target.arch),
            status,
            notes,
        ]);
    }
    println!("{}", style.heading("Targets"));
    println!("{}", t.render(style));
    println!();
    println!(
        "  {}",
        style.dim(format!(
            "More targets (e.g. from an SDK) can be added as [[target]] TOML files in {}",
            paths::targets_dir().display()
        ))
    );
    Ok(ExitCode::SUCCESS)
}

pub fn pack(
    style: &Style,
    project: &Path,
    output: Option<PathBuf>,
    force: bool,
) -> anyhow::Result<ExitCode> {
    let reg = registry(style);
    let m = Manifest::load(project)?;
    let output = output.unwrap_or_else(|| PathBuf::from(default_pack_name(&m)));
    let report = ptk_core::pack::pack(project, &output, &reg, PackOptions { overwrite: force })?;
    print_issues(style, &report.warnings);
    let mut t = Table::new(&["FILE", ">SIZE", "SHA-256"]);
    for e in &report.entries {
        t.row(vec![
            e.path.clone(),
            human_size(e.size),
            style.dim(&e.sha256[..16]),
        ]);
    }
    println!(
        "{}",
        style.heading(&format!(
            "Packed {} {}",
            report.manifest.name, report.manifest.version
        ))
    );
    println!("{}", t.render(style));
    if !report.licensed_third_party.is_empty() {
        println!();
        println!(
            "  {} included third-party content under {}:",
            style.warn(),
            report
                .manifest
                .game
                .content_license
                .as_deref()
                .unwrap_or("?")
        );
        for f in &report.licensed_third_party {
            println!("    - {f}");
        }
    }
    println!();
    println!(
        "{} {} ({} files, {})",
        style.ok(),
        style.bold(report.output.display()),
        report.entries.len(),
        human_size(report.bytes)
    );
    println!(
        "  {}",
        style.dim("Contains recipes, settings, patches and hashes only: no game files.")
    );
    Ok(ExitCode::SUCCESS)
}

pub fn unpack(style: &Style, pack: &Path, output: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let dest = match output {
        Some(o) => o,
        None => PathBuf::from(read_pack_manifest(pack)?.name),
    };
    let report = ptk_core::pack::unpack(pack, &dest)?;
    println!(
        "{} Unpacked {} {} into {} ({} files, all checksums match)",
        style.ok(),
        style.bold(&report.manifest.name),
        report.manifest.version,
        report.dest.display(),
        report.entries.len()
    );
    if !report.manifest.source_files.is_empty() {
        println!(
            "  Next: {}",
            style.bold(format!(
                "ptk verify /path/to/your/game -p {}",
                report.dest.display()
            ))
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn verify(
    style: &Style,
    game_dir: &Path,
    project: &Path,
    json: bool,
) -> anyhow::Result<ExitCode> {
    let m = load_any_manifest(project)?;
    if m.source_files.is_empty() {
        bail!(
            "{} has no recorded game hashes; record them with `ptk scan GAME --record PROJECT`",
            project.display()
        );
    }
    let show_progress = !json && std::io::stderr().is_terminal();
    let report = verify_game(game_dir, &m, |done, total| {
        if show_progress && (done % 25 == 0 || done == total) {
            eprint!("\r  checking {done}/{total} files");
        }
    })
    .with_context(|| format!("verifying {}", game_dir.display()))?;
    if show_progress {
        eprint!("\r\x1b[2K");
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(if report.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    println!(
        "{}",
        style.heading(&format!(
            "Verify {} against {} {}",
            game_dir.display(),
            m.name,
            m.version
        ))
    );
    for f in &report.missing {
        println!("  {} missing    {f}", style.fail());
    }
    for f in &report.mismatched {
        let why = if f.actual_sha256.is_none() {
            format!("size {} instead of {}", f.actual_size, f.expected_size)
        } else {
            "different contents".into()
        };
        println!(
            "  {} different  {}  {}",
            style.fail(),
            f.path,
            style.dim(why)
        );
    }
    for f in &report.skipped {
        println!(
            "  {} skipped    {f} {}",
            style.warn(),
            style.dim("(unsafe path in manifest)")
        );
    }
    if report.is_ok() {
        println!(
            "  {} all {} files match: this is the game version the project was made for",
            style.ok(),
            report.ok
        );
        Ok(ExitCode::SUCCESS)
    } else {
        println!();
        println!(
            "  {} {} of {} files match, {} different, {} missing",
            style.fail(),
            report.ok,
            report.checked,
            report.mismatched.len(),
            report.missing.len()
        );
        Ok(ExitCode::FAILURE)
    }
}
