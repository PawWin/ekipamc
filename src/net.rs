//! HTTP downloads with hash verification and progress callbacks.

use crate::paths::ensure_parent;
use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, stream};
use serde::de::DeserializeOwned;
use sha1::Digest;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const PARALLEL_DOWNLOADS: usize = 24;
const ATTEMPTS: usize = 3;

/// A file to fetch. Known size and sha1 let us skip files already on disk
/// and reject corrupted ones.
#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Download {
    /// Cheap presence check: existence plus size, no rehashing.
    fn is_present(&self) -> bool {
        std::fs::metadata(&self.path)
            .is_ok_and(|meta| self.size.is_none_or(|size| meta.len() == size))
    }
}

#[derive(Clone)]
pub struct Net {
    client: reqwest::Client,
}

impl Net {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("ekipamc/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub async fn json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        response
            .json()
            .await
            .with_context(|| format!("bad JSON from {url}"))
    }

    /// Fetches every missing file, `PARALLEL_DOWNLOADS` at a time.
    /// `on_progress(done, total)` counts only the files that needed fetching.
    pub async fn download_all(
        &self,
        files: &[Download],
        on_progress: impl Fn(usize, usize),
    ) -> Result<()> {
        let missing: Vec<Download> = files
            .iter()
            .filter(|file| !file.is_present())
            .cloned()
            .collect();
        let total = missing.len();
        // Owned values keep the futures 'static-friendly for `tokio::spawn`.
        let mut results = stream::iter(missing)
            .map(|file| {
                let net = self.clone();
                async move { net.download(&file).await }
            })
            .buffer_unordered(PARALLEL_DOWNLOADS);
        let mut done = 0;
        while let Some(result) = results.next().await {
            result?;
            done += 1;
            on_progress(done, total);
        }
        Ok(())
    }

    /// Downloads one file, retrying transient failures.
    pub async fn download(&self, file: &Download) -> Result<()> {
        let mut last_error = None;
        for _ in 0..ATTEMPTS {
            match self
                .fetch_verified::<sha1::Sha1>(
                    &file.url,
                    &file.path,
                    file.sha1.as_deref(),
                    |_, _| {},
                )
                .await
            {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("at least one attempt"))
    }

    /// Streams a large file to disk, verifying its sha256.
    /// `on_bytes(received, total)` is throttled to roughly every megabyte.
    pub async fn download_sha256(
        &self,
        url: &str,
        path: &Path,
        sha256: &str,
        on_bytes: impl Fn(u64, Option<u64>),
    ) -> Result<()> {
        self.fetch_verified::<sha2::Sha256>(url, path, Some(sha256), on_bytes)
            .await
    }

    /// Writes to `<path>.part`, checks the digest, then renames into place,
    /// so an interrupted download never looks complete.
    async fn fetch_verified<H: Digest>(
        &self,
        url: &str,
        path: &Path,
        expected: Option<&str>,
        on_bytes: impl Fn(u64, Option<u64>),
    ) -> Result<()> {
        ensure_parent(path)?;
        let part = path.with_added_extension("part");
        let response = self
            .client
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .with_context(|| format!("cannot download {url}"))?;
        let total = response.content_length();

        let mut out = tokio::fs::File::create(&part).await?;
        let mut hasher = H::new();
        let mut received = 0u64;
        let mut reported = 0u64;
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.with_context(|| format!("download of {url} was interrupted"))?;
            hasher.update(&chunk);
            out.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if received - reported >= 1 << 20 {
                reported = received;
                on_bytes(received, total);
            }
        }
        out.flush().await?;
        drop(out);

        if let Some(expected) = expected {
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                tokio::fs::remove_file(&part).await.ok();
                bail!("{url} is corrupted (hash {actual}, expected {expected})");
            }
        }
        tokio::fs::rename(&part, path).await?;
        on_bytes(received, total);
        Ok(())
    }
}
