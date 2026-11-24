//! `ptk market submit`: publishes a packaged plugin, SDK or game port to
//! the marketplace in one step, with the GitHub CLI (`gh`):
//!
//! 1. the file (and a port's poster) goes into a GitHub release, of the
//!    marketplace repository itself for its owner or of the user's fork;
//! 2. the entry, with those download URLs filled in, is written to
//!    `index.json` on a new branch;
//! 3. a pull request to the marketplace is opened for review.
//!
//! [`plan`] works out every name and URL without changing anything;
//! [`submit`] carries it out. Both run `gh` through [`Gh`], so tests can
//! stand in for GitHub.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::market::{EntryKind, MARKETPLACE_REPO, MarketEntry, MarketIndex, Published};

/// Runs `gh` with arguments and returns its standard output.
pub trait Gh {
    fn run(&mut self, args: &[String]) -> Result<String>;
}

/// The real GitHub CLI.
pub struct GhCli {
    pub exe: PathBuf,
}

impl GhCli {
    pub fn find() -> Option<Self> {
        crate::engine::which("gh")
            .or_else(|| {
                ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.is_file())
            })
            .map(|exe| GhCli { exe })
    }
}

impl Gh for GhCli {
    fn run(&mut self, args: &[String]) -> Result<String> {
        let out = Command::new(&self.exe)
            .args(args)
            .output()
            .map_err(|e| Error::Invalid(format!("could not run gh: {e}")))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(Error::Invalid(format!(
                "gh {} failed: {}",
                args.first().map(String::as_str).unwrap_or(""),
                err.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

/// Everything a submission will do, for showing before it happens.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SubmitPlan {
    pub login: String,
    /// The marketplace, `owner/name`.
    pub upstream: String,
    /// Where the release and the branch go: the marketplace or a fork.
    pub work_repo: String,
    pub fork: bool,
    pub tag: String,
    pub branch: String,
    pub assets: Vec<PathBuf>,
    pub entry: MarketEntry,
    pub title: String,
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// `owner/name` of the marketplace repository.
pub fn upstream_repo() -> String {
    MARKETPLACE_REPO
        .trim_end_matches('/')
        .trim_start_matches("https://github.com/")
        .to_owned()
}

/// The URL GitHub serves a release asset from.
pub fn release_url(repo: &str, tag: &str, file: &str) -> String {
    let encoded: String = file
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://github.com/{repo}/releases/download/{tag}/{encoded}")
}

/// Works out the release, branch and entry without changing anything.
pub fn plan(
    published: &Published,
    poster: Option<&Path>,
    store: Option<&str>,
    gh: &mut dyn Gh,
) -> Result<SubmitPlan> {
    let login = gh.run(&args(&["api", "user", "--jq", ".login"]))?;
    if login.is_empty() {
        return Err(Error::Invalid(
            "gh is not signed in: run gh auth login".into(),
        ));
    }
    let upstream = upstream_repo();
    let owner = upstream.split('/').next().unwrap_or_default();
    let fork = !login.eq_ignore_ascii_case(owner);
    let name = upstream.split('/').nth(1).unwrap_or("ptk-marketplace");
    let work_repo = if fork {
        format!("{login}/{name}")
    } else {
        upstream.clone()
    };
    let mut entry = published.entry.clone();
    let tag = format!(
        "{}-v{}",
        entry.id,
        entry
            .version
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            })
            .collect::<String>()
    );
    let file = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut assets = vec![published.archive.clone()];
    entry.download = release_url(&work_repo, &tag, &file(&published.archive));
    if entry.kind != EntryKind::Project {
        // Posters and store pages belong to game ports only.
        entry.poster = None;
        entry.store = None;
    } else {
        match poster {
            Some(p) => {
                entry.poster = Some(release_url(&work_repo, &tag, &file(p)));
                assets.push(p.to_owned());
            }
            None => entry.poster = None,
        }
        match store {
            Some(s) => entry.store = Some(s.to_owned()),
            None => {
                return Err(Error::Invalid(
                    "a game port needs a store page where players get the game: pass --store https://...".into(),
                ));
            }
        }
    }
    let problems = entry.problems();
    if !problems.is_empty() {
        return Err(Error::Invalid(format!(
            "the entry is not ready:\n  - {}",
            problems.join("\n  - ")
        )));
    }
    Ok(SubmitPlan {
        title: format!(
            "Add {} {} ({})",
            entry.name,
            entry.version,
            entry.kind.label()
        ),
        branch: format!("add-{tag}"),
        login,
        upstream,
        work_repo,
        fork,
        tag,
        assets,
        entry,
    })
}

/// Adds or replaces the entry (same kind and id) in an index file.
pub fn updated_index(text: &str, entry: &MarketEntry) -> Result<String> {
    let mut index: MarketIndex = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("the marketplace index is not valid JSON: {e}")))?;
    index
        .entries
        .retain(|e| !(e.kind == entry.kind && e.id == entry.id));
    let mut e = entry.clone();
    e.reviewed = false; // maintainers set it when they merge
    index.entries.push(e);
    index.entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(serde_json::to_string_pretty(&index).expect("index serializes") + "\n")
}

/// Carries out a plan. Returns the pull request's URL.
pub fn submit(plan: &SubmitPlan, gh: &mut dyn Gh, mut step: impl FnMut(&str)) -> Result<String> {
    let work = plan.work_repo.as_str();
    if plan.fork {
        step("Forking the marketplace");
        if let Err(e) = gh.run(&args(&["repo", "fork", &plan.upstream, "--clone=false"]))
            && !e.to_string().contains("already exists")
        {
            return Err(e);
        }
        step("Bringing the fork up to date");
        gh.run(&args(&["repo", "sync", work, "--source", &plan.upstream]))?;
    }
    step("Uploading the release");
    let mut release = args(&["release", "create", &plan.tag]);
    release.extend(plan.assets.iter().map(|p| p.to_string_lossy().into_owned()));
    release.extend(args(&[
        "--repo",
        work,
        "--title",
        &plan.title,
        "--notes",
        "Published with Porting Toolkit (ptk market submit).",
    ]));
    gh.run(&release)?;
    step("Opening a branch");
    let main = gh.run(&args(&[
        "api",
        &format!("repos/{work}/git/ref/heads/main"),
        "--jq",
        ".object.sha",
    ]))?;
    gh.run(&args(&[
        "api",
        "-X",
        "POST",
        &format!("repos/{work}/git/refs"),
        "-f",
        &format!("ref=refs/heads/{}", plan.branch),
        "-f",
        &format!("sha={main}"),
    ]))?;
    let path = format!("repos/{work}/contents/index.json");
    let current = gh.run(&args(&[
        "api",
        "-H",
        "Accept: application/vnd.github.raw",
        &format!("{path}?ref={}", plan.branch),
    ]))?;
    let blob = gh.run(&args(&[
        "api",
        &format!("{path}?ref={}", plan.branch),
        "--jq",
        ".sha",
    ]))?;
    step("Adding the entry to index.json");
    let text = updated_index(&current, &plan.entry)?;
    gh.run(&args(&[
        "api",
        "-X",
        "PUT",
        &path,
        "-f",
        &format!("message={}", plan.title),
        "-f",
        &format!("content={}", base64(text.as_bytes())),
        "-f",
        &format!("sha={blob}"),
        "-f",
        &format!("branch={}", plan.branch),
    ]))?;
    step("Opening the pull request");
    let head = if plan.fork {
        format!("{}:{}", plan.login, plan.branch)
    } else {
        plan.branch.clone()
    };
    let body = format!(
        "{}\n\n- Kind: {}\n- Version: {}\n- File: {}\n- SHA-256: `{}`\n\nPublished with Porting Toolkit.",
        if plan.entry.description.is_empty() {
            &plan.entry.name
        } else {
            &plan.entry.description
        },
        plan.entry.kind.label(),
        plan.entry.version,
        plan.entry.download,
        plan.entry.sha256,
    );
    gh.run(&args(&[
        "pr",
        "create",
        "--repo",
        &plan.upstream,
        "--base",
        "main",
        "--head",
        &head,
        "--title",
        &plan.title,
        "--body",
        &body,
    ]))
}

/// Standard base64 (GitHub's contents API wants it).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeGh {
        login: String,
        calls: Vec<Vec<String>>,
        index: String,
    }

    impl Gh for FakeGh {
        fn run(&mut self, args: &[String]) -> Result<String> {
            self.calls.push(args.to_vec());
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            Ok(match a.as_slice() {
                ["api", "user", ..] => self.login.clone(),
                ["api", p, "--jq", ".object.sha"] if p.ends_with("ref/heads/main") => {
                    "abc123".into()
                }
                ["api", "-H", ..] => self.index.clone(),
                ["api", _, "--jq", ".sha"] => "blob456".into(),
                ["pr", "create", ..] => {
                    "https://github.com/Hallel133/ptk-marketplace/pull/7".into()
                }
                _ => String::new(),
            })
        }
    }

    fn published(kind: EntryKind) -> Published {
        Published {
            kind,
            archive: PathBuf::from("/tmp/dist/moon port.ptk"),
            warnings: vec![],
            poster_file: None,
            entry: MarketEntry {
                id: "moon-port".into(),
                name: "Moon Garden".into(),
                kind,
                version: "1.2.0".into(),
                description: "Moon Garden for macOS".into(),
                author: "me".into(),
                license: "MIT".into(),
                homepage: None,
                download: crate::market::DOWNLOAD_PLACEHOLDER.into(),
                sha256: crate::hash::sha256_bytes(b"x"),
                reviewed: false,
                engines: vec!["renpy".into()],
                targets: vec!["macos-arm64".into()],
                game: Some("Moon Garden".into()),
                poster: Some("REPLACE".into()),
                store: Some("REPLACE".into()),
                fingerprint: vec![],
            },
        }
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn plan_for_the_owner_and_for_others() {
        let mut gh = FakeGh {
            login: "Hallel133".into(),
            calls: vec![],
            index: String::new(),
        };
        let p = plan(
            &published(EntryKind::Project),
            Some(Path::new("/tmp/dist/moon-port-poster.png")),
            Some("https://store.example.org/moon"),
            &mut gh,
        )
        .unwrap();
        assert!(!p.fork && p.work_repo == "Hallel133/ptk-marketplace");
        assert_eq!(p.tag, "moon-port-v1.2.0");
        assert_eq!(
            p.entry.download,
            "https://github.com/Hallel133/ptk-marketplace/releases/download/moon-port-v1.2.0/moon%20port.ptk"
        );
        assert_eq!(p.assets.len(), 2);
        assert!(
            p.entry
                .poster
                .as_deref()
                .unwrap()
                .ends_with("/moon-port-poster.png")
        );
        let mut other = FakeGh {
            login: "someone".into(),
            calls: vec![],
            index: String::new(),
        };
        let p = plan(&published(EntryKind::Plugin), None, None, &mut other).unwrap();
        assert!(p.fork && p.work_repo == "someone/ptk-marketplace");
        // A port without a store page is refused.
        let err = plan(&published(EntryKind::Project), None, None, &mut other).unwrap_err();
        assert!(err.to_string().contains("--store"));
    }

    #[test]
    fn submit_runs_the_steps_in_order() {
        let index = "{\"schema_version\":1,\"entries\":[]}";
        let mut gh = FakeGh {
            login: "someone".into(),
            calls: vec![],
            index: index.into(),
        };
        let p = plan(&published(EntryKind::Plugin), None, None, &mut gh).unwrap();
        let mut steps = vec![];
        let url = submit(&p, &mut gh, |s| steps.push(s.to_owned())).unwrap();
        assert!(url.ends_with("/pull/7"));
        let firsts: Vec<String> = gh.calls.iter().map(|c| c[..2].join(" ")).collect();
        assert_eq!(
            firsts,
            [
                "api user",
                "repo fork",
                "repo sync",
                "release create",
                "api repos/someone/ptk-marketplace/git/ref/heads/main",
                "api -X",
                "api -H",
                "api repos/someone/ptk-marketplace/contents/index.json?ref=add-moon-port-v1.2.0",
                "api -X",
                "pr create",
            ]
        );
        let pr = gh.calls.last().unwrap();
        assert!(pr.contains(&"someone:add-moon-port-v1.2.0".to_owned()));
        assert!(steps.len() >= 5);
    }

    #[test]
    fn index_entries_are_added_or_replaced() {
        let mut e = published(EntryKind::Plugin).entry;
        e.download = "https://example.org/x.zip".into();
        let one = updated_index("{\"schema_version\":1,\"entries\":[]}", &e).unwrap();
        e.version = "1.3.0".into();
        let two = updated_index(&one, &e).unwrap();
        let index: MarketIndex = serde_json::from_str(&two).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].version, "1.3.0");
        assert!(updated_index("not json", &e).is_err());
    }
}
