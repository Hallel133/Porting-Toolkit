//! Asset categories.
//!
//! The same ids are used by the Python engine (`ptk_engine/categories.py`)
//! when it classifies files, and by the manifest's advanced `options`.
//! An integration test asserts both lists match.

/// One kind of game asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetCategory {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Whether `quality` / `max_resolution` make sense for this category.
    pub lossy: bool,
}

/// Every category, in display order.
pub const CATEGORIES: &[AssetCategory] = &[
    AssetCategory {
        id: "textures",
        name: "Textures",
        description: "Images, texture dictionaries, sprites",
        lossy: true,
    },
    AssetCategory {
        id: "models",
        name: "Models",
        description: "Meshes, skeletons, animations",
        lossy: true,
    },
    AssetCategory {
        id: "audio",
        name: "Audio",
        description: "Music, sound effects, voice, sound banks",
        lossy: true,
    },
    AssetCategory {
        id: "video",
        name: "Video",
        description: "Cutscenes and movies",
        lossy: true,
    },
    AssetCategory {
        id: "shaders",
        name: "Shaders",
        description: "Shader sources and compiled shaders",
        lossy: false,
    },
    AssetCategory {
        id: "scripts",
        name: "Scripts",
        description: "Game scripts and bytecode",
        lossy: false,
    },
    AssetCategory {
        id: "data",
        name: "Data",
        description: "Levels, tables, configuration",
        lossy: false,
    },
    AssetCategory {
        id: "fonts",
        name: "Fonts",
        description: "Font files",
        lossy: false,
    },
    AssetCategory {
        id: "localization",
        name: "Localization",
        description: "Text and translations",
        lossy: false,
    },
    AssetCategory {
        id: "archives",
        name: "Archives",
        description: "Engine packages that contain other assets",
        lossy: false,
    },
    AssetCategory {
        id: "executables",
        name: "Executables",
        description: "Programs and native libraries",
        lossy: false,
    },
    AssetCategory {
        id: "other",
        name: "Other",
        description: "Everything not recognised",
        lossy: false,
    },
];

/// Looks up a category by id.
pub fn category(id: &str) -> Option<&'static AssetCategory> {
    CATEGORIES.iter().find(|c| c.id == id)
}

/// All category ids, in display order.
pub fn category_ids() -> Vec<&'static str> {
    CATEGORIES.iter().map(|c| c.id).collect()
}

/// Output formats ("algorithms") offered for recompressing a category,
/// first = the usual choice. Empty: the category has no format choice.
/// The quality used when an option leaves it unset (1-100). Matches
/// `DEFAULT_QUALITY` in `engine/ptk_engine/media/encoders.py`; PNG stays
/// lossless unless a quality is set.
pub fn default_quality(category: &str) -> u8 {
    match category {
        "textures" => 80,
        _ => 60,
    }
}

/// Why the advanced options do nothing for games of `engine`, when its
/// export plugin never drops or re-encodes assets. The Unity exporter says
/// the same in its check (`applies_options = False` in the engine).
pub fn options_ignored(engine: &str) -> Option<&'static str> {
    match engine {
        "unity" => Some(
            "Unity games keep their images and sound inside Unity's own data files (.assets, .resS, bundles), \
             which Porting Toolkit cannot re-encode yet. These options are ignored: the export is the same as with Keep.",
        ),
        _ => None,
    }
}

/// True when `format` is one of [`formats`] for the category.
pub fn is_format(category: &str, format: &str) -> bool {
    formats(category).iter().any(|(id, _)| *id == format)
}

/// The display name of a format id, or the id itself.
pub fn format_name(category: &str, format: &str) -> String {
    formats(category)
        .iter()
        .find(|(id, _)| *id == format)
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| format.to_string())
}

pub fn formats(id: &str) -> &'static [(&'static str, &'static str)] {
    match id {
        "textures" => &[
            ("astc", "ASTC (Apple GPUs, phones)"),
            ("bc7", "BC7 (desktop GPUs)"),
            ("etc2", "ETC2 (WebGL, Android)"),
            ("webp", "WebP (web, smallest download)"),
            ("png", "PNG (lossless)"),
        ],
        "audio" => &[
            ("opus", "Opus"),
            ("vorbis", "Ogg Vorbis"),
            ("aac", "AAC"),
            ("mp3", "MP3"),
        ],
        "video" => &[
            ("h264", "H.264"),
            ("hevc", "HEVC (H.265)"),
            ("av1", "AV1"),
            ("vp9", "VP9 (WebM)"),
        ],
        "models" => &[("meshopt", "meshoptimizer"), ("draco", "Draco")],
        "fonts" => &[("woff2", "WOFF2")],
        _ => &[],
    }
}

#[cfg(test)]
mod options_tests {
    #[test]
    fn unity_ignores_options() {
        assert!(super::options_ignored("unity").is_some());
        assert!(super::options_ignored("renpy").is_none());
    }
}
