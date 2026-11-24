//! Minecraft version JSON (`versions/<id>/<id>.json`) and its rule system.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: String,
    #[serde(default)]
    pub arguments: Arguments,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub asset_index: Option<AssetIndexRef>,
    pub downloads: Option<VersionDownloads>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    action: RuleAction,
    os: Option<OsRule>,
    features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
struct OsRule {
    name: Option<String>,
    arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<LibraryArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryArtifact {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(flatten)]
    pub file: FileRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: FileRef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    /// JVM argument template containing `${path}`.
    pub argument: String,
    pub file: LogConfigFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfigFile {
    pub id: String,
    #[serde(flatten)]
    pub file: FileRef,
}

impl VersionJson {
    /// Loads `id` and folds in every `inheritsFrom` parent, the way the
    /// vanilla launcher does: child libraries win, arguments are appended.
    pub fn resolve(versions_dir: &Path, id: &str) -> Result<Self> {
        let path = versions_dir.join(id).join(format!("{id}.json"));
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("missing version file {}", path.display()))?;
        let version: Self = serde_json::from_str(&text)
            .with_context(|| format!("invalid version file {}", path.display()))?;
        match version.inherits_from.clone() {
            Some(parent) => Ok(version.merge_onto(Self::resolve(versions_dir, &parent)?)),
            None => Ok(version),
        }
    }

    fn merge_onto(self, parent: Self) -> Self {
        let mut libraries = self.libraries;
        for library in parent.libraries {
            let key = library_key(&library.name);
            if !libraries.iter().any(|own| library_key(&own.name) == key) {
                libraries.push(library);
            }
        }
        let mut arguments = parent.arguments;
        arguments.game.extend(self.arguments.game);
        arguments.jvm.extend(self.arguments.jvm);
        Self {
            id: self.id,
            inherits_from: None,
            main_class: self.main_class,
            arguments,
            libraries,
            asset_index: self.asset_index.or(parent.asset_index),
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
            logging: self.logging.or(parent.logging),
        }
    }

    /// Libraries that apply to this OS and have a jar to put on the classpath.
    pub fn artifacts(&self) -> impl Iterator<Item = &LibraryArtifact> {
        self.libraries
            .iter()
            .filter(|library| rules_allow(&library.rules))
            .filter_map(|library| library.downloads.as_ref()?.artifact.as_ref())
    }
}

impl Argument {
    /// The argument's strings if its rules apply here.
    pub fn values(&self) -> Vec<&str> {
        match self {
            Self::Plain(value) => vec![value],
            Self::Conditional { rules, value } if rules_allow(rules) => match value {
                ArgumentValue::One(value) => vec![value],
                ArgumentValue::Many(values) => values.iter().map(String::as_str).collect(),
            },
            Self::Conditional { .. } => vec![],
        }
    }
}

/// `group:artifact:version[:classifier]` without the version, so two
/// versions of the same library collide when merging.
fn library_key(name: &str) -> String {
    let mut parts: Vec<&str> = name.split(':').collect();
    if parts.len() > 2 {
        parts.remove(2);
    }
    parts.join(":")
}

/// Last matching rule wins; no rules means allowed. We enable no launcher
/// features (demo mode, custom resolution, quick play).
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules
        .iter()
        .rev()
        .find(|rule| rule.matches())
        .is_some_and(|rule| rule.action == RuleAction::Allow)
}

impl Rule {
    fn matches(&self) -> bool {
        let os_matches = self.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|name| name == current_os())
                && os
                    .arch
                    .as_deref()
                    .is_none_or(|arch| arch == std::env::consts::ARCH)
        });
        let features_match = self
            .features
            .as_ref()
            .is_none_or(|features| features.values().all(|wanted| !wanted));
        os_matches && features_match
    }
}

fn current_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(json: &str) -> Rule {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn os_rules() {
        let only_here = rule(&format!(
            r#"{{"action":"allow","os":{{"name":"{}"}}}}"#,
            current_os()
        ));
        let only_mac = rule(r#"{"action":"allow","os":{"name":"osx"}}"#);
        let not_here = rule(&format!(
            r#"{{"action":"disallow","os":{{"name":"{}"}}}}"#,
            current_os()
        ));
        let allow_all = rule(r#"{"action":"allow"}"#);

        assert!(rules_allow(&[]));
        assert!(rules_allow(std::slice::from_ref(&only_here)));
        assert!(!rules_allow(&[only_mac]));
        assert!(!rules_allow(&[allow_all, not_here]));
    }

    #[test]
    fn feature_rules_stay_off() {
        let demo = rule(r#"{"action":"allow","features":{"is_demo_user":true}}"#);
        assert!(!rules_allow(&[demo]));
    }

    #[test]
    fn library_key_ignores_version() {
        assert_eq!(library_key("org.ow2.asm:asm:9.10.1"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org.lwjgl:lwjgl:natives-linux"
        );
    }
}
