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

/// What the launcher remembers about a modpack instance: where it came from and which files the
/// pack put there. This is what lets a later update replace only the pack's own files.
#[derive(Clone, Debug, Default, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PackState {
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
    pub name: String,
    /// Files listed in the pack index (paths relative to the game folder).
    pub files: Vec<String>,
    /// Files unpacked from the pack's overrides, with the hash they had when written.
    pub overrides: std::collections::BTreeMap<String, String>,
}

fn state_path(ctx: &Ctx, id: &str) -> PathBuf {
    ctx.instance_dir(id).join("modpack.json")
}

pub async fn load_state(ctx: &Ctx, id: &str) -> Option<PackState> {
    read_json::<PackState>(&state_path(ctx, id)).await.ok()
}

/// Remembers which Modrinth modpack version an instance was installed from.
pub async fn set_origin(ctx: &Ctx, id: &str, project_id: &str, version_id: &str, version_number: &str) -> Result<()> {
    let mut st = load_state(ctx, id).await.unwrap_or_default();
    st.project_id = project_id.to_string();
    st.version_id = version_id.to_string();
    st.version_number = version_number.to_string();
    write_json(&state_path(ctx, id), &st).await
}

fn sha1_bytes(b: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    hex::encode(Sha1::digest(b))
}

fn read_index(file: &Path) -> Result<Index> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file)?).context("This is not a valid modpack file.")?;
    let raw = read_entry(&mut zip, "modrinth.index.json").ok_or_else(|| anyhow!("modrinth.index.json is missing from the modpack."))?;
    Ok(serde_json::from_slice(&raw).context("The modpack index is invalid.")?)
}

fn loader_of(index: &Index) -> Result<(String, Loader, String)> {
    let mc = index.dependencies.get("minecraft").cloned().ok_or_else(|| anyhow!("The modpack does not say which Minecraft version it needs."))?;
    let d = &index.dependencies;
    let (loader, v) = if let Some(v) = d.get("fabric-loader") {
        (Loader::Fabric, v.clone())
    } else if let Some(v) = d.get("quilt-loader") {
        (Loader::Quilt, v.clone())
    } else if let Some(v) = d.get("neoforge") {
        (Loader::Neoforge, v.clone())
    } else if let Some(v) = d.get("forge") {
        (Loader::Forge, v.clone())
    } else {
        (Loader::Vanilla, String::new())
    };
    Ok((mc, loader, v))
}

/// Unpacks overrides without clobbering the player's own edits: a file is replaced only when it is
/// missing, or still exactly as this launcher wrote it last time. Returns the new hash table.
fn extract_overrides(
    file: &Path,
    game: &Path,
    old: &std::collections::BTreeMap<String, String>,
) -> Result<(std::collections::BTreeMap<String, String>, std::collections::BTreeSet<String>)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file)?)?;
    let mut table = old.clone();
    let mut seen = std::collections::BTreeSet::new();
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let name = f.name().to_string();
            let Some(rel) = name.strip_prefix(prefix) else { continue };
            if rel.is_empty() || f.is_dir() {
                continue;
            }
            let rel_path = safe_relative(rel)?;
            let key = rel_path.to_string_lossy().replace('\\', "/");
            let mut data = Vec::new();
            f.read_to_end(&mut data)?;
            let out = game.join(&rel_path);
            let write = match std::fs::read(&out) {
                Err(_) => true,
                Ok(cur) => {
                    let cur_hash = sha1_bytes(&cur);
                    // Untouched since we wrote it (or written moments ago by the base overrides).
                    old.get(&key).map(|h| *h == cur_hash).unwrap_or(false) || table.get(&key).map(|h| *h == cur_hash && seen.contains(&key)).unwrap_or(false)
                }
            };
            if write {
                if let Some(dir) = out.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&out, &data)?;
                table.insert(key.clone(), sha1_bytes(&data));
            }
            seen.insert(key);
        }
    }
    Ok((table, seen))
}

fn remove_with_disabled(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(PathBuf::from(format!("{}.disabled", path.display())));
}

