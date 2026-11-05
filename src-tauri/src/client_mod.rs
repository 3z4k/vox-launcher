//! The optional Vox Client mod (HUD, keystrokes, local cosmetics). It lives in this project's GitHub
//! releases as `vox-client-<version>+mc<minecraft>.jar` and is installed into one instance's mods folder.
//! Removing it deletes that one file, nothing else.

use crate::mojang::Report;
use crate::types::{InstanceConfig, Loader};
use crate::util::*;
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

pub const PREFIX: &str = "vox-client-";
const FABRIC_API_PROJECT: &str = "P7dR8mSH";

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    /// File name in the instance's mods folder, when installed.
    pub installed: Option<String>,
    /// Minecraft versions the latest release supports (empty when no source is configured).
    pub supported: Vec<String>,
    /// False when no GitHub repository is known, so nothing can be downloaded.
    pub source_known: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub mc: String,
}

/// "vox-client-0.1.0+mc1.21.4.jar" -> "1.21.4"
fn mc_of(name: &str) -> Option<String> {
    let stem = name.strip_prefix(PREFIX)?.strip_suffix(".jar")?;
    let (_, mc) = stem.split_once("+mc")?;
    (!mc.is_empty() && mc.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')).then(|| mc.to_string())
}

pub fn assets_of(release: &Value) -> Vec<Asset> {
    release["assets"].as_array().map(|a| {
        a.iter().filter_map(|x| {
            let name = x["name"].as_str()?.to_string();
            Some(Asset { mc: mc_of(&name)?, url: x["browser_download_url"].as_str()?.to_string(), size: x["size"].as_u64().unwrap_or(0), name })
        }).collect()
    }).unwrap_or_default()
}

fn mods_dir(ctx: &Ctx, id: &str) -> PathBuf {
    ctx.game_dir(id).join("mods")
}

fn installed_file(dir: &std::path::Path) -> Option<String> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().to_string())
        .find(|n| n.starts_with(PREFIX) && (n.ends_with(".jar") || n.ends_with(".jar.disabled")))
}

pub async fn status(ctx: &Ctx, repo: &str, id: &str) -> ClientStatus {
    let supported = match crate::update::latest_release(ctx, repo).await {
        Ok(r) => assets_of(&r).into_iter().map(|a| a.mc).collect(),
        Err(_) => Vec::new(),
    };
    ClientStatus { installed: installed_file(&mods_dir(ctx, id)), supported, source_known: !repo.is_empty() }
}

pub async fn install(ctx: &Ctx, repo: &str, inst: &InstanceConfig, report: &Report) -> Result<String> {
    if !matches!(inst.loader, Loader::Fabric | Loader::Quilt) {
        bail!("Vox Client runs on Fabric or Quilt. Create or pick an instance that uses one of them.");
    }
    if repo.is_empty() {
        bail!("This launcher does not know which GitHub repository to download Vox Client from. Enter it in Settings → General → GitHub repository.");
    }
    report("Finding Vox Client", 0, 3);
    let release = crate::update::latest_release(ctx, repo).await?;
    let assets = assets_of(&release);
    let Some(asset) = assets.iter().find(|a| a.mc == inst.mc_version) else {
        let have: Vec<_> = assets.iter().map(|a| a.mc.as_str()).collect();
        bail!("Vox Client is not available for Minecraft {} yet. It supports: {}.", inst.mc_version, if have.is_empty() { "nothing published".into() } else { have.join(", ") });
    };
    if !crate::update::asset_url_allowed(&asset.url, repo) {
        bail!("That download is not from the Vox Launcher releases.");
    }
    let dir = mods_dir(ctx, &inst.id);
    tokio::fs::create_dir_all(&dir).await?;

    // The mod needs Fabric API for its HUD hooks.
    let have_api = std::fs::read_dir(&dir).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains("fabric-api"))).unwrap_or(false);
    if !have_api {
        report("Installing Fabric API", 1, 3);
        let versions = crate::modrinth::project_versions(ctx, FABRIC_API_PROJECT, "mods", &inst.mc_version, Some(inst.loader)).await?;
        let Some(v) = versions.first() else { bail!("Fabric API is not available for Minecraft {}.", inst.mc_version) };
        crate::modrinth::install_with_dependencies(ctx, inst, "mods", v, "Fabric API", None, report).await?;
    }

    report("Downloading Vox Client", 2, 3);
    remove(ctx, &inst.id).await?;
    let dest = dir.join(&asset.name);
    download(&ctx.http, &asset.url, &dest, &Expect { sha1: None, size: (asset.size > 0).then_some(asset.size) }).await?;
    report("Done", 3, 3);
    Ok(asset.name.clone())
}

pub async fn remove(ctx: &Ctx, id: &str) -> Result<()> {
    let dir = mods_dir(ctx, id);
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with(PREFIX) && (n.ends_with(".jar") || n.ends_with(".jar.disabled")) {
                tokio::fs::remove_file(e.path()).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn asset_names_carry_the_minecraft_version() {
        assert_eq!(mc_of("vox-client-0.1.0+mc1.21.4.jar").as_deref(), Some("1.21.4"));
        assert_eq!(mc_of("vox-client-0.1.0+mc25w02a.jar").as_deref(), Some("25w02a"));
        assert_eq!(mc_of("vox-client-0.1.0.jar"), None);
        assert_eq!(mc_of("other-0.1.0+mc1.21.4.jar"), None);
        assert_eq!(mc_of("vox-client-0.1.0+mc../x.jar"), None);
        let rel = json!({"assets": [
            {"name": "vox-client-0.1.0+mc1.21.4.jar", "browser_download_url": "https://github.com/a/b/releases/download/v1/x.jar", "size": 5},
            {"name": "Vox_setup.exe", "browser_download_url": "https://github.com/a/b/releases/download/v1/s.exe", "size": 9},
        ]});
        let a = assets_of(&rel);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].mc, "1.21.4");
    }

    #[tokio::test]
    async fn remove_only_touches_vox_client_files() {
        let ctx = Ctx::new(std::env::temp_dir().join(format!("vox-client-{}-{}", std::process::id(), now_ms())));
        let dir = mods_dir(&ctx, "i");
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["vox-client-0.1.0+mc1.21.4.jar", "sodium.jar", "vox-client-notes.txt"] {
            std::fs::write(dir.join(n), "x").unwrap();
        }
        assert!(installed_file(&dir).is_some());
        remove(&ctx, "i").await.unwrap();
        assert!(installed_file(&dir).is_none());
        assert!(dir.join("sodium.jar").exists() && dir.join("vox-client-notes.txt").exists());
    }
}
