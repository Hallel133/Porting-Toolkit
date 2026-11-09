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
