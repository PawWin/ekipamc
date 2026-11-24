//! The modpack definition baked into the binary from `pack.toml`.

use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct Pack {
    /// Data directory name, so different packs never share an install.
    pub id: String,
    pub name: String,
    pub subtitle: String,
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

impl Pack {
    /// Version id the NeoForge installer creates under `versions/`.
    pub fn version_id(&self) -> String {
        format!("neoforge-{}", self.neoforge)
    }
}

pub static PACK: LazyLock<Pack> =
    LazyLock::new(|| toml::from_str(include_str!("../pack.toml")).expect("pack.toml is invalid"));

#[cfg(test)]
mod tests {
    #[test]
    fn baked_pack_parses() {
        let pack = &*super::PACK;
        assert!(!pack.id.is_empty());
        if let Some(modpack) = &pack.modpack {
            assert_eq!(modpack.sha256.len(), 64, "sha256 must be a hex digest");
        }
    }
}
