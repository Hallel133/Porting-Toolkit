//! `ptk scan`: rendered from the Python engine's `scan` result.

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use ptk_core::engine::EngineClient;
use ptk_core::hash::hash_tree;
use ptk_core::manifest::Manifest;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::style::{Style, Table, human_size};

#[derive(Deserialize)]
pub struct ScanResult {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub symlinks_skipped: u64,
    pub unreadable: u64,
    pub truncated: bool,
    pub engines: Vec<Detected>,
    pub primary: Option<Detected>,
    pub platforms: Vec<String>,
    pub categories: Vec<CategoryStat>,
    pub largest: Vec<LargeFile>,
}

#[derive(Clone, Deserialize)]
pub struct Detected {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub confidence: f64,
    pub label: String,
    pub variant: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Deserialize)]
pub struct CategoryStat {
    pub id: String,
    pub files: u64,
    pub size: u64,
}

#[derive(Deserialize)]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
    pub category: String,
}

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
    record: Option<&Path>,
) -> anyhow::Result<ExitCode> {
    let abs = game_dir
        .canonicalize()
        .with_context(|| format!("{} does not exist", game_dir.display()))?;
    let mut engine = EngineClient::start()?;
    let show_progress = !json_out && std::io::stderr().is_terminal();
    let raw = engine.call_with(
        "scan",
        json!({ "path": abs.to_string_lossy() }),
        &mut |method, params: &Value| {
            if show_progress && method == "progress" {
                eprint!("\r  scanned {} files", params["files"]);
            }
        },
    )?;
    if show_progress {
        eprint!("\r\x1b[2K");
    }
    engine.shutdown()?;

    if json_out && record.is_none() {
        println!("{}", serde_json::to_string_pretty(&raw)?);
        return Ok(ExitCode::SUCCESS);
    }
    let scan: ScanResult = serde_json::from_value(raw.clone()).context("unexpected scan result")?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&raw)?);
    } else {
        print_scan(style, &scan, game_dir);
    }
    if let Some(project) = record {
        record_into(style, project, &abs, &scan, json_out)?;
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
            row(
                "",
                style.dim(format!(
                    "evidence: {}{}",
                    p.evidence
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    if p.evidence.len() > 3 {
                        format!(" +{} more", p.evidence.len() - 3)
                    } else {
                        String::new()
                    }
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

    let mut cats: Vec<&CategoryStat> = scan.categories.iter().collect();
    cats.sort_by(|a, b| b.size.cmp(&a.size).then(b.files.cmp(&a.files)));
    let by_size = scan.total_size > 0;
    let total = if by_size {
        scan.total_size
    } else {
        scan.file_count.max(1)
    } as f64;
    let mut t = Table::new(&["CATEGORY", ">FILES", ">SIZE", "SHARE", ">"]);
    for c in cats {
        let share = if by_size { c.size } else { c.files } as f64 / total;
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
        style.heading(if by_size {
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
    println!();
    println!(
        "  {}",
        style.dim(
            "Detected from file names and headers only; no game archive was opened or decoded."
        )
    );
}

fn record_into(
    style: &Style,
    project: &Path,
    game: &Path,
    scan: &ScanResult,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut m = Manifest::load(project)?;
    if !quiet {
        eprintln!(
            "  hashing {} files ({})...",
            scan.file_count,
            human_size(scan.total_size)
        );
    }
    let files = hash_tree(game).with_context(|| format!("hashing {}", game.display()))?;
    if let Some(p) = scan.primary.as_ref().filter(|p| p.kind == "engine") {
        m.game.engine = Some(p.id.clone());
    }
    if m.game.title.is_none() {
        m.game.title = game.file_name().map(|n| n.to_string_lossy().into_owned());
    }
    if let [platform] = scan.platforms.as_slice() {
        m.game.source_platform = Some(platform.clone());
    }
    let count = files.len();
    m.source_files = files;
    let path = m.save(project)?;
    let msg = format!(
        "{} Recorded {} file hashes and the engine into {}",
        style.ok(),
        count,
        path.display()
    );
    if quiet {
        eprintln!("{msg}")
    } else {
        println!("{msg}")
    }
    Ok(())
}
