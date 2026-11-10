# Porting Toolkit

```
██████      ████    ██████    ██████████  ██████  ██      ██    ██████
██░░░░██  ██ ░░░██  ██░░░░██   ░░░██░░░░░  ░██░░░ ████    ██░ ██ ░░░░░░
██████ ░░ ██░   ██░ ██████ ░░     ██░       ██░   ██░░██  ██░ ██░ ████
██░░░░░   ██░   ██░ ██░░██░       ██░       ██░   ██░  ░████░ ██░  ░██░
██░        ░████ ░░ ██░  ░██      ██░     ██████  ██░    ░██░  ░██████░
 ░░          ░░░░    ░░    ░░      ░░      ░░░░░░  ░░      ░░    ░░░░░░
██████████    ████      ████    ██        ██    ██  ██████  ██████████
 ░░░██░░░░░ ██ ░░░██  ██ ░░░██  ██░       ██░ ██ ░░  ░██░░░  ░░░██░░░░░
    ██░     ██░   ██░ ██░   ██░ ██░       ████ ░░     ██░       ██░
    ██░     ██░   ██░ ██░   ██░ ██░       ██░░██      ██░       ██░
    ██░      ░████ ░░  ░████ ░░ ████████  ██░  ░██  ██████      ██░
     ░░        ░░░░      ░░░░    ░░░░░░░░  ░░    ░░  ░░░░░░      ░░
```

A community tool to manage, convert, export and import game projects between
platforms. macOS first; Windows and Linux later.

**Bring your own game.** Porting Toolkit works on game files you own. Community
project files (`.ptk`) share recipes, settings, community-written patches and
SHA-256 hashes, never original game files or decompiled game code. See
[docs/legal.md](docs/legal.md).

![ptk scan on a fake Unity game](docs/images/scan.png)

> Status: **phase 2** (CLI + desktop GUI). Detection, project files, advanced
> options, packing, verifying and plugin validation work. Converting and
> exporting arrive with plugins in phase 3.

## Install from source (macOS)

