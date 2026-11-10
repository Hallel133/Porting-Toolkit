//! Importing a game build the user owns: a folder, a macOS `.app`, a single
//! executable (`.exe`, Linux binary, `.pck`), or an archive or installer
//! image (`.zip`, `.deb`, `.dmg`).
//!
//! No source code is needed: archives are unpacked into the user's data
//! folder (`<data>/imports/`) so the engine can scan them and `ptk` can hash
//! them. Unpacked files stay on the user's machine and are never packed into
//! a `.ptk` (see `docs/legal.md`).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use zip::ZipArchive;

use crate::error::{Error, Result, io_ctx};
use crate::hash::{is_junk_file, sha256_bytes};
use crate::paths;

/// What kind of build was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildKind {
    Folder,
    App,
    Executable,
    Zip,
    Deb,
    Dmg,
}

impl BuildKind {
    pub fn describe(self) -> &'static str {
        match self {
            BuildKind::Folder => "game folder",
            BuildKind::App => "macOS app",
            BuildKind::Executable => "single executable",
            BuildKind::Zip => "zip archive",
            BuildKind::Deb => "Debian package",
            BuildKind::Dmg => "disk image",
        }
    }
}

/// A build ready to scan and hash.
#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub source: PathBuf,
    pub kind: BuildKind,
    /// The folder or file to scan.
    pub game: PathBuf,
    /// Where an archive was unpacked (inside `<data>/imports`).
    pub extracted: Option<PathBuf>,
    /// A title for the game, from the file name.
    pub title: String,
}

/// Classifies a path by its extension (and whether it is a folder).
pub fn classify(path: &Path) -> Result<BuildKind> {
    let meta = fs::metadata(path).map_err(io_ctx(path.display().to_string()))?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if meta.is_dir() {
        return Ok(if ext == "app" {
            BuildKind::App
        } else {
            BuildKind::Folder
        });
    }
    Ok(match ext.as_str() {
        "zip" => BuildKind::Zip,
        "deb" => BuildKind::Deb,
        "dmg" => BuildKind::Dmg,
        "ptk" => {
            return Err(Error::Invalid(
                "this is a Porting Toolkit project, not a game: unpack it instead".into(),
            ));
        }
        _ => BuildKind::Executable,
    })
}

/// A readable title from a build's file name: `MyGame.exe` → `MyGame`,
/// `game_1.2_amd64.deb` → `game`.
pub fn title_of(path: &Path) -> String {
    let stem = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Game".into());
    let is_deb = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("deb"));
    let base = if is_deb {
        stem.split('_').next().unwrap_or(&stem).to_owned()
    } else {
        stem
    };
    strip_platform_suffix(&base)
}

/// `MyGame-windows`, `MyGame_Win64`, `MyGame (Mac)` → `MyGame`.
fn strip_platform_suffix(name: &str) -> String {
    const SUFFIXES: &[&str] = &[
        "windows",
        "win64",
        "win32",
        "win",
        "x86_64",
        "x64",
        "amd64",
        "macos",
        "mac",
        "osx",
        "linux",
        "universal",
        "web",
        "html5",
        "pc",
        "desktop",
    ];
    let mut s = name.trim().to_owned();
    loop {
        let lower = s.to_ascii_lowercase();
        let cut = SUFFIXES.iter().find_map(|suf| {
            let paren = format!("({suf})");
            if lower.ends_with(&paren) {
                return Some(s.len() - paren.len());
            }
            let n = s.len().checked_sub(suf.len() + 1)?;
            (lower.ends_with(suf) && matches!(s.as_bytes()[n], b'-' | b'_' | b' ' | b'.'))
                .then_some(n)
        });
        match cut {
            Some(n) if n > 0 => s = s[..n].trim_end_matches([' ', '-', '_', '.']).to_owned(),
            _ => return s,
        }
    }
}

/// Where imported archives are unpacked.
pub fn imports_dir() -> PathBuf {
    paths::data_dir().join("imports")
}

/// A stable folder name per archive (same file → same folder).
fn extraction_dir(path: &Path) -> Result<PathBuf> {
    let meta = fs::metadata(path).map_err(io_ctx(path.display().to_string()))?;
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = format!("{}|{}|{modified}", canonical.display(), meta.len());
    let name = crate::manifest::slugify(&title_of(path));
    Ok(imports_dir().join(format!("{name}-{}", &sha256_bytes(key.as_bytes())[..10])))
}