/// Makes the game folder match `file` (a .mrpack). Works for a fresh folder and for updating in place:
/// files the old version had and the new one drops are removed, everything else the player has stays.
async fn apply_pack(ctx: &Ctx, game: &Path, file: &Path, index: &Index, old: &PackState, threads: usize, verify: bool, report: &Report) -> Result<PackState> {
    let mut tasks = Vec::new();
    let mut files = Vec::new();
    for f in index.files.iter().filter(|f| f.env.as_ref().and_then(|e| e.get("client")).map(|c| c != "unsupported").unwrap_or(true)) {
        let rel = safe_relative(&f.path)?;
        files.push(rel.to_string_lossy().replace('\\', "/"));
        tasks.push((game.join(rel), f.downloads.clone(), Expect { sha1: f.hashes.get("sha1").cloned(), size: f.file_size }));
    }
    // Drop what the old version had and this one no longer lists.
    for gone in old.files.iter().filter(|p| !files.contains(p)) {
        if let Ok(rel) = safe_relative(gone) {
            remove_with_disabled(&game.join(rel));
        }
    }
    let total = tasks.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    report("Downloading modpack files", 0, total);
    let (http, rep, d2) = (ctx.http.clone(), report.clone(), done.clone());
    pool(tasks, threads, move |(dest, urls, expect)| {
        let (http, rep, done) = (http.clone(), rep.clone(), d2.clone());
        async move {
            let disabled = PathBuf::from(format!("{}.disabled", dest.display()));
            // A mod the player switched off stays off as long as it is the right file.
            let have = file_is_good(&dest, &expect, verify).await || file_is_good(&disabled, &expect, verify).await;
            if !have {
                let mut last = anyhow!("No download link for {}", dest.display());
                let mut ok = false;
                for u in &urls {
                    match download(&http, u, &dest, &expect).await {
                        Ok(()) => {
                            ok = true;
                            let _ = tokio::fs::remove_file(&disabled).await;
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
    let (f, g, oldo) = (file.to_path_buf(), game.to_path_buf(), old.overrides.clone());
    let (mut overrides, present) = tokio::task::spawn_blocking(move || extract_overrides(&f, &g, &oldo)).await??;
    // Override files the new version dropped are removed only if the player never edited them.
    for (k, h) in &old.overrides {
        if present.contains(k) {
            continue;
        }
        if let Ok(rel) = safe_relative(k) {
            let p = game.join(rel);
            if std::fs::read(&p).map(|b| sha1_bytes(&b) == *h).unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    overrides.retain(|k, _| present.contains(k));
    Ok(PackState { name: index.name.clone(), files, overrides, ..old.clone() })
}

/// Creates an instance from a Modrinth modpack file and downloads everything it lists.
pub async fn import_mrpack(ctx: &Ctx, file: &Path, report: &Report, threads: usize) -> Result<InstanceConfig> {
    let f = file.to_path_buf();
    let index = tokio::task::spawn_blocking(move || read_index(&f)).await??;
    let (mc, loader, loader_version) = loader_of(&index)?;
    let inst = create_instance(ctx, NewInstance { name: index.name.clone(), mc_version: mc, loader, loader_version, group: "Modpacks".into(), icon: "pack".into() }).await?;
    let game = ctx.game_dir(&inst.id);
    match apply_pack(ctx, &game, file, &index, &PackState::default(), threads, false, report).await {
        Ok(state) => write_json(&state_path(ctx, &inst.id), &state).await?,
        Err(e) => {
            let _ = delete_instance(ctx, &inst.id).await; // do not leave a half-installed pack behind
            return Err(e);
        }
    }
    report("Modpack ready", 1, 1);
    Ok(inst)
}

/// Updates an installed modpack to the version in `file`, keeping worlds, settings and the player's own mods.
/// A snapshot is taken first so the update can be rolled back.
pub async fn update_pack_in_place(ctx: &Ctx, id: &str, file: &Path, project_id: &str, version_id: &str, version_number: &str, threads: usize, report: &Report) -> Result<InstanceConfig> {
    let inst = get_instance(ctx, id).await?;
    let f = file.to_path_buf();
    let index = tokio::task::spawn_blocking(move || read_index(&f)).await??;
    let (mc, loader, loader_version) = loader_of(&index)?;
    let old = load_state(ctx, id).await.unwrap_or_default();
    report("Saving a snapshot", 0, 1);
    crate::snapshots::create(ctx, id, &format!("Before updating to {version_number}")).await?;
    let game = ctx.game_dir(id);
    let mut state = apply_pack(ctx, &game, file, &index, &old, threads, true, report).await?;
    state.project_id = project_id.to_string();
    state.version_id = version_id.to_string();
    state.version_number = version_number.to_string();
    write_json(&state_path(ctx, id), &state).await?;
    let mut out = inst.clone();
    if mc != inst.mc_version || loader != inst.loader || loader_version != inst.loader_version {
        // A new Minecraft or loader version: the loader is installed again on the next launch.
        out.mc_version = mc;
        out.loader = loader;
        out.loader_version = loader_version;
        out.version_id = String::new();
        write_json(&ctx.instance_dir(id).join("instance.json"), &out).await?;
    }
    report("Modpack updated", 1, 1);
    Ok(out)
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
