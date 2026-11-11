//! A port's icon and poster: images you choose, kept in the project's
//! `art/` folder and shared in `.ptk` files.
//!
//! Exports use the icon for the app (macOS) and the page (web); the
//! marketplace shows the poster. Use art you made or may share. The game's
//! own icon, which an import plugin can find, stays on this computer
//! (`.ptk-local.json`) and is never put into the project.

use std::fs;
use std::path::{Path, PathBuf};

use crate::convert::LocalState;
use crate::error::{Error, Result, io_ctx};
use crate::manifest::Manifest;

/// Largest image accepted.
pub const MAX_BYTES: u64 = 8 << 20;
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtKind {
    Icon,
    Poster,
}

impl ArtKind {
    pub fn name(self) -> &'static str {
        match self {
            ArtKind::Icon => "icon",
            ArtKind::Poster => "poster",
        }
    }

    fn slot(self, m: &mut Manifest) -> &mut Option<String> {
        match self {
            ArtKind::Icon => &mut m.game.icon,
            ArtKind::Poster => &mut m.game.poster,
        }
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| EXTENSIONS.contains(&e.as_str()))
}

/// Copies `image` into `art/<kind>.<ext>` and records it in `ptk.json`.
/// Returns the stored relative path.
pub fn set(project: &Path, kind: ArtKind, image: &Path) -> Result<String> {
    let ext = extension(image).ok_or_else(|| {
        Error::Invalid(format!(
            "{} must be a PNG, JPEG or WebP image",
            image.display()
        ))
    })?;
    let meta = fs::metadata(image).map_err(io_ctx(image.display().to_string()))?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(Error::Invalid(format!(
            "{} must be an image file of at most 8 MB",
            image.display()
        )));
    }
    let mut m = Manifest::load(project)?;
    let dir = project.join("art");
    fs::create_dir_all(&dir).map_err(io_ctx(dir.display().to_string()))?;
    let rel = format!("art/{}.{ext}", kind.name());
    let bytes = fs::read(image).map_err(io_ctx(image.display().to_string()))?;
    if let Some(old) = kind.slot(&mut m).take()
        && old != rel
    {
        let _ = fs::remove_file(project.join(&old));
    }
    let dest = project.join(&rel);
    fs::write(&dest, bytes).map_err(io_ctx(dest.display().to_string()))?;
    *kind.slot(&mut m) = Some(rel.clone());
    m.save(project)?;
    Ok(rel)
}

/// Removes the icon or poster.
pub fn clear(project: &Path, kind: ArtKind) -> Result<()> {
    let mut m = Manifest::load(project)?;
    if let Some(old) = kind.slot(&mut m).take() {
        let _ = fs::remove_file(project.join(old));
        m.save(project)?;
    }
    Ok(())
}

/// The image to show for a project: its chosen art, else (for the icon)
/// the game's own icon found on this computer.
pub fn resolve(project: &Path, m: &Manifest, kind: ArtKind) -> Option<PathBuf> {
    let chosen = match kind {
        ArtKind::Icon => m.game.icon.as_ref(),
        ArtKind::Poster => m.game.poster.as_ref(),
    };
    if let Some(p) = chosen.map(|rel| project.join(rel)).filter(|p| p.is_file()) {
        return Some(p);
    }
    match kind {
        ArtKind::Icon => LocalState::load(project).game_icon.filter(|p| p.is_file()),
        ArtKind::Poster => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::TargetRegistry;

    #[test]
    fn set_replace_and_clear() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("p");
        crate::project::init(&project, Default::default(), &TargetRegistry::builtin()).unwrap();
        let png = tmp.path().join("cover.PNG");
        fs::write(&png, b"\x89PNG fake").unwrap();
        assert_eq!(
            set(&project, ArtKind::Poster, &png).unwrap(),
            "art/poster.png"
        );
        let m = Manifest::load(&project).unwrap();
        assert_eq!(m.game.poster.as_deref(), Some("art/poster.png"));
        assert!(m.validate(&TargetRegistry::builtin()).is_empty());
        let jpg = tmp.path().join("cover.jpg");
        fs::write(&jpg, b"jpeg").unwrap();
        set(&project, ArtKind::Poster, &jpg).unwrap();
        assert!(!project.join("art/poster.png").exists());
        assert_eq!(
            resolve(
                &project,
                &Manifest::load(&project).unwrap(),
                ArtKind::Poster
            ),
            Some(project.join("art/poster.jpg"))
        );
        assert!(set(&project, ArtKind::Icon, &tmp.path().join("x.gif")).is_err());
        clear(&project, ArtKind::Poster).unwrap();
        assert!(Manifest::load(&project).unwrap().game.poster.is_none());
        assert!(!project.join("art/poster.jpg").exists());
    }
}
