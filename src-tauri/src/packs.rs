//! Importing and exporting: Modrinth modpacks (.mrpack) and Vox instance archives (.zip).

use crate::instances::*;
use crate::mojang::Report;
use crate::types::*;
use crate::util::*;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A path from an archive, made safe: relative, no `..`, no drive letters.
pub fn safe_relative(p: &str) -> Result<PathBuf> {
    let p = p.replace('\\', "/");
    let path = Path::new(&p);
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => bail!("Unsafe path in archive: {p}"),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("Empty path in archive");
    }
    if p.contains(':') {
        bail!("Unsafe path in archive: {p}");
    }
    Ok(out)
}

#[derive(Deserialize)]
struct IndexFile {
    path: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default, rename = "fileSize")]
    file_size: Option<u64>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct Index {
    name: String,
    #[serde(default)]
    files: Vec<IndexFile>,
    dependencies: HashMap<String, String>,
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Extracts every entry under `prefix` (for example "overrides/") into `dest`.
fn extract_prefix(zip: &mut zip::ZipArchive<std::fs::File>, prefix: &str, dest: &Path) -> Result<()> {
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let name = f.name().to_string();
        let Some(rel) = name.strip_prefix(prefix) else { continue };
        if rel.is_empty() || f.is_dir() {
            continue;
        }
        let rel = safe_relative(rel)?;
        let out = dest.join(rel);
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::io::copy(&mut f, &mut std::fs::File::create(out)?)?;
    }
    Ok(())
}

/// Creates an instance from a Modrinth modpack file and downloads everything it lists.
pub async fn import_mrpack(ctx: &Ctx, file: &Path, report: &Report, threads: usize) -> Result<InstanceConfig> {
    let f = file.to_path_buf();
    let index: Index = tokio::task::spawn_blocking(move || -> Result<Index> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&f)?).context("This is not a valid modpack file.")?;
        let raw = read_entry(&mut zip, "modrinth.index.json").ok_or_else(|| anyhow!("modrinth.index.json is missing from the modpack."))?;
        Ok(serde_json::from_slice(&raw).context("The modpack index is invalid.")?)
    })
    .await??;

    let mc = index.dependencies.get("minecraft").cloned().ok_or_else(|| anyhow!("The modpack does not say which Minecraft version it needs."))?;
    let (loader, loader_version) = if let Some(v) = index.dependencies.get("fabric-loader") {
        (Loader::Fabric, v.clone())
    } else if let Some(v) = index.dependencies.get("quilt-loader") {
        (Loader::Quilt, v.clone())
    } else if let Some(v) = index.dependencies.get("neoforge") {
        (Loader::Neoforge, v.clone())
    } else if let Some(v) = index.dependencies.get("forge") {
        (Loader::Forge, v.clone())
    } else {
        (Loader::Vanilla, String::new())
    };

    let inst = create_instance(
        ctx,
        NewInstance { name: index.name.clone(), mc_version: mc, loader, loader_version, group: "Modpacks".into(), icon: "pack".into() },
    )
    .await?;
    let game = ctx.game_dir(&inst.id);

    let result: Result<()> = async {
        // Files listed in the index
        let mut tasks = Vec::new();
        for f in index.files.iter().filter(|f| f.env.as_ref().and_then(|e| e.get("client")).map(|c| c != "unsupported").unwrap_or(true)) {
            let rel = safe_relative(&f.path)?;
            tasks.push((game.join(rel), f.downloads.clone(), Expect { sha1: f.hashes.get("sha1").cloned(), size: f.file_size }));
        }
        let total = tasks.len() as u64;
        let done = Arc::new(AtomicU64::new(0));
        report("Downloading modpack files", 0, total);
        let (http, rep, d2) = (ctx.http.clone(), report.clone(), done.clone());
        pool(tasks, threads, move |(dest, urls, expect)| {
            let (http, rep, done) = (http.clone(), rep.clone(), d2.clone());
            async move {
                if !file_is_good(&dest, &expect, false).await {
                    let mut last = anyhow!("No download link for {}", dest.display());
                    let mut ok = false;
                    for u in &urls {
                        match download(&http, u, &dest, &expect).await {
                            Ok(()) => {
                                ok = true;
                                break;
                            }
                            Err(e) => last = e,
                        }
                    }
                    if !ok {
                        return Err(last);
                    }
                }
                rep("Downloading modpack files", done.fetch_add(1, Ordering::Relaxed) + 1, total);
                Ok(())
            }
        })
        .await?;

        report("Applying modpack overrides", 0, 1);
        let (f, g) = (file.to_path_buf(), game.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(&f)?)?;
            extract_prefix(&mut zip, "overrides/", &g)?;
            extract_prefix(&mut zip, "client-overrides/", &g)
        })
        .await??;
        Ok(())
    }
    .await;

    if let Err(e) = result {
        let _ = delete_instance(ctx, &inst.id).await; // do not leave a half-installed pack behind
        return Err(e);
    }
    report("Modpack ready", 1, 1);
    Ok(inst)
}

