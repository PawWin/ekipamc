//! On-disk layout. The root mirrors a `.minecraft` folder so the NeoForge
//! installer can target it directly; the game itself runs in `instance/`
//! unless the player picks another game directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    instance: PathBuf,
}

impl Paths {
    /// `~/.local/share/<id>` on Linux, `%APPDATA%\<id>` on Windows.
    pub fn new(id: &str) -> Result<Self> {
        let root = dirs::data_dir()
            .context("no data directory on this system")?
            .join(id);
        std::fs::create_dir_all(&root)
            .with_context(|| format!("cannot create {}", root.display()))?;
        Ok(Self {
            instance: root.join("instance"),
            root,
        })
    }

    /// The same layout with `dir` as the game directory, if given.
    pub fn with_instance(&self, dir: Option<&Path>) -> Self {
        Self {
            root: self.root.clone(),
            instance: dir.map_or_else(|| self.root.join("instance"), Path::to_path_buf),
        }
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

    /// The game directory: mods, config, saves, options.txt.
    pub fn instance(&self) -> PathBuf {
        self.instance.clone()
    }

    pub fn natives(&self) -> PathBuf {
        self.root.join("natives")
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
