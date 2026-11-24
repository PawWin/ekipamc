//! The modpacks baked into the binary from `pack.toml`.

use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Data directory name, so different launchers never share an install.
    pub id: String,
    pub name: String,
    /// Never empty; the first one is installed on first start.
    pub packs: Vec<Pack>,
}

#[derive(Debug, Deserialize)]
pub struct Pack {
    /// Stable key saved in settings.toml and used as the game folder name.
    pub id: String,
    pub name: String,
    pub minecraft: String,
    pub neoforge: String,
    pub modpack: Option<Modpack>,
}

/// A zip with the instance folders (mods/, config/, ...) at its root.
#[derive(Debug, Deserialize)]
pub struct Modpack {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

impl Config {
    pub fn default_pack(&self) -> &Pack {
        &self.packs[0]
    }

    pub fn pack(&self, id: &str) -> Option<&Pack> {
        self.packs.iter().find(|pack| pack.id == id)
    }
}

impl Pack {
    /// Version id the NeoForge installer creates under `versions/`.
    pub fn version_id(&self) -> String {
        format!("neoforge-{}", self.neoforge)
    }

    /// One-line summary, e.g. "All the Mods 10 8.6".
    pub fn label(&self) -> String {
        match &self.modpack {
            Some(modpack) => format!("{} {}", self.name, modpack.version),
            None => self.name.clone(),
        }
    }
}

pub static CONFIG: LazyLock<Config> =
    LazyLock::new(|| toml::from_str(include_str!("../pack.toml")).expect("pack.toml is invalid"));

#[cfg(test)]
mod tests {
    #[test]
    fn baked_config_is_valid() {
        let config = &*super::CONFIG;
        assert!(!config.id.is_empty());
        assert!(
            !config.packs.is_empty(),
            "pack.toml needs at least one pack"
        );
        for (i, pack) in config.packs.iter().enumerate() {
            assert!(
                config.packs[..i].iter().all(|other| other.id != pack.id),
                "duplicate pack id {}",
                pack.id
            );
            if let Some(modpack) = &pack.modpack {
                assert_eq!(modpack.sha256.len(), 64, "sha256 must be a hex digest");
            }
        }
    }
}
