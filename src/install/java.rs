//! Installs the Java runtime Mojang ships for the version (e.g. Java 25 for
//! 26.1.2), so players don't need Java installed themselves.

use crate::net::{Download, Net};
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const RUNTIMES_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const MARKER: &str = ".installed";

/// platform -> component -> builds (normally one).
type RuntimeIndex = HashMap<String, HashMap<String, Vec<RuntimeBuild>>>;

#[derive(Deserialize)]
struct RuntimeBuild {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Deserialize)]
struct ManifestRef {
    url: String,
}

#[derive(Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Deserialize)]
struct RuntimeManifest {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: RuntimeDownloads,
    },
    Directory,
    Link {
        #[cfg_attr(not(unix), allow(dead_code))]
        target: String,
    },
}

#[derive(Deserialize)]
struct RuntimeDownloads {
    raw: crate::game::version::FileRef,
}

/// The java binary inside an installed runtime.
pub fn executable(paths: &Paths, component: &str) -> PathBuf {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    paths.java(component).join("bin").join(name)
}

/// A command for the bundled Java that opens no console window on Windows.
pub fn command(java: &Path) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut command = tokio::process::Command::new(java);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Installs `component` (e.g. `java-runtime-delta`) unless it is already
/// present. Returns the java binary.
pub async fn ensure(
    net: &Net,
    paths: &Paths,
    component: &str,
    on_progress: impl Fn(usize, usize),
) -> Result<PathBuf> {
    let dir = paths.java(component);
    let marker = dir.join(MARKER);
    let java = executable(paths, component);
    if java.exists() && marker.exists() {
        return Ok(java);
    }

    let index: RuntimeIndex = net.json(RUNTIMES_URL).await?;
    let build = index
        .get(platform()?)
        .and_then(|components| components.get(component))
        .and_then(|builds| builds.first())
        .with_context(|| format!("Mojang has no {component} Java for this system"))?;
    let manifest: RuntimeManifest = net.json(&build.manifest.url).await?;
    let mut downloads = Vec::new();
    for (name, file) in &manifest.files {
        let path = dir.join(name);
        match file {
            RuntimeFile::Directory => std::fs::create_dir_all(&path)?,
            RuntimeFile::File { downloads: d, .. } => downloads.push(Download {
                url: d.raw.url.clone(),
                path,
                sha1: Some(d.raw.sha1.clone()),
                size: Some(d.raw.size),
            }),
            RuntimeFile::Link { .. } => {}
        }
    }
    net.download_all(&downloads, on_progress).await?;

    #[cfg(unix)]
    for (name, file) in &manifest.files {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        match file {
            RuntimeFile::File {
                executable: true, ..
            } => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
            RuntimeFile::Link { target } if std::fs::symlink_metadata(&path).is_err() => {
                std::os::unix::fs::symlink(target, &path)?;
            }
            _ => {}
        }
    }

    std::fs::write(&marker, &build.version.name)?;
    Ok(java)
}

fn platform() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux",
        ("linux", "x86") => "linux-i386",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("windows", "aarch64") => "windows-arm64",
        (os, arch) => bail!("unsupported system {os}/{arch}"),
    })
}
