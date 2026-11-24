//! Downloads the hosted modpack zip and unpacks it into the instance.
//!
//! On a new pack version `mods/` is wiped so removed mods disappear; every
//! other file in the zip overwrites its copy. Files the zip doesn't contain
//! (saves, options.txt, screenshots) are left alone.

use super::Progress;
use crate::net::Net;
use crate::pack::Modpack;
use crate::paths::{Paths, ensure_parent};
use anyhow::{Context, Result};
use std::path::Path;

/// Holds the sha256 of the installed zip.
const MARKER: &str = ".modpack";

pub async fn ensure(
    net: &Net,
    paths: &Paths,
    modpack: &Modpack,
    game_dir: &Path,
    progress: &impl Progress,
) -> Result<()> {
    let marker = game_dir.join(MARKER);
    if std::fs::read_to_string(&marker).is_ok_and(|sha| sha.eq_ignore_ascii_case(&modpack.sha256)) {
        return Ok(());
    }

    let zip_path = paths.tmp().join("modpack.zip");
    let stage = format!("Downloading modpack {}", modpack.version);
    net.download_sha256(
        &modpack.url,
        &zip_path,
        &modpack.sha256,
        |received, total| {
            let mb = |bytes: u64| bytes / (1 << 20);
            match total {
                Some(total) => progress.update(
                    &stage,
                    &format!("{} / {} MB", mb(received), mb(total)),
                    Some(received as f32 / total as f32),
                ),
                None => progress.update(&stage, &format!("{} MB", mb(received)), None),
            }
        },
    )
    .await?;

    progress.update("Unpacking modpack", "", None);
    let target = game_dir.to_path_buf();
    let source = zip_path.clone();
    tokio::task::spawn_blocking(move || extract(&source, &target)).await??;

    std::fs::write(&marker, &modpack.sha256)?;
    std::fs::remove_file(&zip_path).ok();
    Ok(())
}

fn extract(zip_path: &Path, instance: &Path) -> Result<()> {
    let mods = instance.join("mods");
    if mods.exists() {
        std::fs::remove_dir_all(&mods).context("cannot clear old mods")?;
    }

    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)
        .context("modpack zip is unreadable")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        // enclosed_name rejects absolute paths and `..` escapes.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let path = instance.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
        } else {
            ensure_parent(&path)?;
            std::io::copy(&mut entry, &mut std::fs::File::create(&path)?)?;
        }
    }
    Ok(())
}
