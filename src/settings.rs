//! Per-user choices remembered between runs.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MIN_RAM_GB: u32 = 4;
pub const MAX_RAM_GB: u32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub nick: String,
    pub ram_gb: u32,
}

impl Default for Settings {
    fn default() -> Self {
        // ATM10 recommends 8-10 GB.
        Self {
            nick: String::new(),
            ram_gb: 8,
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
    use super::is_valid_nick;

    #[test]
    fn nick_rules() {
        assert!(is_valid_nick("Steve_123"));
        assert!(!is_valid_nick("ab"));
        assert!(!is_valid_nick("seventeen_chars_x"));
        assert!(!is_valid_nick("bad nick"));
        assert!(!is_valid_nick("żółw"));
    }
}
