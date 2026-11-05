//! Version manifest, version resolution, and making sure every game file is on disk.

use crate::core::*;
use crate::util::*;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// stage, current, total
pub type Report = Arc<dyn Fn(&str, u64, u64) + Send + Sync>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub release_time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "releaseTime")]
    pub release_time: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub versions: Vec<ManifestEntry>,
}

/// Manifest base URL; tests point this at a local fake server.
pub fn manifest_url() -> String {
    std::env::var("VOX_MANIFEST_URL").unwrap_or_else(|_| MANIFEST_URL.to_string())
}

fn manifest_cache() -> &'static Mutex<Option<(std::time::Instant, Manifest)>> {
    static C: std::sync::OnceLock<Mutex<Option<(std::time::Instant, Manifest)>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

pub async fn get_manifest(ctx: &Ctx) -> Result<Manifest> {
    if let Some((at, m)) = manifest_cache().lock().unwrap().as_ref() {
        if at.elapsed().as_secs() < 600 {
            return Ok(m.clone());
        }
    }
    let cache_file = ctx.cache_dir().join("version_manifest.json");
    let fresh = get_json::<Manifest>(&ctx.http, &manifest_url()).await;
    let m = match fresh {
        Ok(m) => {
            let _ = write_json(&cache_file, &m).await;
            m
        }
        // offline fallback
        Err(e) => read_json::<Manifest>(&cache_file).await.map_err(|_| e)?,
    };
    *manifest_cache().lock().unwrap() = Some((std::time::Instant::now(), m.clone()));
    Ok(m)
}

pub async fn list_versions(ctx: &Ctx) -> Result<Vec<McVersion>> {
    let m = get_manifest(ctx).await?;
    Ok(m.versions
        .into_iter()
        .map(|v| McVersion { id: v.id, kind: v.kind, release_time: v.release_time })
        .collect())
}

/// Loads one version JSON, downloading it from Mojang when it is not installed locally.
pub async fn load_version_json(ctx: &Ctx, id: &str) -> Result<VersionJson> {
    let file = ctx.version_json_path(id);
    if let Ok(v) = read_json::<VersionJson>(&file).await {
        return Ok(v);
    }
    let manifest = get_manifest(ctx).await?;
    let entry = manifest.versions.iter().find(|v| v.id == id).ok_or_else(|| anyhow!("Unknown Minecraft version \"{id}\"."))?;
    let raw: Value = get_json(&ctx.http, &entry.url).await?;
    write_json(&file, &raw).await?;
    Ok(serde_json::from_value(raw)?)
}

/// Loads a version and folds in everything it inherits from.
pub fn resolve_version<'a>(ctx: &'a Ctx, id: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<VersionJson>> + Send + 'a>> {
    Box::pin(async move {
        let mut json = load_version_json(ctx, id).await?;
        match json.inherits_from.clone() {
            None => {
                json.jar_id = json.jar.clone().unwrap_or_else(|| json.id.clone());
                Ok(json)
            }
            Some(parent_id) => {
                let parent = resolve_version(ctx, &parent_id).await?;
                Ok(merge_versions(parent, json))
            }
        }
    })
}

