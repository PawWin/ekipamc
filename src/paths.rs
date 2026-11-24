//! On-disk layout. The root mirrors a `.minecraft` folder so the NeoForge
//! installer can target it directly and every pack shares Java, libraries
//! and assets. Each pack's game runs in `instances/<pack id>/` unless the
//! player picks another game directory.

use crate::settings::Profile;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    /// `~/.local/share/<id>` on Linux, `%APPDATA%\<id>` on Windows.
    pub fn new(id: &str) -> Result<Self> {
        let root = dirs::data_dir()
            .context("no data directory on this system")?
            .join(id);
        std::fs::create_dir_all(&root)
            .with_context(|| format!("cannot create {}", root.display()))?;
        Ok(Self { root })
    }

    /// v0.1 kept its only installation, always the default pack, in
    /// `instance/`. Moves it to that pack's folder so players keep their
    /// worlds and skip the modpack download. Does nothing after the move
    /// or on a fresh install.
    pub fn move_legacy_instance(&self, default_pack_id: &str) -> Result<()> {
        let legacy = self.root.join("instance");
        let target = self.default_game_dir(default_pack_id);
        if legacy.is_dir() && !target.exists() {
            ensure_parent(&target)?;
            std::fs::rename(&legacy, &target)
                .with_context(|| format!("cannot move {}", legacy.display()))?;
        }
        Ok(())
    }

    /// Where a pack's game runs unless the profile picks another directory.
    pub fn default_game_dir(&self, pack_id: &str) -> PathBuf {
        self.root.join("instances").join(pack_id)
    }

    /// The profile's game directory: mods, config, saves, options.txt.
    pub fn game_dir(&self, profile: &Profile) -> PathBuf {
        profile
            .game_dir
            .clone()
            .unwrap_or_else(|| self.default_game_dir(&profile.pack))
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_file(&self, id: &str, ext: &str) -> PathBuf {
        self.versions().join(id).join(format!("{id}.{ext}"))
    }

    pub fn java(&self, component: &str) -> PathBuf {
        self.root.join("java").join(component)
    }

    /// Native libraries LWJGL and Netty extract, per version so packs on
    /// different LWJGL releases don't overwrite each other's.
    pub fn natives(&self, version_id: &str) -> PathBuf {
        self.root.join("natives").join(version_id)
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Scratch space for installers and the modpack zip; safe to delete.
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.toml")
    }
}

/// Creates the parent directory of `path` if needed.
pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_instance_moves_to_default_pack() {
        let root = std::env::temp_dir().join(format!("ekipamc-paths-{}", std::process::id()));
        let paths = Paths { root: root.clone() };
        std::fs::create_dir_all(root.join("instance/mods")).unwrap();

        paths.move_legacy_instance("atm10").unwrap();
        let moved = paths.default_game_dir("atm10");
        assert!(moved.join("mods").is_dir());
        assert!(!root.join("instance").exists());

        // A later start finds nothing to move.
        paths.move_legacy_instance("atm10").unwrap();
        assert!(moved.join("mods").is_dir());
        std::fs::remove_dir_all(&root).ok();
    }
}