// ---- Vox instance archives ---------------------------------------------------------------

const EXPORT_SKIP_DIRS: &[&str] = &["logs", "crash-reports", ".vox-cache"];

/// Writes the instance (settings + game folder) to `dest`. `include_worlds` = false leaves out `saves/`.
pub async fn export_instance(ctx: &Ctx, id: &str, dest: &Path, include_worlds: bool, report: &Report) -> Result<()> {
    let inst = get_instance(ctx, id).await?;
    let game = ctx.game_dir(id);
    let dest = dest.to_path_buf();
    let report = report.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>, include_worlds: bool) -> std::io::Result<()> {
            for e in std::fs::read_dir(dir)? {
                let e = e?;
                let p = e.path();
                let rel = p.strip_prefix(base).unwrap().to_path_buf();
                let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
                if EXPORT_SKIP_DIRS.contains(&top.as_str()) || (!include_worlds && top == "saves") {
                    continue;
                }
                if e.file_type()?.is_dir() {
                    walk(&p, base, out, include_worlds)?;
                } else {
                    out.push(rel);
                }
            }
            Ok(())
        }
        let mut files = Vec::new();
        if game.exists() {
            walk(&game, &game, &mut files, include_worlds)?;
        }
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&dest)?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("vox-instance.json", opts)?;
        zip.write_all(&serde_json::to_vec_pretty(&inst)?)?;
        let total = files.len() as u64;
        for (i, rel) in files.iter().enumerate() {
            let name = format!("minecraft/{}", rel.to_string_lossy().replace('\\', "/"));
            zip.start_file(name, opts)?;
            std::io::copy(&mut std::fs::File::open(game.join(rel))?, &mut zip)?;
            if i % 20 == 0 {
                report("Exporting instance", i as u64, total);
            }
        }
        zip.finish()?;
        report("Exported", total, total);
        Ok(())
    })
    .await?
}

/// Imports a `.mrpack` or a Vox instance `.zip`.
pub async fn import_file(ctx: &Ctx, file: &Path, report: &Report, threads: usize) -> Result<InstanceConfig> {
    let f = file.to_path_buf();
    let kind = tokio::task::spawn_blocking(move || -> Result<&'static str> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&f)?).map_err(|_| anyhow!("That file is not a zip or modpack archive."))?;
        if zip.by_name("vox-instance.json").is_ok() {
            Ok("vox")
        } else if zip.by_name("modrinth.index.json").is_ok() {
            Ok("mrpack")
        } else if zip.by_name("manifest.json").is_ok() {
            bail!("CurseForge modpacks are not supported. Use the Modrinth version of the pack, or export it as .mrpack.")
        } else {
            bail!("This archive is not an instance export or a Modrinth modpack.")
        }
    })
    .await??;
    if kind == "mrpack" {
        return import_mrpack(ctx, file, report, threads).await;
    }

    let f = file.to_path_buf();
    let cfg: InstanceConfig = tokio::task::spawn_blocking(move || -> Result<InstanceConfig> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&f)?)?;
        let raw = read_entry(&mut zip, "vox-instance.json").ok_or_else(|| anyhow!("vox-instance.json is missing."))?;
        Ok(serde_json::from_slice(&raw).context("The instance settings in this archive are invalid.")?)
    })
    .await??;
    let inst = create_instance(
        ctx,
        NewInstance { name: cfg.name.clone(), mc_version: cfg.mc_version.clone(), loader: cfg.loader, loader_version: cfg.loader_version.clone(), group: cfg.group.clone(), icon: cfg.icon.clone() },
    )
    .await?;
    let (f, g) = (file.to_path_buf(), ctx.game_dir(&inst.id));
    report("Importing instance", 0, 1);
    let r = tokio::task::spawn_blocking(move || -> Result<()> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&f)?)?;
        extract_prefix(&mut zip, "minecraft/", &g)
    })
    .await?;
    if let Err(e) = r {
        let _ = delete_instance(ctx, &inst.id).await;
        return Err(e);
    }
    let mut patch = serde_json::to_value(&cfg)?;
    for k in ["id", "name", "mcVersion", "loader", "loaderVersion", "created", "lastPlayed", "playedMs", "verifyNext", "versionId"] {
        patch.as_object_mut().unwrap().remove(k);
    }
    let out = update_instance(ctx, &inst.id, patch).await?;
    report("Imported", 1, 1);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_cannot_escape() {
        assert!(safe_relative("mods/a.jar").is_ok());
        assert!(safe_relative("./config/x.toml").is_ok());
        assert!(safe_relative("../evil").is_err());
        assert!(safe_relative("a/../../evil").is_err());
        assert!(safe_relative("/etc/passwd").is_err());
        assert!(safe_relative("C:\\Windows\\x").is_err());
        assert!(safe_relative("").is_err());
    }
}