#[derive(Clone, Debug)]
pub struct LegacyAssets {
    pub r#virtual: bool,
    pub map_to_resources: bool,
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    #[serde(default)]
    r#virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Clone, Debug)]
pub struct NativeJar {
    pub file: PathBuf,
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GamePlan {
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<NativeJar>,
    pub asset_index_id: String,
    pub assets_root: PathBuf,
    pub log_arg: Option<String>,
    pub legacy_assets: Option<LegacyAssets>,
}

#[derive(Clone)]
struct Task {
    url: String,
    dest: PathBuf,
    expect: Expect,
}

async fn run_downloads(ctx: &Ctx, stage: &str, tasks: Vec<Task>, report: &Report, verify: bool, limit: usize) -> Result<()> {
    let total = tasks.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    report(stage, 0, total);
    let http = ctx.http.clone();
    let rep = report.clone();
    let stage_s = stage.to_string();
    pool(tasks, limit, move |t| {
        let (http, done, rep, stage) = (http.clone(), done.clone(), rep.clone(), stage_s.clone());
        async move {
            if !file_is_good(&t.dest, &t.expect, verify).await {
                download(&http, &t.url, &t.dest, &t.expect).await?;
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            rep(&stage, n, total);
            Ok(())
        }
    })
    .await
}

/// Makes sure the client jar, libraries, assets and log config for a version are on disk.
pub async fn ensure_game(ctx: &Ctx, v: &VersionJson, report: &Report, verify: bool, threads: usize) -> Result<GamePlan> {
    let platform = current_platform();
    let libs = effective_libraries(v, &platform);
    let libs_dir = ctx.libraries_dir();

    let mut tasks: Vec<Task> = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut natives: Vec<NativeJar> = Vec::new();

    for lib in libs {
        if let Some(art) = artifact_of(lib) {
            let dest = libs_dir.join(&art.path);
            classpath.push(dest.clone());
            // Installer-generated libraries (Forge/NeoForge) have an empty url and already exist on disk.
            if !art.url.is_empty() {
                tasks.push(Task { url: art.url, dest, expect: Expect { sha1: art.sha1, size: art.size } });
            }
        }
        for n in native_artifacts(lib, &platform) {
            let dest = libs_dir.join(&n.file.path);
            if !n.file.url.is_empty() {
                tasks.push(Task { url: n.file.url.clone(), dest: dest.clone(), expect: Expect { sha1: n.file.sha1.clone(), size: n.file.size } });
            }
            natives.push(NativeJar { file: dest, exclude: n.exclude });
        }
    }

    let jar = ctx.version_jar_path(&v.jar_id);
    if let Some(c) = v.downloads.as_ref().and_then(|d| d.client.as_ref()) {
        tasks.push(Task { url: c.url.clone(), dest: jar.clone(), expect: Expect { sha1: Some(c.sha1.clone()), size: Some(c.size) } });
    }
    classpath.push(jar);

    let mut log_arg = None;
    if let Some(cfg) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let dest = ctx.assets_dir().join("log_configs").join(&cfg.file.id);
        tasks.push(Task {
            url: cfg.file.url.clone(),
            dest: dest.clone(),
            expect: Expect { sha1: Some(cfg.file.sha1.clone()), size: Some(cfg.file.size) },
        });
        log_arg = Some(cfg.argument.replace("${path}", &dest.to_string_lossy()));
    }

    // Natives-only libraries show up both as a classpath entry and as a native: download each file once,
    // otherwise two workers race on the same temp file.
    let mut seen = std::collections::HashSet::new();
    tasks.retain(|t| seen.insert(t.dest.clone()));
    run_downloads(ctx, "Downloading libraries", tasks, report, verify, (threads / 2).clamp(4, 16)).await?;

    // Assets
    let idx = v.asset_index.as_ref().ok_or_else(|| anyhow!("This version has no asset index."))?;
    let index_file = ctx.assets_dir().join("indexes").join(format!("{}.json", idx.id));
    let expect = Expect { sha1: Some(idx.sha1.clone()), size: Some(idx.size) };
    if !file_is_good(&index_file, &expect, verify).await {
        download(&ctx.http, &idx.url, &index_file, &expect).await?;
    }
    let index: AssetIndex = read_json(&index_file).await?;

    // A stamp is written once every asset of this version has been downloaded, so later launches skip
    // looking at the thousands of asset files at all. "Verify" forces a full check.
    #[derive(Serialize, Deserialize)]
    struct Stamp {
        v: u32,
        #[serde(rename = "assetIndex")]
        asset_index: String,
    }
    let stamp_file = ctx.version_dir(&v.id).join(".vox-ready.json");
    let stamp_ok = !verify && read_json::<Stamp>(&stamp_file).await.map(|s| s.v == 1 && s.asset_index == idx.id).unwrap_or(false);
    if !stamp_ok {
        let mut unique: HashMap<&str, u64> = HashMap::new();
        for o in index.objects.values() {
            unique.insert(o.hash.as_str(), o.size);
        }
        let objects = ctx.assets_dir().join("objects");
        let resources = std::env::var("VOX_RESOURCES_URL").unwrap_or_else(|_| RESOURCES_URL.to_string());
        let asset_tasks: Vec<Task> = unique
            .into_iter()
            .map(|(hash, size)| Task {
                url: format!("{resources}/{}/{hash}", &hash[..2]),
                dest: objects.join(&hash[..2]).join(hash),
                expect: Expect { sha1: Some(hash.to_string()), size: Some(size) },
            })
            .collect();
        run_downloads(ctx, "Downloading assets", asset_tasks, report, verify, threads.max(1)).await?;
        write_json(&stamp_file, &Stamp { v: 1, asset_index: idx.id.clone() }).await?;
    }

    let legacy = (index.r#virtual || index.map_to_resources)
        .then(|| LegacyAssets { r#virtual: index.r#virtual, map_to_resources: index.map_to_resources, objects: index.objects.clone() });
    Ok(GamePlan { classpath, natives, asset_index_id: idx.id.clone(), assets_root: ctx.assets_dir(), log_arg, legacy_assets: legacy })
}

/// Copies hashed assets into the readable layout that very old versions expect.
pub async fn materialize_legacy_assets(ctx: &Ctx, plan: &GamePlan, game_directory: &Path) -> Result<PathBuf> {
    let Some(legacy) = &plan.legacy_assets else { return Ok(plan.assets_root.clone()) };
    let target_root = if legacy.r#virtual {
        ctx.assets_dir().join("virtual").join(&plan.asset_index_id)
    } else {
        game_directory.join("resources")
    };
    for (name, obj) in &legacy.objects {
        let src = ctx.assets_dir().join("objects").join(&obj.hash[..2]).join(&obj.hash);
        let mut dest = target_root.clone();
        for part in name.split('/') {
            if part.is_empty() || part == ".." {
                bail!("Unsafe asset name {name}");
            }
            dest.push(part);
        }
        if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
            continue;
        }
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::copy(&src, &dest).await.with_context(|| format!("copy asset {name}"))?;
    }
    Ok(if legacy.r#virtual { target_root } else { plan.assets_root.clone() })
}

/// Unpacks native libraries into a clean directory, unless the same natives are already unpacked there.
pub async fn extract_natives(plan: &GamePlan, dir: &Path) -> Result<()> {
    let marker = dir.join(".vox-natives.json");
    let mut sig: Vec<(String, u64)> = Vec::new();
    for n in &plan.natives {
        sig.push((n.file.to_string_lossy().to_string(), tokio::fs::metadata(&n.file).await.with_context(|| format!("native library {} is missing", n.file.display()))?.len()));
    }
    let signature = serde_json::to_string(&sig)?;
    #[derive(Serialize, Deserialize)]
    struct Marker {
        sig: String,
    }
    if read_json::<Marker>(&marker).await.map(|m| m.sig == signature).unwrap_or(false) {
        return Ok(());
    }
    let dir_buf = dir.to_path_buf();
    let jars = plan.natives.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let _ = std::fs::remove_dir_all(&dir_buf);
        std::fs::create_dir_all(&dir_buf)?;
        for n in &jars {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(&n.file)?)?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_string();
                if n.exclude.iter().any(|ex| name.starts_with(ex.as_str())) {
                    continue;
                }
                let Some(base) = Path::new(&name).file_name() else { continue };
                let mut out = std::fs::File::create(dir_buf.join(base))?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
        Ok(())
    })
    .await??;
    // written last, so a half-finished extraction is redone next time
    write_json(&marker, &Marker { sig: signature }).await
}
