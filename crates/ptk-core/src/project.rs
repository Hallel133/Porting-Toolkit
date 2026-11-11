//! Creating and inspecting project folders.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result, io_ctx};
use crate::hash::is_junk_file;
use crate::manifest::{MANIFEST_FILE, Manifest, slugify, valid_project_name};
use crate::pack::PluginsLock;
use crate::targets::{TargetRegistry, TargetStatus};

const PROJECT_README: &str = "# {title}\n\nA [Porting Toolkit](https://github.com/Hallel133/Porting-Toolkit) project.\n\n- `ptk.json`: the manifest (game, targets, advanced options, source hashes)\n- `recipes/`: conversion recipes\n- `patches/`: community-written patches and code\n- `plugins.lock`: exact plugin versions\n\nBring your own copy of the game, then:\n\n```bash\nptk verify /path/to/your/game -p .\n```\n\nThis project never contains original game files.\n";
const RECIPES_README: &str = "# Recipes\n\nWhat import plugins learned about the game (for example `godot.json`). Plain JSON, no game files.\n";
const PATCHES_README: &str = "# Patches\n\nCommunity-written patches and code. Never put original or decompiled game files here.\n";
const GITIGNORE: &str = "# Never commit game files or builds.\n/game/\n/build/\n*.ptk\n.ptk-local.json\n.DS_Store\n._*\n";

/// Paths `init` creates, for display.
pub const CREATED: &[&str] = &[
    MANIFEST_FILE,
    "README.md",
    "plugins.lock",
    "recipes/",
    "patches/",
    ".gitignore",
];

/// Options for [`init`].
#[derive(Clone, Debug, Default)]
pub struct InitOptions {
    /// Project name (default: from the folder name).
    pub name: Option<String>,
    pub title: Option<String>,
    /// Target ids (default: this machine, if it is an active target).
    pub targets: Vec<String>,
    /// Advanced options to start with (usually the app's defaults).
    pub options: std::collections::BTreeMap<String, crate::manifest::CategoryOptions>,
}

impl InitOptions {
    /// Options for a new project with the app's saved defaults
    /// (`settings.json`): default targets and advanced options.
    pub fn with_defaults(title: Option<String>) -> Self {
        let s = crate::settings::AppSettings::load();
        // A default may name a target from an SDK that was removed since.
        let registry = TargetRegistry::load_default().0;
        InitOptions {
            title,
            targets: s
                .default_targets
                .iter()
                .filter(|t| registry.get(t).is_some())
                .cloned()
                .collect(),
            options: s.new_project_options(),
            ..Default::default()
        }
    }
}

/// Creates a project in `dir` (created if needed). Refuses a folder that
/// already has a `ptk.json`; existing README/.gitignore files are kept.
pub fn init(dir: &Path, opts: InitOptions, registry: &TargetRegistry) -> Result<Manifest> {
    if dir.join(MANIFEST_FILE).exists() {
        return Err(Error::Invalid(format!(
            "{} already has a {MANIFEST_FILE}",
            dir.display()
        )));
    }
    let folder_name = dir
        .canonicalize()
        .ok()
        .or_else(|| std::env::current_dir().ok().map(|c| c.join(dir)))
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "project".into());
    let name = opts.name.unwrap_or_else(|| slugify(&folder_name));
    if !valid_project_name(&name) {
        return Err(Error::Invalid(format!(
            "{name:?} is not a valid project name (letters, digits, '-', '_', '.')"
        )));
    }
    for t in &opts.targets {
        if registry.get(t).is_none() {
            return Err(Error::Invalid(format!(
                "unknown target {t:?}; see `ptk targets`"
            )));
        }
    }
    let targets = if opts.targets.is_empty() {
        registry
            .host()
            .filter(|t| t.status == TargetStatus::Active)
            .map(|t| vec![t.id.clone()])
            .unwrap_or_default()
    } else {
        opts.targets
    };

    for sub in ["recipes", "patches"] {
        fs::create_dir_all(dir.join(sub)).map_err(io_ctx(dir.join(sub).display().to_string()))?;
    }
    let mut m = Manifest::new(&name);
    m.game.title = opts.title.clone();
    m.targets = targets;
    m.options.extend(opts.options);
    m.save(dir)?;
    let title = opts.title.unwrap_or_else(|| name.clone());
    let files = [
        ("README.md", PROJECT_README.replace("{title}", &title)),
        ("plugins.lock", PluginsLock::TEMPLATE.to_owned()),
        ("recipes/README.md", RECIPES_README.to_owned()),
        ("patches/README.md", PATCHES_README.to_owned()),
        (".gitignore", GITIGNORE.to_owned()),
    ];
    for (rel, text) in &files {
        let path = dir.join(rel);
        if !path.exists() {
            fs::write(&path, text).map_err(io_ctx(path.display().to_string()))?;
        }
    }
    Ok(m)
}

/// Number of files under a folder, ignoring OS clutter and README.md.
pub fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if is_junk_file(&name) || name == "README.md" {
                0
            } else if p.is_dir() {
                count_files(&p)
            } else {
                1
            }
        })
        .sum()
}

/// What a project folder holds besides the manifest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    pub recipes: usize,
    pub patches: usize,
    pub locked_plugins: usize,
}

pub fn contents(project: &Path) -> Result<Contents> {
    Ok(Contents {
        recipes: count_files(&project.join("recipes")),
        patches: count_files(&project.join("patches")),
        locked_plugins: PluginsLock::load(project)?
            .map(|l| l.plugin.len())
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_the_layout_once() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("My Port");
        let reg = TargetRegistry::builtin();
        let m = init(
            &project,
            InitOptions {
                title: Some("My Game".into()),
                targets: vec!["web".into()],
                ..Default::default()
            },
            &reg,
        )
        .unwrap();
        assert_eq!(m.name, "my-port");
        assert_eq!(m.targets, ["web"]);
        for rel in CREATED {
            assert!(project.join(rel.trim_end_matches('/')).exists(), "{rel}");
        }
        assert_eq!(contents(&project).unwrap(), Contents::default());
        assert!(init(&project, InitOptions::default(), &reg).is_err());
        let bad = InitOptions {
            targets: vec!["nope".into()],
            ..Default::default()
        };
        assert!(init(&dir.path().join("x"), bad, &reg).is_err());
    }
}
