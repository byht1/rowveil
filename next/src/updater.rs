//! Updates of Rowveil Next: its own channel, GitHub pre-releases tagged `next-v*`.
//! The stable app's updater never sees these releases (it reads only the latest release).
//!
//! Download → SHA256 check against SHA256SUMS.txt → the running exe is renamed to
//! `.exe.old` and the new one takes its place → the app starts again.

use anyhow::{anyhow, bail, Context as _, Result};
use serde_json::Value;
use std::path::PathBuf;

const RELEASES: &str = "https://api.github.com/repos/byht1/rowveil/releases?per_page=40";
pub const ASSET: &str = "rowveil-next-windows-x86_64.exe";

#[derive(Clone, Debug, Default)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub asset: Option<(String, u64)>,
    pub sums: Option<String>,
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent("rowveil-next-updater").timeout(std::time::Duration::from_secs(30)).build()?)
}

/// The newest `next-v*` release, or None.
pub async fn latest() -> Result<Option<Release>> {
    let list: Value = client()?.get(RELEASES).header("Accept", "application/vnd.github+json").send().await.context("Cannot reach GitHub to check for updates")?.error_for_status()?.json().await?;
    let mut best: Option<Release> = None;
    for r in list.as_array().into_iter().flatten() {
        if r["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let Some(tag) = r["tag_name"].as_str().and_then(|t| t.strip_prefix("next-v")) else { continue };
        if best.as_ref().is_some_and(|b| !rowveil::update::is_newer(tag, &b.version)) {
            continue;
        }
        let assets = r["assets"].as_array().cloned().unwrap_or_default();
        let find = |n: &str| assets.iter().find(|a| a["name"] == n);
        best = Some(Release {
            version: tag.to_string(),
            notes: r["body"].as_str().unwrap_or("").to_string(),
            url: r["html_url"].as_str().unwrap_or("").to_string(),
            asset: find(ASSET).and_then(|a| Some((a["browser_download_url"].as_str()?.to_string(), a["size"].as_u64().unwrap_or(0)))),
            sums: find("SHA256SUMS.txt").and_then(|a| a["browser_download_url"].as_str().map(str::to_string)),
        });
    }
    Ok(best)
}

pub fn is_newer(r: &Release) -> bool {
    rowveil::update::is_newer(&r.version, current())
}

/// Downloads and verifies the new exe; returns its path (next to the running exe).
pub async fn download(r: Release, progress: impl Fn(u64, u64) + Send + 'static) -> Result<PathBuf> {
    let (url, size) = r.asset.clone().ok_or_else(|| anyhow!("The release has no {ASSET}"))?;
    let sums_url = r.sums.clone().ok_or_else(|| anyhow!("The release has no SHA256SUMS.txt"))?;
    let c = client()?;
    let sums = c.get(&sums_url).send().await?.error_for_status()?.text().await?;
    let expected = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (h, n) = (it.next()?, it.next()?);
            (n.trim_start_matches('*') == ASSET).then(|| h.to_lowercase())
        })
        .ok_or_else(|| anyhow!("SHA256SUMS.txt has no line for {ASSET}"))?;
    let mut res = reqwest::Client::builder().user_agent("rowveil-next-updater").build()?.get(&url).send().await?.error_for_status()?;
    let total = res.content_length().unwrap_or(size);
    let mut data = Vec::with_capacity(total as usize);
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    while let Some(chunk) = res.chunk().await? {
        ctx.update(&chunk);
        data.extend_from_slice(&chunk);
        progress(data.len() as u64, total);
    }
    let got: String = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if got != expected {
        bail!("The download is damaged (SHA-256 does not match). Try again.");
    }
    let exe = std::env::current_exe()?;
    let staged = exe.with_extension("exe.new");
    std::fs::write(&staged, data).context("Cannot write the downloaded file")?;
    Ok(staged)
}

/// Swaps the exe (a running exe can be renamed on Windows) and starts the new one.
pub fn install_and_restart(staged: PathBuf) -> Result<()> {
    let exe = std::env::current_exe()?;
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).context("Cannot move the running exe aside")?;
    if let Err(e) = std::fs::rename(&staged, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e).context("Cannot put the new exe in place");
    }
    std::process::Command::new(&exe).args(std::env::args().skip(1)).spawn().context("Cannot start the new version")?;
    Ok(())
}

/// Removes the previous exe left by an update.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
        let _ = std::fs::remove_file(exe.with_extension("exe.new"));
    }
}
