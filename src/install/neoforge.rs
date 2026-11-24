//! Runs the official NeoForge installer headlessly against our data root.
//! It writes `versions/neoforge-<v>/` and patches the game into `libraries/`.

use crate::net::{Download, Net};
use crate::pack::Pack;
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use std::process::Stdio;

pub async fn ensure(net: &Net, paths: &Paths, pack: &Pack, java: &std::path::Path) -> Result<()> {
    if paths.version_file(&pack.version_id(), "json").exists() {
        return Ok(());
    }

    // The installer refuses to run without a launcher profile file.
    let profiles = paths.root.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, r#"{"profiles":{}}"#)?;
    }

    let v = &pack.neoforge;
    let installer = paths.tmp().join(format!("neoforge-{v}-installer.jar"));
    net.download(&Download {
        url: format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar"),
        path: installer.clone(),
        sha1: None,
        size: None,
    })
    .await?;

    std::fs::create_dir_all(paths.logs())?;
    let log_path = paths.logs().join("neoforge-installer.log");
    let log = std::fs::File::create(&log_path)?;
    let status = super::java::command(java)
        .arg("-jar")
        .arg(&installer)
        .arg("--install-client")
        .arg(&paths.root)
        .current_dir(paths.tmp())
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
        .await
        .context("cannot run the NeoForge installer")?;
    if !status.success() {
        bail!("NeoForge installer failed, see {}", log_path.display());
    }
    Ok(())
}
