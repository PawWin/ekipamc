//! Per-user choices remembered between runs, stored in `settings.toml`.

use crate::pack::{Config, Pack};
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
    /// Installations in creation order. Never empty after `load`.
    pub profiles: Vec<Profile>,
    /// Index into `profiles` of the installation Play starts.
    pub selected: usize,
    /// The single installation v0.1 saved; `fit` turns it into the first
    /// profile and it is never written back.
    #[serde(rename = "profile", skip_serializing)]
    legacy_profile: Option<Profile>,
}

/// An installation, editable like a profile in the vanilla launcher. Its
/// pack is chosen when it is created and can't change afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Id of a pack in pack.toml.
    pub pack: String,
    pub ram_gb: u32,
    pub fullscreen: bool,
    /// Extra JVM options, split on whitespace and added after `-Xmx`.
    pub jvm_args: String,
    /// Java to run instead of the bundled runtime.
    pub java_path: Option<PathBuf>,
    /// Game directory instead of `instances/<pack>/` in the data directory.
    /// A new directory gets its own copy of the modpack on the next install.
    pub game_dir: Option<PathBuf>,
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

impl Profile {
    /// A fresh installation of `pack` with default options.
    pub fn new(name: &str, pack: &Pack) -> Self {
        Self {
            name: name.into(),
            pack: pack.id.clone(),
            ..Self::default()
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            pack: String::new(),
            // ATM10 recommends 8-10 GB, ATM11 6.
            ram_gb: 8,
            fullscreen: false,
            jvm_args: String::new(),
            java_path: None,
            game_dir: None,
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
    /// Missing or unreadable settings fall back to defaults; see `fit`.
    pub fn load(path: &Path, config: &Config) -> Self {
        let mut settings: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        settings.fit(config);
        settings
    }

    /// Drops installations of packs no longer in pack.toml, adds the
    /// default one when none are left (carrying over a v0.1 profile, which
    /// always played the default pack) and keeps `selected` in range.
    fn fit(&mut self, config: &Config) {
        let selected = self.profiles.get(self.selected).cloned();
        self.profiles
            .retain(|profile| config.pack(&profile.pack).is_some());
        let legacy = self.legacy_profile.take();
        if self.profiles.is_empty() {
            let default = config.default_pack();
            self.profiles.push(match legacy {
                Some(legacy) => Profile {
                    pack: default.id.clone(),
                    ..legacy
                },
                None => Profile::new("EkipaCraft", default),
            });
        }
        self.selected = selected
            .and_then(|selected| self.profiles.iter().position(|p| *p == selected))
            .unwrap_or(0);
    }

    pub fn profile(&self) -> &Profile {
        &self.profiles[self.selected]
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
        let config = &*crate::pack::CONFIG;
        let path =
            std::env::temp_dir().join(format!("ekipamc-settings-{}.toml", std::process::id()));
        let settings = Settings {
            nick: "Steve".into(),
            launcher: LauncherOptions {
                keep_open: false,
                open_log: true,
            },
            profiles: vec![
                Profile::new("EkipaCraft", config.default_pack()),
                Profile {
                    java_path: Some("/opt/java/bin/java".into()),
                    game_dir: Some("/games/ekipacraft".into()),
                    resolution: Some(Resolution {
                        width: 1920,
                        height: 1080,
                    }),
                    ..Profile::new("Second", config.default_pack())
                },
            ],
            selected: 1,
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path, config);
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn fit_drops_unknown_packs_and_keeps_selection() {
        let config = &*crate::pack::CONFIG;
        let known = Profile::new("Known", config.default_pack());
        let mut settings = Settings {
            profiles: vec![
                Profile {
                    pack: "removed".into(),
                    ..known.clone()
                },
                known.clone(),
            ],
            selected: 1,
            ..Settings::default()
        };
        settings.fit(config);
        assert_eq!(settings.profiles, [known]);
        assert_eq!(settings.selected, 0);

        let mut empty = Settings::default();
        empty.fit(config);
        assert_eq!(empty.profiles.len(), 1);
        assert_eq!(empty.profile().pack, config.default_pack().id);
    }

    #[test]
    fn v0_1_profile_becomes_the_default_installation() {
        let config = &*crate::pack::CONFIG;
        let path = std::env::temp_dir().join(format!("ekipamc-legacy-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "nick = \"Steve\"\n[profile]\nname = \"Mine\"\nram_gb = 12\n",
        )
        .unwrap();
        let loaded = Settings::load(&path, config);
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profile().name, "Mine");
        assert_eq!(loaded.profile().ram_gb, 12);
        assert_eq!(loaded.profile().pack, config.default_pack().id);
        assert!(!toml::to_string(&loaded).unwrap().contains("[profile]"));
    }
}
