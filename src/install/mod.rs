//! Brings the data directory up to date: Java, Minecraft, NeoForge, assets
//! and the modpack. Every step skips work already done, so after the first
//! run this is quick and works offline.

pub mod java;
mod minecraft;
mod modpack;
mod neoforge;

use crate::game::version::VersionJson;
use crate::net::Net;
use crate::pack::Pack;
use crate::paths::Paths;
use anyhow::{Context, Result};

/// Receives install status for display. `fraction` is `None` when the step
/// has no measurable progress.
pub trait Progress: Sync {
    fn update(&self, stage: &str, detail: &str, fraction: Option<f32>);
}

pub async fn install(
    net: &Net,
    paths: &Paths,
    pack: &Pack,
    progress: &impl Progress,
) -> Result<()> {
    std::fs::create_dir_all(paths.instance())?;

    progress.update("Preparing Minecraft", "", None);
    minecraft::ensure_version_file(net, paths, &pack.minecraft).await?;
    let vanilla = VersionJson::resolve(&paths.versions(), &pack.minecraft)?;
    let component = &vanilla
        .java_version
        .as_ref()
        .context("no Java version in version file")?
        .component;

    progress.update("Installing Java", "", None);
    let java = java::ensure(
        net,
        paths,
        component,
        count_files(progress, "Installing Java"),
    )
    .await?;

    progress.update("Installing NeoForge", "This can take a minute", None);
    neoforge::ensure(net, paths, pack, &java).await?;
    let version = VersionJson::resolve(&paths.versions(), &pack.version_id())?;

    progress.update("Downloading libraries", "", None);
    minecraft::ensure_libraries(
        net,
        paths,
        &version,
        count_files(progress, "Downloading libraries"),
    )
    .await?;

    progress.update("Downloading assets", "", None);
    minecraft::ensure_assets(
        net,
        paths,
        &version,
        count_files(progress, "Downloading assets"),
    )
    .await?;

    if let Some(pack) = &pack.modpack {
        progress.update("Checking modpack", "", None);
        modpack::ensure(net, paths, pack, progress).await?;
    }
    std::fs::remove_dir_all(paths.tmp()).ok();
    Ok(())
}

fn count_files<'a>(progress: &'a impl Progress, stage: &'a str) -> impl Fn(usize, usize) + 'a {
    move |done, total| {
        progress.update(
            stage,
            &format!("{done} / {total} files"),
            Some(done as f32 / total as f32),
        );
    }
}
