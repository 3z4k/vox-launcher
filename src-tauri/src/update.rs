//! Launcher updates from GitHub Releases: check on start, and optionally download and run the installer.

use crate::util::*;
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

fn api_base() -> String {
    std::env::var("VOX_UPDATE_URL").unwrap_or_else(|_| "https://api.github.com".to_string())
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub page_url: String,
    pub notes: String,
    /// Direct installer download, when the release has one.
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

/// "v1.2.3-beta" -> [1, 2, 3]
fn parse_version(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (a, b) = (parse_version(candidate), parse_version(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn valid_repo(repo: &str) -> bool {
    let mut it = repo.split('/');
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    matches!((it.next(), it.next(), it.next()), (Some(a), Some(b), None) if ok(a) && ok(b))
}

pub async fn latest_release(ctx: &Ctx, repo: &str) -> Result<Value> {
    if !valid_repo(repo) {
        bail!("No GitHub repository is set. Enter it in Settings → General → GitHub repository.");
    }
    let r = ctx.http.get(format!("{}/repos/{repo}/releases/latest", api_base())).header("Accept", "application/vnd.github+json").send().await?;
    if r.status().as_u16() == 404 {
        bail!("No releases have been published yet.");
    }
    if !r.status().is_success() {
        bail!("GitHub answered HTTP {}.", r.status().as_u16());
    }
    Ok(r.json().await?)
}

/// `Ok(None)` when the launcher is up to date (or no source is configured).
pub async fn check(ctx: &Ctx, repo: &str, current: &str) -> Result<Option<UpdateInfo>> {
    if !valid_repo(repo) {
        return Ok(None);
    }
    let rel = latest_release(ctx, repo).await?;
    let tag = rel["tag_name"].as_str().unwrap_or("");
    if tag.is_empty() || !is_newer(tag, current) {
        return Ok(None);
    }
    let asset = rel["assets"].as_array().and_then(|a| {
        a.iter().find(|x| {
            let n = x["name"].as_str().unwrap_or("").to_lowercase();
            n.ends_with(".exe") && (n.contains("setup") || n.contains("install"))
        })
    });
    Ok(Some(UpdateInfo {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        current: current.to_string(),
        page_url: rel["html_url"].as_str().unwrap_or("").to_string(),
        notes: rel["body"].as_str().unwrap_or("").chars().take(1500).collect(),
        asset_url: asset.and_then(|a| a["browser_download_url"].as_str()).map(String::from),
        asset_name: asset.and_then(|a| a["name"].as_str()).map(String::from),
        asset_size: asset.and_then(|a| a["size"].as_u64()),
    }))
}

/// Only files published in this repository's releases may be downloaded and run.
pub fn asset_url_allowed(url: &str, repo: &str) -> bool {
    valid_repo(repo) && url.starts_with(&format!("https://github.com/{repo}/releases/download/")) && !url.contains("..")
}

/// Downloads the installer into a temp folder and returns its path.
pub async fn download_installer(ctx: &Ctx, repo: &str, url: &str, name: &str, size: Option<u64>) -> Result<PathBuf> {
    if !asset_url_allowed(url, repo) {
        bail!("That download is not from the Vox Launcher releases.");
    }
    let name = std::path::Path::new(name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !name.to_lowercase().ends_with(".exe") {
        bail!("The update is not an installer.");
    }
    let dest = std::env::temp_dir().join("vox-update").join(&name);
    let _ = tokio::fs::remove_file(&dest).await;
    download(&ctx.http, url, &dest, &Expect { sha1: None, size }).await?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.3.1", "0.3.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("v0.2.9", "0.3.0"));
        assert!(!is_newer("0.3.0-beta", "0.3.0"));
        assert!(!is_newer("garbage", "0.3.0"));
    }

    #[test]
    fn only_this_repos_release_assets_are_allowed() {
        let ok = "https://github.com/me/vox/releases/download/v1/Vox_setup.exe";
        assert!(asset_url_allowed(ok, "me/vox"));
        assert!(!asset_url_allowed(ok, "other/vox"));
        assert!(!asset_url_allowed("https://evil.example/releases/download/x.exe", "me/vox"));
        assert!(!asset_url_allowed("https://github.com/me/vox/releases/download/../x.exe", "me/vox"));
        assert!(!asset_url_allowed(ok, ""));
        assert!(!valid_repo("a/b/c") && !valid_repo("a") && valid_repo("a-b/c_d.e"));
    }
}
