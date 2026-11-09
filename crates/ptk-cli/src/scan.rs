//! `ptk scan`: rendered from the engine's `scan` result.

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use ptk_core::engine::EngineClient;
use ptk_core::import::prepare;
use ptk_core::manifest::MANIFEST_FILE;
use ptk_core::routes::{RouteKind, route_for};
use ptk_core::scan::{Detected, ScanResult, record, scan};

use crate::style::{Style, Table, human_size};

fn bar(style: &Style, share: f64, width: usize) -> String {
    let filled = ((share * width as f64).round() as usize).min(width);
    format!(
        "{}{}",
        style.paint("█".repeat(filled), "38;5;214"),
        style.dim("░".repeat(width - filled))
    )
}

fn engine_line(style: &Style, d: &Detected) -> String {
    let variant = d
        .variant
        .as_ref()
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    let label = match d.label.as_str() {
        "high" => style.green("high"),
        "medium" => style.yellow("medium"),
        _ => style.dim("low"),
    };
    format!(
        "{}{}  {} confidence {}",
        style.bold(&d.name),
        variant,
        label,
        style.dim(format!("({:.2})", d.confidence))
    )
}

pub fn run(
    style: &Style,
    game_dir: &Path,
    json_out: bool,
    project: Option<&Path>,
) -> anyhow::Result<ExitCode> {
    let prepared = prepare(game_dir)?;
    if let Some(dir) = &prepared.extracted
        && !json_out
    {
        eprintln!(
            "  unpacked the {} into {}",
            prepared.kind.describe(),
            dir.display()
        );
    }
    let game_dir = prepared.game.as_path();
    let mut engine = EngineClient::start()?;
    let show_progress = !json_out && std::io::stderr().is_terminal();
    let (result, raw) = scan(&mut engine, game_dir, &mut |n| {
        if show_progress {
            eprint!("\r  scanned {n} files");
        }
    })?;
    if show_progress {
        eprint!("\r\x1b[2K");
    }
    engine.shutdown()?;

    if json_out {
        println!("{}", serde_json::to_string_pretty(&raw)?);
    } else {
        print_scan(style, &result, game_dir);
    }
    if let Some(project) = project {
        if !json_out {
            eprintln!(
                "  hashing {} files ({})...",
                result.file_count,
                human_size(result.total_size)
            );
        }
        let r = record(project, game_dir, &result)?;
        let msg = format!(
            "{} Recorded {} file hashes and the engine into {}",
            style.ok(),
            r.files,
            r.manifest_path.display()
        );
        if json_out {
            eprintln!("{msg}")
        } else {
            println!("{msg}")
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `shown` is the folder as the user typed it (the JSON has the full path).
pub fn print_scan(style: &Style, scan: &ScanResult, shown: &Path) {
    println!("{}", style.heading(&format!("Scan  {}", shown.display())));
    let row = |k: &str, v: String| println!("  {} {v}", style.dim(format!("{k:<10}")));
    match &scan.primary {
        Some(p) => {
            row(
                if p.kind == "engine" {
                    "Engine"
                } else {
                    "Format"
                },
                engine_line(style, p),
            );
            let more = if p.evidence.len() > 3 {
                format!(" +{} more", p.evidence.len() - 3)
            } else {
                String::new()
            };
            row(
                "",
                style.dim(format!(
                    "evidence: {}{more}",
                    p.evidence[..p.evidence.len().min(3)].join(", ")
                )),
            );
        }
        None => row("Engine", style.yellow("not recognised")),
    }
    for other in scan.engines.iter().skip(1) {
        row("Also", engine_line(style, other));
    }
    row(
        "Platforms",
        if scan.platforms.is_empty() {
            style.dim("unknown")
        } else {
            scan.platforms.join(", ")
        },
    );
    row(
        "Contents",
        format!(
            "{} files in {} folders, {}",
            scan.file_count,
            scan.dir_count,
            style.bold(human_size(scan.total_size))
        ),
    );
    if scan.symlinks_skipped > 0 || scan.unreadable > 0 {
        row(
            "Skipped",
            format!(
                "{} symbolic links, {} unreadable",
                scan.symlinks_skipped, scan.unreadable
            ),
        );
    }
    if scan.truncated {
        row("", style.yellow("stopped early: too many files"));
    }

    let mut t = Table::new(&["CATEGORY", ">FILES", ">SIZE", "SHARE", ">"]);
    for c in scan.categories_by_size() {
        let share = scan.share(c);
        t.row(vec![
            c.id.clone(),
            c.files.to_string(),
            human_size(c.size),
            bar(style, share, 24),
            format!("{:.0}%", share * 100.0),
        ]);
    }
    println!(
        "{}",
        style.heading(if scan.total_size > 0 {
            "Assets by size"
        } else {
            "Assets by file count"
        })
    );
    println!("{}", t.render(style));
    if !scan.largest.is_empty() {
        println!("{}", style.heading("Largest files"));
        let mut t = Table::new(&["FILE", "CATEGORY", ">SIZE"]);
        for f in &scan.largest {
            t.row(vec![
                f.path.clone(),
                style.dim(&f.category),
                human_size(f.size),
            ]);
        }
        println!("{}", t.render(style));
    }
    print_route(style, scan);
    println!();
    println!(
        "  {}",
        style.dim(
            "Detected from file names and headers only; no game archive was opened or decoded."
        )
    );
}

/// What it takes to port this engine, with or without source code.
pub fn print_route(style: &Style, scan: &ScanResult) {
    let engine = scan.primary.as_ref().map(|p| p.id.as_str());
    if let Some(r) = route_for(engine) {
        println!("{}", style.heading("Porting route"));
        let label = match r.kind {
            RouteKind::Retarget | RouteKind::Native => style.green(r.kind.label()),
            RouteKind::Partial => style.yellow(r.kind.label()),
            RouteKind::NeedsSource => style.red(r.kind.label()),
        };
        println!("  {}  {label}", style.bold(r.name));
        println!("  {}", r.how);
    }
}

/// `ptk import`: a project from a game build.
pub fn import(style: &Style, build: &Path, project: Option<&Path>) -> anyhow::Result<ExitCode> {
    let prepared = prepare(build)?;
    println!(
        "{} {} ({})",
        style.ok(),
        prepared.title,
        prepared.kind.describe()
    );
    if let Some(dir) = &prepared.extracted {
        println!("  unpacked into {}", dir.display());
    }
    let mut engine = EngineClient::start()?;
    let (result, _) = scan(&mut engine, &prepared.game, &mut |_| {})?;
    engine.shutdown()?;
    let project = project.map(Path::to_path_buf).unwrap_or_else(|| {
        std::path::PathBuf::from(format!(
            "{}-port",
            ptk_core::manifest::slugify(&prepared.title)
        ))
    });
    if !project.join(MANIFEST_FILE).exists() {
        let registry = ptk_core::targets::TargetRegistry::load_default().0;
        ptk_core::project::init(
            &project,
            ptk_core::project::InitOptions {
                title: Some(prepared.title.clone()),
                ..Default::default()
            },
            &registry,
        )?;
        println!("{} Created project {}", style.ok(), project.display());
    }
    let r = record(&project, &prepared.game, &result)?;
    println!(
        "{} Recorded {} file hashes{}",
        style.ok(),
        r.files,
        r.engine
            .as_ref()
            .map(|e| format!(" and the engine ({e})"))
            .unwrap_or_default()
    );
    print_route(style, &result);
    println!();
    println!(
        "  Next: {}",
        style.bold(format!("ptk info {}", project.display()))
    );
    Ok(ExitCode::SUCCESS)
}
