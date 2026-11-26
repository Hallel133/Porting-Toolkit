//! `ptk`: the Porting Toolkit command line.

mod banner;
mod community;
mod convert;
mod doctor;
mod plugin;
mod project;
mod scan;
mod style;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

use ptk_core::convert::AssetsMode;
use style::Style;

const ABOUT: &str =
    "Porting Toolkit: manage, convert, export and import game projects between platforms";

#[derive(Parser)]
#[command(name = "ptk", version, about = ABOUT, disable_help_subcommand = true)]
struct Cli {
    /// Never use colors (also: NO_COLOR=1)
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show the banner, version, roadmap and principles
    About,
    /// Create a new porting project in DIR
    Init {
        dir: PathBuf,
        /// Project name (default: the folder name)
        #[arg(long)]
        name: Option<String>,
        /// Title of the game being ported
        #[arg(long)]
        title: Option<String>,
        /// Target platform id (repeatable; default: this Mac)
        #[arg(long = "target", value_name = "ID")]
        targets: Vec<String>,
    },
    /// Show a project, ptk.json or .ptk file: game, targets, advanced options
    Info {
        #[arg(default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Show or change the advanced options applied before porting
    Options {
        /// Project folder
        #[arg(short, long, default_value = ".", global = true)]
        project: PathBuf,
        #[command(subcommand)]
        action: Option<OptionsAction>,
    },
    /// List the platforms a project can target
    Targets {
        #[arg(long)]
        json: bool,
    },
    /// Check the tools ptk needs and set up the Python engine
    Doctor {
        /// Only check; do not create or update the managed environment
        #[arg(long)]
        no_install: bool,
    },
    /// Detect the engine and contents of a game folder (names only, nothing is decoded)
    Scan {
        game_dir: PathBuf,
        #[arg(long)]
        json: bool,
        /// Record the engine and SHA-256 hashes of the game files into PROJECT
        #[arg(long, value_name = "PROJECT")]
        record: Option<PathBuf>,
    },
    /// Every engine Porting Toolkit recognises, and what porting each one takes
    Engines {
        /// Only this status: supported, web, partial, runtime, closed, not_engine, unresolved
        #[arg(long)]
        status: Option<String>,
        /// Only engines whose name or id contains this text
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Pack a project into a shareable .ptk file (never game files)
    Pack {
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Output file (default: NAME-VERSION.ptk in the current folder)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Replace an existing output file
        #[arg(long)]
        force: bool,
    },
    /// Unpack and check a .ptk file
    Unpack {
        pack: PathBuf,
        /// Destination folder (default: the project name)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check your own game files against a project's recorded hashes
    Verify {
        game_dir: PathBuf,
        /// Project folder, ptk.json or .ptk file
        #[arg(short, long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Create, validate, list and install import/export plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Start a project from a game build: folder, .app, .exe, .zip, .deb or .dmg
    Import {
        /// The game build you own (no source code needed)
        build: PathBuf,
        /// Project folder (created if missing; default: NAME-port)
        #[arg(short, long)]
        project: Option<PathBuf>,
    },
    /// Build the game for other platforms with export plugins (macOS, web, ...)
    Export {
        /// Project folder
        #[arg(short, long, default_value = ".")]
        project: PathBuf,
        /// Target id (repeatable; default: the project's targets)
        #[arg(short, long = "target", value_name = "ID")]
        targets: Vec<String>,
        /// Output folder; each target gets a subfolder (default: PROJECT/build)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Game folder (default: the one the project was imported from)
        #[arg(long)]
        game: Option<PathBuf>,
        /// Use this plugin id instead of the best match
        #[arg(long)]
        plugin: Option<String>,
        /// Plugin setting, e.g. --set templates=PATH (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Open the result when done
        #[arg(long)]
        open: bool,
        /// Advanced options: apply them, only estimate the savings (plan), or ignore them
        #[arg(long, value_name = "MODE", default_value = "apply", value_parser = ["apply", "plan", "off"])]
        assets: String,
        /// Only say what would stop or limit the export (builds nothing)
        #[arg(long)]
        check: bool,
        #[arg(long)]
        json: bool,
    },
    /// Estimate what the advanced options save, per target (builds nothing)
    Assets {
        /// Project folder
        #[arg(short, long, default_value = ".")]
        project: PathBuf,
        /// Target id (repeatable; default: the project's targets)
        #[arg(short, long = "target", value_name = "ID")]
        targets: Vec<String>,
        /// Game folder (default: the one the project was imported from)
        #[arg(long)]
        game: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Tools exports use (FFmpeg, cwebp, oxipng, the Android SDK, ...): list or install them
    Tools {
        #[command(subcommand)]
        action: Option<ToolsAction>,
    },
    /// Set or remove the port's icon and poster (images you may share)
    Art {
        /// Project folder
        #[arg(short, long, default_value = ".")]
        project: PathBuf,
        /// PNG, JPEG or WebP for the app icon
        #[arg(long)]
        icon: Option<PathBuf>,
        /// PNG, JPEG or WebP poster (cover art) for the marketplace
        #[arg(long)]
        poster: Option<PathBuf>,
        #[arg(long)]
        clear_icon: bool,
        #[arg(long)]
        clear_poster: bool,
    },
    /// Serve a web build on this computer (with the headers WebAssembly games need)
    Serve {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, default_value_t = 8000)]
        port: u16,
        /// Open it in the browser
        #[arg(long)]
        open: bool,
    },
    /// Create, validate and install SDKs (new targets + their plugins)
    Sdk {
        #[command(subcommand)]
        action: SdkAction,
    },
    /// Browse and install community plugins, SDKs and projects
    Market {
        #[command(subcommand)]
        action: MarketAction,
    },
    /// Package a plugin, SDK or .ptk for the marketplace (zip + SHA-256 + entry)
    Publish {
        /// Plugin folder, SDK folder or .ptk file
        path: PathBuf,
        /// Where to write the zip (default: ./dist)
        #[arg(short, long, default_value = "dist")]
        output: PathBuf,
    },
}

#[derive(Subcommand, Clone)]
pub enum ToolsAction {
    /// Show each tool: found where, or how to get it
    List {
        #[arg(long)]
        json: bool,
    },
    /// Download the pinned build of a tool (checked against its SHA-256)
    Install {
        /// Tool id, e.g. oxipng
        id: String,
    },
}

#[derive(Subcommand)]
pub enum SdkAction {
    /// Create an SDK skeleton (a target and its export plugin)
    New {
        dir: PathBuf,
        /// SDK id (default: from the folder name)
        #[arg(long)]
        id: Option<String>,
    },
    /// Check ptk-sdk.toml, targets and plugins
    Validate {
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Install an SDK after showing its plugins' permissions
    Install {
        dir: PathBuf,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum MarketAction {
    /// List marketplace entries
    List {
        /// Filter by text
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Download (SHA-256 checked) and install an entry
    Install {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Check that your copy of a game is the one a port was made for
    Check {
        /// Port id
        id: String,
        /// Your game folder (or its executable)
        game: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Publish a plugin, SDK or .ptk port: GitHub release + pull request (uses gh)
    Submit {
        /// Plugin folder, SDK folder or .ptk file
        path: PathBuf,
        /// Game ports: a page where players get the game (https)
        #[arg(long)]
        store: Option<String>,
        /// Where the packaged file is written
        #[arg(short, long, default_value = "dist")]
        output: PathBuf,
        /// Show what would be published, change nothing
        #[arg(long)]
        dry_run: bool,
        /// Do not ask before publishing
        #[arg(long)]
        yes: bool,
    },
    /// Installed plugins, SDKs and ports that have a newer version
    Outdated {
        /// Folder with port projects (default: your projects folder)
        #[arg(long)]
        projects: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum OptionsAction {
    /// Set the options of one asset category
    Set {
        /// Category id (textures, audio, video, ...)
        category: String,
        /// keep, drop or recompress
        #[arg(long)]
        action: Option<String>,
        /// 1-100, for recompression
        #[arg(long)]
        quality: Option<u8>,
        /// Longest edge in pixels
        #[arg(long)]
        max_resolution: Option<u32>,
        /// Preferred output format (e.g. astc, opus)
        #[arg(long)]
        format: Option<String>,
        /// Back to the default (keep)
        #[arg(long, conflicts_with_all = ["action", "quality", "max_resolution", "format"])]
        reset: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PluginKind {
    Import,
    Export,
}

impl PluginKind {
    fn as_str(self) -> &'static str {
        match self {
            PluginKind::Import => "import",
            PluginKind::Export => "export",
        }
    }
}

#[derive(Subcommand)]
pub enum PluginAction {
    /// List installed plugins (and ./plugins)
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a plugin skeleton in DIR
    New {
        dir: PathBuf,
        #[arg(long, value_enum)]
        kind: PluginKind,
        /// Plugin id (default: from the folder name)
        #[arg(long)]
        id: Option<String>,
    },
    /// Check a plugin's plugin.toml and show its permissions
    Validate {
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Install a local plugin after showing its permissions
    Install {
        dir: PathBuf,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
    },
}

fn clap_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default().bold())
        .usage(AnsiColor::Yellow.on_default().bold())
        .literal(AnsiColor::Cyan.on_default().bold())
        .placeholder(AnsiColor::Cyan.on_default())
}

fn command(style: &Style) -> clap::Command {
    Cli::command().styles(clap_styles()).color(if style.color {
        ColorChoice::Always
    } else {
        ColorChoice::Never
    })
}

fn main() -> ExitCode {
    let no_color_flag = std::env::args_os().any(|a| a == "--no-color");
    let style = Style::detect(no_color_flag);
    let matches = command(&style).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match run(cli, &style) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e:#}", style.red("error:"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, style: &Style) -> anyhow::Result<ExitCode> {
    let Some(cmd) = cli.command else {
        print!("{}", banner::banner(style));
        println!();
        command(style).print_help()?;
        return Ok(ExitCode::SUCCESS);
    };
    match cmd {
        Command::About => project::about(style),
        Command::Init {
            dir,
            name,
            title,
            targets,
        } => project::init(style, &dir, name, title, targets),
        Command::Info { project, json } => project::info(style, &project, json),
        Command::Options { project, action } => project::options(style, &project, action),
        Command::Targets { json } => project::targets(style, json),
        Command::Doctor { no_install } => doctor::run(style, no_install),
        Command::Scan {
            game_dir,
            json,
            record,
        } => scan::run(style, &game_dir, json, record.as_deref()),
        Command::Engines {
            status,
            search,
            json,
        } => scan::engines(style, status.as_deref(), search.as_deref(), json),
        Command::Pack {
            project,
            output,
            force,
        } => project::pack(style, &project, output, force),
        Command::Unpack { pack, output } => project::unpack(style, &pack, output),
        Command::Verify {
            game_dir,
            project,
            json,
        } => project::verify(style, &game_dir, &project, json),
        Command::Plugin { action } => plugin::run(style, action),
        Command::Import { build, project } => scan::import(style, &build, project.as_deref()),
        Command::Export {
            project,
            targets,
            game,
            plugin,
            set,
            check: true,
            json,
            ..
        } => convert::check(style, &project, targets, game, plugin, set, json),
        Command::Export {
            project,
            targets,
            output,
            game,
            plugin,
            set,
            open,
            assets,
            check: false,
            json,
        } => convert::export(
            style,
            &project,
            targets,
            output,
            game,
            plugin,
            set,
            json,
            open,
            AssetsMode::parse(&assets).unwrap_or_default(),
        ),
        Command::Assets {
            project,
            targets,
            game,
            json,
        } => convert::export(
            style,
            &project,
            targets,
            None,
            game,
            None,
            Vec::new(),
            json,
            false,
            AssetsMode::Plan,
        ),
        Command::Tools { action } => {
            convert::tools(style, action.unwrap_or(ToolsAction::List { json: false }))
        }
        Command::Serve { dir, port, open } => convert::serve(style, &dir, port, open),
        Command::Art {
            project,
            icon,
            poster,
            clear_icon,
            clear_poster,
        } => convert::art(style, &project, icon, poster, clear_icon, clear_poster),
        Command::Sdk { action } => community::sdk(style, action),
        Command::Market { action } => community::market(style, action),
        Command::Publish { path, output } => community::publish(style, &path, &output),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        super::Cli::command().debug_assert();
    }
}
