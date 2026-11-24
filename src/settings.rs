//! Per-user choices remembered between runs, stored in `settings.toml`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MIN_RAM_GB: u32 = 4;
pub const MAX_RAM_GB: u32 = 16;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub nick: String,
    pub launcher: LauncherOptions,
    pub profile: Profile,
}

/// The one installation the launcher plays, editable like a profile in the
/// vanilla launcher. The pack version itself is baked in and not editable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub ram_gb: u32,
    pub fullscreen: bool,
    /// Extra JVM options, split on whitespace and added after `-Xmx`.
    pub jvm_args: String,
    /// Java to run instead of the bundled runtime.
    pub java_path: Option<PathBuf>,
    /// Initial window size; `None` lets the game pick.
    pub resolution: Option<Resolution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherOptions {
    /// When false, the launcher hides while the game runs and comes back
    /// when it exits.
    pub keep_open: bool,
    /// Open `game-output.log` in the system viewer once the game starts.
    pub open_log: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "EkipaCraft".into(),
            // ATM10 recommends 8-10 GB.
            ram_gb: 8,
            fullscreen: false,
            jvm_args: String::new(),
            java_path: None,
            resolution: None,
        }
    }
}

impl Default for LauncherOptions {
    fn default() -> Self {
        Self {
            keep_open: true,
            open_log: false,
        }
    }
}

impl Settings {
    /// Missing or unreadable settings fall back to defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, toml::to_string(self)?)?;
        Ok(())
    }
}

/// Minecraft usernames: 3-16 characters of letters, digits and underscores.
pub fn is_valid_nick(nick: &str) -> bool {
    (3..=16).contains(&nick.len()) && nick.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nick_rules() {
        assert!(is_valid_nick("Steve_123"));
        assert!(!is_valid_nick("ab"));
        assert!(!is_valid_nick("seventeen_chars_x"));
        assert!(!is_valid_nick("bad nick"));
        assert!(!is_valid_nick("żółw"));
    }

    #[test]
    fn settings_round_trip() {
        let path =
            std::env::temp_dir().join(format!("ekipamc-settings-{}.toml", std::process::id()));
        let settings = Settings {
            nick: "Steve".into(),
            launcher: LauncherOptions {
                keep_open: false,
                open_log: true,
            },
            profile: Profile {
                java_path: Some("/opt/java/bin/java".into()),
                resolution: Some(Resolution {
                    width: 1920,
                    height: 1080,
                }),
                ..Profile::default()
            },
        };
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded, settings);
    }
}
