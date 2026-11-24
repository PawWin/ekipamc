//! Builds the Java command line for an offline-mode session and starts it.

use super::version::VersionJson;
use crate::install::java;
use crate::pack::Pack;
use crate::paths::Paths;
use crate::settings::Profile;
use anyhow::{Context, Result};
use md5::{Digest, Md5};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Child;
use uuid::Uuid;

/// Starts `profile` (an installation of `pack`) as `nick`; the caller waits
/// on the returned process. Game output goes to `logs/game-output.log` next
/// to the launcher data.
pub fn start(paths: &Paths, pack: &Pack, profile: &Profile, nick: &str) -> Result<Child> {
    let version = VersionJson::resolve(&paths.versions(), &pack.version_id())?;
    let component = &version
        .java_version
        .as_ref()
        .context("no Java version in version file")?
        .component;
    let args = command_line(paths, &version, profile, nick)?;

    std::fs::create_dir_all(paths.logs())?;
    let log = std::fs::File::create(paths.logs().join("game-output.log"))?;
    let java = match &profile.java_path {
        Some(custom) => custom.clone(),
        None => java::executable(paths, component),
    };
    java::command(&java)
        .args(&args)
        .current_dir(paths.game_dir(profile))
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .with_context(|| format!("cannot start Java ({})", java.display()))
}

/// JVM options, main class and game arguments, with every `${...}`
/// placeholder filled in.
fn command_line(
    paths: &Paths,
    version: &VersionJson,
    profile: &Profile,
    nick: &str,
) -> Result<Vec<String>> {
    let libraries = paths.libraries();
    let mut classpath: Vec<PathBuf> = version
        .artifacts()
        .map(|artifact| libraries.join(&artifact.path))
        .collect();
    classpath.push(paths.version_file(&version.id, "jar"));
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = std::env::join_paths(&classpath)?
        .to_string_lossy()
        .into_owned();

    let asset_index = version
        .asset_index
        .as_ref()
        .context("no asset index in version file")?;
    let uuid = offline_uuid(nick).simple().to_string();
    let path_string = |path: PathBuf| path.to_string_lossy().into_owned();
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", nick.to_owned()),
        ("auth_uuid", uuid),
        ("auth_access_token", "0".into()),
        ("auth_xuid", "0".into()),
        ("clientid", "0".into()),
        ("user_type", "legacy".into()),
        ("version_name", version.id.clone()),
        ("version_type", "release".into()),
        ("game_directory", path_string(paths.game_dir(profile))),
        ("assets_root", path_string(paths.assets())),
        ("assets_index_name", asset_index.id.clone()),
        ("natives_directory", path_string(paths.natives(&version.id))),
        ("library_directory", path_string(libraries)),
        ("classpath", classpath),
        ("classpath_separator", separator.into()),
        ("launcher_name", env!("CARGO_PKG_NAME").into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
    ]);

    let mut args = vec![format!("-Xmx{}G", profile.ram_gb)];
    if let Some(logging) = version
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
    {
        let config = paths.assets().join("log_configs").join(&logging.file.id);
        args.push(logging.argument.replace("${path}", &path_string(config)));
    }
    args.extend(
        version
            .arguments
            .jvm
            .iter()
            .flat_map(|arg| arg.values())
            .map(|arg| substitute(arg, &vars)),
    );
    // After the version's own options so the player's can override them.
    args.extend(profile.jvm_args.split_whitespace().map(String::from));
    args.push(version.main_class.clone());
    args.extend(
        version
            .arguments
            .game
            .iter()
            .flat_map(|arg| arg.values())
            .map(|arg| substitute(arg, &vars)),
    );
    args.extend(window_args(profile));
    Ok(args)
}

/// Game arguments for the profile's window options. Vanilla gates
/// `--width`/`--height` behind the `has_custom_resolution` feature; adding
/// them directly has the same effect.
fn window_args(profile: &Profile) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(resolution) = profile.resolution {
        args.extend([
            "--width".into(),
            resolution.width.to_string(),
            "--height".into(),
            resolution.height.to_string(),
        ]);
    }
    if profile.fullscreen {
        args.push("--fullscreen".into());
    }
    args
}

/// Replaces `${name}` placeholders; unknown ones are left as-is.
fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) if vars.contains_key(&after[..end]) => {
                out.push_str(&vars[&after[..end]]);
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str("${");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Same UUID an offline-mode server assigns: Java's
/// `UUID.nameUUIDFromBytes("OfflinePlayer:" + nick)`. Keeping it identical
/// keeps inventories tied to the nick.
pub fn offline_uuid(nick: &str) -> Uuid {
    let hash = Md5::digest(format!("OfflinePlayer:{nick}"));
    uuid::Builder::from_md5_bytes(hash.into()).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java() {
        // From `UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))`.
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn window_options_become_game_args() {
        let profile = Profile {
            fullscreen: true,
            resolution: Some(crate::settings::Resolution {
                width: 1280,
                height: 720,
            }),
            ..Profile::default()
        };
        assert_eq!(
            window_args(&profile),
            ["--width", "1280", "--height", "720", "--fullscreen"]
        );
        assert!(window_args(&Profile::default()).is_empty());
    }

    #[test]
    fn substitutes_placeholders() {
        let vars = HashMap::from([("a", "1".to_string()), ("b", "2".to_string())]);
        assert_eq!(substitute("-p ${a}${b}/x", &vars), "-p 12/x");
        assert_eq!(substitute("${missing} ${a}", &vars), "${missing} 1");
    }
}