/// Makes a build ready to scan: folders, apps and executables are used in
/// place; archives are unpacked once into `<data>/imports/`.
pub fn prepare(path: &Path) -> Result<Prepared> {
    let kind = classify(path)?;
    let title = title_of(path);
    let source = path.to_owned();
    let (game, extracted) = match kind {
        BuildKind::Folder | BuildKind::App | BuildKind::Executable => (path.to_owned(), None),
        BuildKind::Zip | BuildKind::Deb | BuildKind::Dmg => {
            let dest = extraction_dir(path)?;
            let ready = dest.is_dir()
                && fs::read_dir(&dest)
                    .map(|mut d| d.next().is_some())
                    .unwrap_or(false);
            if !ready {
                unpack_build(path, kind, &dest)?;
            }
            (dest.clone(), Some(dest))
        }
    };
    Ok(Prepared {
        source,
        kind,
        game,
        extracted,
        title,
    })
}

/// Unpacks into a staging folder, then moves it into place.
fn unpack_build(path: &Path, kind: BuildKind, dest: &Path) -> Result<()> {
    let parent = dest.parent().expect("imports dir has a parent");
    fs::create_dir_all(parent).map_err(io_ctx(parent.display().to_string()))?;
    let staging = parent.join(format!(
        ".{}.partial-{}",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(io_ctx(staging.display().to_string()))?;
    let result = match kind {
        BuildKind::Zip => extract_zip(path, &staging).map(|_| ()),
        BuildKind::Deb => extract_deb(path, &staging),
        BuildKind::Dmg => extract_dmg(path, &staging),
        _ => Ok(()),
    };
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let _ = fs::remove_dir_all(dest);
    fs::rename(&staging, dest).map_err(io_ctx(dest.display().to_string()))?;
    Ok(())
}

/// Extracts a zip safely: paths must stay inside `dest`, links are skipped.
/// Returns the number of files written.
pub fn extract_zip(zip: &Path, dest: &Path) -> Result<usize> {
    let file = File::open(zip).map_err(io_ctx(zip.display().to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| Error::Invalid(format!("{}: {e}", zip.display())))?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Error::Invalid(format!("{}: {e}", zip.display())))?;
        if entry.is_symlink() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            return Err(Error::Invalid(format!(
                "{}: unsafe path {:?}",
                zip.display(),
                entry.name().unwrap_or_default()
            )));
        };
        let out = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(io_ctx(out.display().to_string()))?;
            continue;
        }
        if rel
            .file_name()
            .is_some_and(|n| is_junk_file(&n.to_string_lossy()))
            || rel.components().any(|c| c.as_os_str() == "__MACOSX")
        {
            continue;
        }
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir).map_err(io_ctx(dir.display().to_string()))?;
        }
        let mut w = File::create(&out).map_err(io_ctx(out.display().to_string()))?;
        io::copy(&mut entry, &mut w).map_err(io_ctx(out.display().to_string()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o755));
        }
        count += 1;
    }
    Ok(count)
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let out = cmd
        .output()
        .map_err(io_ctx(format!("could not run {what}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Reads the members of an `ar` archive (the container of a `.deb`):
/// `(name, offset, size)`. Handles the GNU (`name/`) and BSD (`#1/len`)
/// name styles.
pub fn ar_members(bytes: &[u8]) -> Result<Vec<(String, usize, usize)>> {
    let bad = |m: &str| Error::Invalid(format!("not a valid .deb: {m}"));
    if !bytes.starts_with(b"!<arch>\n") {
        return Err(bad("missing ar signature"));
    }
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 60 <= bytes.len() {
        let header = &bytes[pos..pos + 60];
        if &header[58..60] != b"`\n" {
            return Err(bad("broken member header"));
        }
        let field = |a: usize, b: usize| String::from_utf8_lossy(&header[a..b]).trim().to_owned();
        let size: usize = field(48, 58).parse().map_err(|_| bad("bad member size"))?;
        let mut name = field(0, 16);
        let mut start = pos + 60;
        let mut len = size;
        if let Some(n) = name.strip_prefix("#1/") {
            let n: usize = n.parse().map_err(|_| bad("bad long name"))?;
            let raw = bytes
                .get(start..start + n)
                .ok_or_else(|| bad("truncated name"))?;
            name = String::from_utf8_lossy(raw)
                .trim_end_matches('\0')
                .to_owned();
            start += n;
            len = size.checked_sub(n).ok_or_else(|| bad("bad long name"))?;
        }
        let name = name.trim_end_matches('/').to_owned();
        if start + len > bytes.len() {
            return Err(bad("truncated member"));
        }
        out.push((name, start, len));
        pos += 60 + size + (size % 2);
    }
    Ok(out)
}

/// A `.deb` is an `ar` archive holding `data.tar.*`, which holds the
/// installed files. The tarball is unpacked with the system `tar` (gzip,
/// xz and zstd; it refuses absolute paths and `..` by default).
fn extract_deb(deb: &Path, dest: &Path) -> Result<()> {
    let bytes = fs::read(deb).map_err(io_ctx(deb.display().to_string()))?;
    let (name, start, len) = ar_members(&bytes)?
        .into_iter()
        .find(|(n, _, _)| n.starts_with("data.tar"))
        .ok_or_else(|| Error::Invalid("no data.tar in the .deb".into()))?;
    let work = dest.join(".deb");
    fs::create_dir_all(&work).map_err(io_ctx(work.display().to_string()))?;
    let tarball = work.join(&name);
    fs::write(&tarball, &bytes[start..start + len])
        .map_err(io_ctx(tarball.display().to_string()))?;
    run(
        Command::new("tar")
            .arg("-xf")
            .arg(&tarball)
            .arg("-C")
            .arg(dest),
        "tar",
    )?;
    fs::remove_dir_all(&work).map_err(io_ctx(work.display().to_string()))?;
    Ok(())
}

/// Mounts a disk image read-only, copies its contents, and unmounts it.
fn extract_dmg(dmg: &Path, dest: &Path) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Invalid(
            "disk images can only be imported on macOS".into(),
        ));
    }
    let mount = dest.join(".mount");
    fs::create_dir_all(&mount).map_err(io_ctx(mount.display().to_string()))?;
    run(
        Command::new("hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(dmg),
        "hdiutil attach",
    )?;
    let copied = run(
        Command::new("ditto").arg(&mount).arg(dest.join("volume")),
        "ditto",
    );
    let _ = Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount)
        .output();
    let _ = fs::remove_dir(&mount);
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn titles_and_kinds() {
        assert_eq!(title_of(Path::new("/x/MyGame.exe")), "MyGame");
        assert_eq!(title_of(Path::new("/x/Game.app")), "Game");
        assert_eq!(title_of(Path::new("/x/supertux_0.6_amd64.deb")), "supertux");
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Demo.app");
        fs::create_dir(&app).unwrap();
        assert_eq!(classify(&app).unwrap(), BuildKind::App);
        assert_eq!(classify(dir.path()).unwrap(), BuildKind::Folder);
        let exe = dir.path().join("g.exe");
        fs::write(&exe, b"MZ").unwrap();
        assert_eq!(classify(&exe).unwrap(), BuildKind::Executable);
        let ptk = dir.path().join("p.ptk");
        fs::write(&ptk, b"").unwrap();
        assert!(classify(&ptk).is_err());
    }

    #[test]
    fn zip_extraction_is_safe() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        let mut zip = zip::ZipWriter::new(File::create(&zip_path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("Game/data.pck", opts).unwrap();
        zip.write_all(b"GDPC").unwrap();
        zip.start_file("__MACOSX/._data.pck", opts).unwrap();
        zip.write_all(b"junk").unwrap();
        zip.finish().unwrap();
        let out = dir.path().join("out");
        assert_eq!(extract_zip(&zip_path, &out).unwrap(), 1);
        assert_eq!(fs::read(out.join("Game/data.pck")).unwrap(), b"GDPC");

        let bad = dir.path().join("bad.zip");
        let mut zip = zip::ZipWriter::new(File::create(&bad).unwrap());
        zip.start_file("../evil", opts).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        assert!(extract_zip(&bad, &dir.path().join("out2")).is_err());
        assert!(!dir.path().join("evil").exists());
    }
}