You need the Xcode Command Line Tools, [rustup](https://rustup.rs) and
[uv](https://docs.astral.sh/uv/).

```bash
git clone https://github.com/Hallel133/Porting-Toolkit.git
cd Porting-Toolkit
cargo build --release
./target/release/ptk doctor     # checks tools, sets up the Python engine
```

`ptk doctor` creates a managed Python environment for the engine in
`~/Library/Application Support/PortingToolkit/`.

## Quick start

```bash
ptk                                   # banner + help
ptk init my-port --title "My Game"    # new project folder
ptk scan ~/Games/MyGame --record my-port
ptk options set textures --action recompress --quality 80 --max-resolution 2048 -p my-port
ptk info my-port                      # game, targets, advanced options
ptk pack my-port                      # -> my-port-0.1.0.ptk (no game files inside)
```

Someone else continues the port with their own copy of the game:

```bash
ptk unpack my-port-0.1.0.ptk
ptk verify ~/Games/MyGame -p my-port  # is this the same game version?
```

## Desktop app

`ptk-gui` is a project manager in the spirit of game-engine launchers: a
**Projects** list (favorites, filter, sort, an action column with Open,
Export, Scan, Verify, Show in Finder, Remove), a **Marketplace** (Browse,
Installed, Publish) and **Settings** (environment check, targets, about).
Opening a project adds its tabs: Overview, Scan, Options and Export. On macOS
it has a real menu bar with the usual shortcuts (⌘N, ⌘I import, ⌘O, ⌘S,
⌘1–⌘6, ⌘, settings). Drop a game, a project or a `.ptk` on the window.

```bash
cargo run --release -p ptk-gui
```

![Porting Toolkit: projects](docs/images/gui-projects.png)

| | |
|---|---|
| ![Overview](docs/images/gui-overview.png) | ![Export](docs/images/gui-export.png) |

## Import a game (no source code)

```bash
ptk import "Space Hopper.exe"        # or a folder, .app, .zip, .deb, .dmg
```

Porting Toolkit reads names and headers (it even finds a Godot `.pck` inside
an `.exe`), records SHA-256 hashes and creates a project. Archives are
unpacked into your data folder; game files never go into the project.

**Can it be ported without source code?** It depends on where the game's
logic lives:

| Engine | Without source code |
|---|---|
| Godot, Ren'Py, RPG Maker | Yes: the game data is portable; pair it with the engine runtime for the target |
| Unity, GameMaker | Depends: Unity Mono yes, IL2CPP no; GameMaker needs a runner for the target |
| Unreal, RAGE, Source | No: native code. Needs the source, or a compatibility layer (Wine-style) |

Exporting runs **export plugins**, which arrive in phase 3; today you can
import, scan, choose the advanced options, install exporters and share.

## Marketplace and SDKs

Plugins, SDKs (new platforms + their exporters) and projects are listed in a
JSON index on GitHub. `ptk market list|install`, `ptk sdk new|validate|install`
and `ptk publish <folder>` (zip + SHA-256 + index entry for a pull request).
See [docs/marketplace.md](docs/marketplace.md).

## Commands

| Command | What it does |
|---|---|
| `ptk`, `ptk about` | Banner, help, version, roadmap |
| `ptk init <dir>` | New project: `ptk.json`, `recipes/`, `patches/`, `plugins.lock` |
| `ptk info [project]` | Project, `ptk.json` or `.ptk`: game, targets, advanced options |
| `ptk options [set …]` | Keep / drop / recompress, quality, max resolution per asset category |
| `ptk targets` | Platforms (web, macos-arm64, macos-x64, windows-x64, linux-x64, + SDK targets) |
| `ptk doctor` | Checks Xcode CLT, clang, rustup, uv; sets up the engine |
| `ptk scan <game-dir>` | Engine and contents from file names only (`--json`, `--record`) |
| `ptk pack` / `ptk unpack` | Create / check-and-extract a `.ptk` community project file |
| `ptk verify <game-dir>` | Compare your game files with a project's recorded hashes |
| `ptk plugin list\|new\|validate\|install` | Import/export plugins (format only in phase 1) |
| `ptk import <build>` | Project from a folder, .app, .exe, .zip, .deb or .dmg |
| `ptk sdk new\|validate\|install` | SDKs: new platforms and their exporters |
| `ptk market list\|install` | Community marketplace |
| `ptk publish <path>` | Package a plugin, SDK or .ptk for the marketplace |

`ptk scan` recognises Unity, Unreal, Godot, RAGE, Source, GameMaker, Ren'Py,
RPG Maker (MV/MZ/VX/VX Ace/XP) and macOS `.app` / Mach-O, from names and
executable headers only. It never opens or decodes game archives.

Colors are off when output is not a terminal, with `NO_COLOR=1` or `--no-color`.

## Roadmap

1. macOS + CLI infrastructure + banner + file structure (done)
2. **Desktop GUI (egui)** (now)
3. Import/export plugins and smart, AI-assisted advanced options
4. Windows/Linux and the full community marketplace

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): Rust app + Python engine, JSON-RPC protocol
- [docs/project-format.md](docs/project-format.md): `ptk.json` and `.ptk` files
- [docs/plugins.md](docs/plugins.md): plugin format and security model
- [docs/marketplace.md](docs/marketplace.md): marketplace, SDKs and publishing
- [docs/legal.md](docs/legal.md): why the toolkit never distributes games

## Development

```bash
cargo test --workspace                # Rust (also starts the Python engine)
(cd engine && uv run pytest)          # Python engine
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT. See [LICENSE](LICENSE).

---

<div dir="rtl">

## עברית

**Porting Toolkit** הוא כלי קהילתי לניהול, המרה, ייצוא וייבוא של פרויקטי משחקים בין פלטפורמות. קודם macOS, ואחר כך Windows ו-Linux.

**מביאים את המשחק שלכם.** הכלי עובד על קבצי משחק שבבעלותכם. קובצי פרויקט קהילתיים (`.ptk`) משתפים מתכונים, הגדרות, תיקונים שהקהילה כתבה וטביעות SHA-256 בלבד, **אף פעם** לא קבצי משחק מקוריים או קוד משחק מפורק. הפקודה `ptk pack` מסרבת לארוז קבצים של צד שלישי, ו-`ptk verify` בודקת שהעותק שלכם תואם לגרסה שהפרויקט נבנה עבורה.

### התקנה (macOS)

צריך Xcode Command Line Tools,‏ rustup ו-uv. אחרי `cargo build --release` מריצים `ptk doctor`, שבודק את הכלים ומקים סביבת Python מנוהלת למנוע בתיקייה `~/Library/Application Support/PortingToolkit/`.

### שימוש בסיסי

- `ptk init my-port` יוצר פרויקט חדש.
- `ptk scan <תיקיית משחק> --record my-port` מזהה את המנוע (Unity,‏ Unreal,‏ Godot,‏ RAGE,‏ Source,‏ GameMaker,‏ Ren'Py,‏ RPG Maker,‏ macOS) לפי שמות קבצים בלבד, ושומר טביעות.
- `ptk options set textures --action recompress --quality 80` קובע מה נשמר, נמחק או נדחס מחדש לפני ההמרה.
- `ptk pack` / `ptk unpack` / `ptk verify` לשיתוף פרויקטים.
- `ptk plugin new|validate|install|list` לתוספי ייבוא/ייצוא (ב-Python, עם הצהרת הרשאות).

### אפליקציה גרפית

`ptk-gui` היא מנהל פרויקטים בהשראת מנהלי הפרויקטים של מנועי משחק: רשימת **Projects** (מועדפים, סינון, מיון ועמודת פעולות), **Marketplace** (Browse, Installed, Publish) ו-**Settings**. פרויקט פתוח מוסיף לשוניות Overview, Scan, Options ו-Export. ב-macOS יש menu bar אמיתי עם קיצורי מקלדת. הפעלה: `cargo run --release -p ptk-gui`.

### ייבוא משחק בלי קוד מקור

`ptk import` (או Import Game באפליקציה) מקבל תיקייה, ‎.app, ‏‎.exe, ‏‎.zip, ‏‎.deb או ‎.dmg, מזהה את המנוע (כולל ‎.pck של Godot שארוז בתוך ‎.exe), שומר hashes ויוצר פרויקט.

- **Godot,‏ Ren'Py,‏ RPG Maker:** אפשר לעשות port בלי קוד מקור, כי הלוגיקה נמצאת במידע שלא תלוי בפלטפורמה.
- **Unity ו-GameMaker:** תלוי ב-build.
- **Unreal,‏ RAGE,‏ Source:** צריך קוד מקור, או שכבת תאימות בסגנון Wine.

Export מתבצע דרך export plugins שמגיעים בשלב 3.

### Marketplace ו-SDK

תוספים, SDK (פלטפורמות חדשות עם ה-exporter שלהן) ופרויקטים נמצאים באינדקס JSON ב-GitHub. כדי לפרסם: `ptk sdk new`, ‏`ptk publish`, ואז pull request. הפרטים ב-[docs/marketplace.md](docs/marketplace.md).

### מפת דרכים

1. ‏macOS + תשתית CLI + באנר + מבנה קבצים (הושלם)
2. ממשק גרפי (עכשיו)
3. תוספי ייבוא/ייצוא ואפשרויות מתקדמות חכמות (בעזרת AI)
4. ‏Windows/Linux וקהילה מלאה

רישיון: MIT.

</div>
