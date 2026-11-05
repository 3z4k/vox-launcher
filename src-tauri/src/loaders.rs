//! Mod loader support: Fabric/Quilt via meta profiles, Forge/NeoForge via their official installers.

use crate::mojang::Report;
use crate::types::{InstanceConfig, Loader};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::path::Path;

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// NeoForge versions are numbered after the Minecraft version: 1.21.1 -> 21.1.x, 1.21 -> 21.0.x.
pub fn neoforge_prefix(mc: &str) -> Option<String> {
    let re = regex::Regex::new(r"^1\.(\d+)(?:\.(\d+))?$").unwrap();
    let c = re.captures(mc)?;
    Some(format!("{}.{}.", &c[1], c.get(2).map(|m| m.as_str()).unwrap_or("0")))
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}
#[derive(Deserialize)]
struct MetaLoader {
    version: String,
}

/// Pulls every <version> out of a Maven metadata file, in file order.
pub fn parse_maven_versions(xml: &str) -> Vec<String> {
    let re = regex::Regex::new(r"<version>([^<]+)</version>").unwrap();
    re.captures_iter(xml).map(|c| c[1].to_string()).collect()
}

pub async fn list_loader_versions(ctx: &Ctx, loader: Loader, mc: &str) -> Result<Vec<String>> {
    Ok(match loader {
        Loader::Vanilla => vec![],
        Loader::Fabric => {
            let r: Vec<MetaEntry> = get_json(&ctx.http, &format!("https://meta.fabricmc.net/v2/versions/loader/{}", enc(mc))).await?;
            r.into_iter().map(|x| x.loader.version).collect()
        }
        Loader::Quilt => {
            let r: Vec<MetaEntry> = get_json(&ctx.http, &format!("https://meta.quiltmc.org/v3/versions/loader/{}", enc(mc))).await?;
            r.into_iter().map(|x| x.loader.version).collect()
        }
        Loader::Forge => {
            let xml = ctx.http.get("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml").send().await?.text().await?;
            let prefix = format!("{mc}-");
            let mut v: Vec<String> = parse_maven_versions(&xml)
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .map(|v| v[prefix.len()..].to_string())
                .collect();
            v.reverse();
            v
        }
        Loader::Neoforge => {
            let Some(prefix) = neoforge_prefix(mc) else { return Ok(vec![]) }; // snapshots / pre-1.20.2 use another scheme
            #[derive(Deserialize)]
            struct R {
                versions: Vec<String>,
            }
            let r: R = get_json(&ctx.http, "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?;
            let mut v: Vec<String> = r.versions.into_iter().filter(|v| v.starts_with(&prefix)).collect();
            v.reverse();
            v
        }
    })
}

async fn install_profile_loader(ctx: &Ctx, base: &str, mc: &str, lv: &str) -> Result<String> {
    let profile: serde_json::Value = get_json(&ctx.http, &format!("{base}/{}/{}/profile/json", enc(mc), enc(lv))).await?;
    let id = profile["id"].as_str().ok_or_else(|| anyhow!("The loader profile has no id."))?.to_string();
    write_json(&ctx.version_json_path(&id), &profile).await?;
    Ok(id)
}

/// Reads the version id an installer jar is going to create.
fn installer_version_id(jar: &Path) -> Result<String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut read = |name: &str| -> Option<serde_json::Value> {
        let mut f = zip.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        serde_json::from_str(&s).ok()
    };
    if let Some(v) = read("version.json") {
        if let Some(id) = v["id"].as_str() {
            return Ok(id.to_string());
        }
    }
    if let Some(v) = read("install_profile.json") {
        if let Some(id) = v["versionInfo"]["id"].as_str() {
            return Ok(id.to_string());
        }
    }
    bail!("Could not read the version id from the installer.")
}

async fn run_installer(java: &Path, jar: &Path, target: &Path) -> Result<()> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-jar").arg(jar).arg("--installClient").arg(target).current_dir(jar.parent().unwrap_or(Path::new(".")));
    cmd.kill_on_drop(true);
    hide_window(&mut cmd);
    let out = cmd.output().await?;
    if out.status.success() {
        return Ok(());
    }
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let tail: Vec<&str> = text.trim().lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
    bail!("The installer failed (exit code {}).\n{}", out.status.code().map(|c| c.to_string()).unwrap_or("?".into()), tail.join("\n"))
}

async fn install_with_installer(ctx: &Ctx, url: &str, label: &str, java: &Path, report: &Report) -> Result<String> {
    let dir = ctx.cache_dir().join("installers");
    tokio::fs::create_dir_all(&dir).await?;
    let name = url.rsplit('/').next().unwrap_or("installer.jar");
    let jar = dir.join(name);
    report(&format!("Downloading {label} installer"), 0, 1);
    download(&ctx.http, url, &jar, &Expect::default()).await?;
    let jar2 = jar.clone();
    let version_id = tokio::task::spawn_blocking(move || installer_version_id(&jar2)).await??;

    // The installer expects a launcher_profiles.json next to the game files.
    let profiles = ctx.root.join("launcher_profiles.json");
    if !tokio::fs::try_exists(&profiles).await.unwrap_or(false) {
        write_json(&profiles, &serde_json::json!({ "profiles": {}, "version": 3 })).await?;
    }
    report(&format!("Installing {label} (this can take a minute)"), 0, 1);
    run_installer(java, &jar, &ctx.root).await?;
    if !tokio::fs::try_exists(ctx.version_json_path(&version_id)).await.unwrap_or(false) {
        bail!("{label} installed, but {version_id} was not created.");
    }
    Ok(version_id)
}

/// Installs the loader for an instance and returns the version id to launch.
pub async fn install_loader(ctx: &Ctx, inst: &InstanceConfig, java: &Path, report: &Report) -> Result<String> {
    let (mc, lv) = (inst.mc_version.as_str(), inst.loader_version.as_str());
    if inst.loader == Loader::Vanilla {
        return Ok(mc.to_string());
    }
    if lv.is_empty() {
        bail!("No loader version selected for this instance.");
    }
    tokio::fs::create_dir_all(ctx.versions_dir()).await?;
    match inst.loader {
        Loader::Fabric => {
            report("Installing Fabric", 0, 1);
            install_profile_loader(ctx, "https://meta.fabricmc.net/v2/versions/loader", mc, lv).await
        }
        Loader::Quilt => {
            report("Installing Quilt", 0, 1);
            install_profile_loader(ctx, "https://meta.quiltmc.org/v3/versions/loader", mc, lv).await
        }
        Loader::Forge => {
            let full = format!("{mc}-{lv}");
            let url = format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar");
            install_with_installer(ctx, &url, "Forge", java, report).await
        }
        Loader::Neoforge => {
            let url = format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{lv}/neoforge-{lv}-installer.jar");
            install_with_installer(ctx, &url, "NeoForge", java, report).await
        }
        Loader::Vanilla => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("24w14a"), None);
    }

    #[test]
    fn maven_metadata_parsing() {
        let xml = "<versions><version>1.20.1-47.2.0</version><version>1.21.1-52.0.1</version></versions>";
        assert_eq!(parse_maven_versions(xml), ["1.20.1-47.2.0", "1.21.1-52.0.1"]);
    }
}
