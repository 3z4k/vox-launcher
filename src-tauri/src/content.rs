//! Everything inside an instance's game folder that the UI lists and manages:
//! mods/packs, worlds, screenshots, servers, logs, and syncing options between installs.

use crate::modrinth::{load_meta, safe_filename, save_meta, CONTENT_KINDS};
use crate::nbt::{self, Tag};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

fn modified_ms(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

// ---- mods / resource packs / shader packs -----------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// File name without the ".disabled" suffix.
    pub name: String,
    pub size: u64,
    pub enabled: bool,
    pub is_dir: bool,
    pub modified: u64,
    pub title: String,
    pub version: String,
    pub icon_url: Option<String>,
    pub project_id: Option<String>,
}

/// Reads the display name and version out of a mod jar (Fabric, Quilt, Forge/NeoForge).
pub fn read_jar_info(path: &Path) -> Option<(String, String)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let f = zip.by_name(name).ok()?;
        let mut s = String::new();
        f.take(256 * 1024).read_to_string(&mut s).ok()?;
        Some(s)
    };
    if let Some(s) = read("fabric.mod.json") {
        let v: serde_json::Value = serde_json::from_str(&s).ok()?;
        let name = v["name"].as_str().or_else(|| v["id"].as_str())?;
        return Some((name.to_string(), v["version"].as_str().unwrap_or_default().to_string()));
    }
    if let Some(s) = read("quilt.mod.json") {
        let v: serde_json::Value = serde_json::from_str(&s).ok()?;
        let l = &v["quilt_loader"];
        let name = l["metadata"]["name"].as_str().or_else(|| l["id"].as_str())?;
        return Some((name.to_string(), l["version"].as_str().unwrap_or_default().to_string()));
    }
    for f in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(s) = read(f) {
            let grab = |key: &str| -> Option<String> {
                let re = regex::Regex::new(&format!(r#"(?m)^\s*{key}\s*=\s*"([^"]*)""#)).ok()?;
                re.captures(&s).map(|c| c[1].to_string())
            };
            if let Some(name) = grab("displayName") {
                return Some((name, grab("version").filter(|v| !v.starts_with("${")).unwrap_or_default()));
            }
        }
    }
    None
}

pub async fn list_files(ctx: &Ctx, id: &str, kind: &str) -> Result<Vec<FileEntry>> {
    if !CONTENT_KINDS.contains(&kind) {
        bail!("Unknown content type \"{kind}\".");
    }
    let dir = ctx.game_dir(id).join(kind);
    let meta = load_meta(ctx, id).await;
    let kind = kind.to_string();
    tokio::task::spawn_blocking(move || -> Result<Vec<FileEntry>> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(out) };
        for e in rd.flatten() {
            let raw = e.file_name().to_string_lossy().to_string();
            if raw.starts_with('.') || raw.ends_with(".part") {
                continue;
            }
            let (name, enabled) = match raw.strip_suffix(".disabled") {
                Some(n) => (n.to_string(), false),
                None => (raw.clone(), true),
            };
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir { dir_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
            let m = meta.get(&format!("{kind}/{name}"));
            let (mut title, mut version) = m.map(|m| (m.title.clone(), m.version_number.clone())).unwrap_or_default();
            if title.is_empty() && kind == "mods" && !is_dir {
                if let Some((t, v)) = read_jar_info(&e.path()) {
                    title = t;
                    version = v;
                }
            }
            out.push(FileEntry {
                title: if title.is_empty() { name.trim_end_matches(".jar").trim_end_matches(".zip").to_string() } else { title },
                name,
                size,
                enabled,
                is_dir,
                modified: modified_ms(&e.path()),
                version,
                icon_url: m.and_then(|m| m.icon_url.clone()),
                project_id: m.map(|m| m.project_id.clone()),
            });
        }
        out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(out)
    })
    .await?
}

fn content_path(ctx: &Ctx, id: &str, kind: &str, name: &str) -> Result<PathBuf> {
    if !CONTENT_KINDS.contains(&kind) {
        bail!("Unknown content type \"{kind}\".");
    }
    Ok(ctx.game_dir(id).join(kind).join(safe_filename(name)?))
}

pub async fn set_file_enabled(ctx: &Ctx, id: &str, kind: &str, name: &str, enabled: bool) -> Result<()> {
    let on = content_path(ctx, id, kind, name)?;
    let off = PathBuf::from(format!("{}.disabled", on.display()));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() {
        tokio::fs::rename(from, to).await?;
    }
    Ok(())
}

pub async fn delete_file(ctx: &Ctx, id: &str, kind: &str, name: &str) -> Result<()> {
    let on = content_path(ctx, id, kind, name)?;
    for p in [on.clone(), PathBuf::from(format!("{}.disabled", on.display()))] {
        if p.is_dir() {
            tokio::fs::remove_dir_all(&p).await?;
        } else if p.exists() {
            tokio::fs::remove_file(&p).await?;
        }
    }
    let mut meta = load_meta(ctx, id).await;
    if meta.remove(&format!("{kind}/{}", safe_filename(name)?)).is_some() {
        save_meta(ctx, id, &meta).await?;
    }
    Ok(())
}

/// Copies a file the user picked into the content folder.
pub async fn add_file(ctx: &Ctx, id: &str, kind: &str, src: &Path) -> Result<String> {
    let name = safe_filename(&src.file_name().ok_or_else(|| anyhow!("No file selected."))?.to_string_lossy())?;
    let dest = content_path(ctx, id, kind, &name)?;
    if let Some(d) = dest.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    tokio::fs::copy(src, &dest).await?;
    Ok(name)
}

// ---- worlds -------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    pub folder: String,
    pub name: String,
    pub last_played: u64,
    pub game_type: String,
    pub size: u64,
    pub icon_path: Option<String>,
}

fn game_type_name(n: i64) -> &'static str {
    match n {
        0 => "Survival",
        1 => "Creative",
        2 => "Adventure",
        3 => "Spectator",
        _ => "Unknown",
    }
}

fn read_level_dat(path: &Path) -> Option<Tag> {
    let raw = std::fs::read(path).ok()?;
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut data).ok()?;
    nbt::parse(&data).ok()
}

pub async fn list_worlds(ctx: &Ctx, id: &str) -> Result<Vec<World>> {
    let dir = ctx.game_dir(id).join("saves");
    tokio::task::spawn_blocking(move || -> Result<Vec<World>> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(out) };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let folder = e.file_name().to_string_lossy().to_string();
            let level = read_level_dat(&p.join("level.dat"));
            let data = level.as_ref().and_then(|l| l.get("Data"));
            let name = data.and_then(|d| d.get("LevelName")).and_then(Tag::as_str).unwrap_or(&folder).to_string();
            let last = data.and_then(|d| d.get("LastPlayed")).and_then(Tag::as_i64).map(|v| v as u64).unwrap_or_else(|| modified_ms(&p));
            let gt = data.and_then(|d| d.get("GameType")).and_then(Tag::as_i64).map(game_type_name).unwrap_or("Unknown").to_string();
            let icon = p.join("icon.png");
            out.push(World {
                folder,
                name,
                last_played: last,
                game_type: gt,
                size: dir_size(&p),
                icon_path: icon.exists().then(|| icon.to_string_lossy().to_string()),
            });
        }
        out.sort_by_key(|w| std::cmp::Reverse(w.last_played));
        Ok(out)
    })
    .await?
}

fn world_path(ctx: &Ctx, id: &str, folder: &str) -> Result<PathBuf> {
    Ok(ctx.game_dir(id).join("saves").join(safe_filename(folder)?))
}

pub async fn delete_world(ctx: &Ctx, id: &str, folder: &str) -> Result<()> {
    let p = world_path(ctx, id, folder)?;
    if !p.is_dir() {
        bail!("World not found.");
    }
    tokio::fs::remove_dir_all(p).await?;
    Ok(())
}

/// Zips a world into `<instance>/minecraft/world-backups`. Returns the archive path.
pub async fn backup_world(ctx: &Ctx, id: &str, folder: &str) -> Result<PathBuf> {
    use std::io::Write;
    let src = world_path(ctx, id, folder)?;
    if !src.is_dir() {
        bail!("World not found.");
    }
    let dest_dir = ctx.game_dir(id).join("world-backups");
    let dest = dest_dir.join(format!("{}-{}.zip", safe_filename(folder)?, now_ms()));
    let d2 = dest.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&dest_dir)?;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&d2)?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let base = src.parent().unwrap().to_path_buf();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name().map(|n| n != "session.lock").unwrap_or(true) {
                    zip.start_file(p.strip_prefix(&base)?.to_string_lossy().replace('\\', "/"), opts)?;
                    zip.write_all(&std::fs::read(&p)?)?;
                }
            }
        }
        zip.finish()?;
        Ok(())
    })
    .await??;
    Ok(dest)
}

// ---- screenshots --------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: u64,
}

pub async fn list_screenshots(ctx: &Ctx, id: &str) -> Result<Vec<Screenshot>> {
    let dir = ctx.game_dir(id).join("screenshots");
    tokio::task::spawn_blocking(move || {
        let mut out: Vec<Screenshot> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("png") || x.eq_ignore_ascii_case("jpg")).unwrap_or(false))
                    .map(|e| Screenshot {
                        name: e.file_name().to_string_lossy().to_string(),
                        path: e.path().to_string_lossy().to_string(),
                        size: e.metadata().map(|m| m.len()).unwrap_or(0),
                        modified: modified_ms(&e.path()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|s| std::cmp::Reverse(s.modified));
        Ok(out)
    })
    .await?
}

pub async fn delete_screenshot(ctx: &Ctx, id: &str, name: &str) -> Result<()> {
    let p = ctx.game_dir(id).join("screenshots").join(safe_filename(name)?);
    tokio::fs::remove_file(p).await?;
    Ok(())
}

// ---- servers.dat ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Server {
    pub name: String,
    pub ip: String,
    /// data: URL of the server icon, if the game saved one
    #[serde(default)]
    pub icon: Option<String>,
}

fn servers_file(ctx: &Ctx, id: &str) -> PathBuf {
    ctx.game_dir(id).join("servers.dat")
}

fn server_items(root: &Tag) -> Vec<&Tag> {
    match root.get("servers") {
        Some(Tag::List(_, items)) => items.iter().collect(),
        _ => vec![],
    }
}

pub async fn list_servers(ctx: &Ctx, id: &str) -> Result<Vec<Server>> {
    let Ok(bytes) = tokio::fs::read(servers_file(ctx, id)).await else { return Ok(vec![]) };
    let root = nbt::parse(&bytes)?;
    Ok(server_items(&root)
        .into_iter()
        .map(|s| Server {
            name: s.get("name").and_then(Tag::as_str).unwrap_or("Minecraft Server").to_string(),
            ip: s.get("ip").and_then(Tag::as_str).unwrap_or_default().to_string(),
            icon: s.get("icon").and_then(Tag::as_str).filter(|i| !i.is_empty()).map(|i| format!("data:image/png;base64,{i}")),
        })
        .collect())
}

/// Saves the list the UI sends. Servers that are still in the list keep everything the game stored for them.
pub async fn save_servers(ctx: &Ctx, id: &str, servers: Vec<Server>) -> Result<()> {
    let file = servers_file(ctx, id);
    let old = match tokio::fs::read(&file).await {
        Ok(b) => nbt::parse(&b).unwrap_or(Tag::Compound(vec![])),
        Err(_) => Tag::Compound(vec![]),
    };
    let old_items: Vec<Tag> = server_items(&old).into_iter().cloned().collect();
    let mut items = Vec::new();
    for s in servers {
        let name = s.name.trim().to_string();
        let ip = s.ip.trim().to_string();
        if ip.is_empty() {
            bail!("Every server needs an address.");
        }
        let existing = old_items.iter().find(|o| o.get("ip").and_then(Tag::as_str) == Some(ip.as_str())).cloned();
        let mut t = existing.unwrap_or_else(|| Tag::Compound(vec![]));
        t.set("name", Tag::String(if name.is_empty() { "Minecraft Server".into() } else { name }));
        t.set("ip", Tag::String(ip));
        items.push(t);
    }
    let mut root = old;
    if !matches!(root, Tag::Compound(_)) {
        root = Tag::Compound(vec![]);
    }
    root.set("servers", Tag::List(10, items));
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::write(&file, nbt::write(&root)).await?;
    Ok(())
}

// ---- logs ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    /// relative to the game folder, e.g. "logs/latest.log"
    pub path: String,
    pub size: u64,
    pub modified: u64,
}

pub async fn list_logs(ctx: &Ctx, id: &str) -> Result<Vec<LogFile>> {
    let game = ctx.game_dir(id);
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for sub in ["logs", "crash-reports"] {
            if let Ok(rd) = std::fs::read_dir(game.join(sub)) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.ends_with(".log") || n.ends_with(".log.gz") || n.ends_with(".txt") {
                        out.push(LogFile { path: format!("{sub}/{n}"), size: e.metadata().map(|m| m.len()).unwrap_or(0), modified: modified_ms(&e.path()) });
                    }
                }
            }
        }
        out.sort_by_key(|l| std::cmp::Reverse(l.modified));
        Ok(out)
    })
    .await?
}

/// Reads the last ~512 KB of a log (".gz" files are unpacked first).
pub async fn read_log(ctx: &Ctx, id: &str, rel: &str) -> Result<String> {
    let (dir, name) = rel.split_once('/').ok_or_else(|| anyhow!("Invalid log path."))?;
    if dir != "logs" && dir != "crash-reports" {
        bail!("Invalid log path.");
    }
    let p = ctx.game_dir(id).join(dir).join(safe_filename(name)?);
    tokio::task::spawn_blocking(move || -> Result<String> {
        let raw = std::fs::read(&p)?;
        let bytes = if p.extension().map(|e| e == "gz").unwrap_or(false) {
            let mut v = Vec::new();
            flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut v)?;
            v
        } else {
            raw
        };
        const MAX: usize = 512 * 1024;
        let slice = if bytes.len() > MAX { &bytes[bytes.len() - MAX..] } else { &bytes[..] };
        Ok(String::from_utf8_lossy(slice).into_owned())
    })
    .await?
}

// ---- options sync --------------------------------------------------------------------------

pub const SYNC_FILES: &[&str] = &["options.txt", "optionsof.txt", "optionsshaders.txt", "servers.dat"];
/// What is copied automatically before each launch (servers.dat only when asked for).
pub const AUTO_SYNC_FILES: &[&str] = &["options.txt", "optionsof.txt", "optionsshaders.txt"];

/// The standard .minecraft folder of the official launcher, if it exists.
pub fn minecraft_dir() -> Option<PathBuf> {
    let p = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?).join(".minecraft")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/minecraft")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".minecraft")
    };
    p.is_dir().then_some(p)
}

fn sync_source(ctx: &Ctx, source: &str) -> Result<PathBuf> {
    if source == "mc" {
        std::env::var_os("VOX_MC_DIR").map(PathBuf::from).or_else(minecraft_dir).ok_or_else(|| anyhow!("The standard .minecraft folder was not found on this computer."))
    } else {
        crate::instances::assert_safe_id(source)?;
        Ok(ctx.game_dir(source))
    }
}

/// Copies option files from `source` ("mc" or an instance id) into instance `id`. Returns the files copied.
pub async fn sync_options(ctx: &Ctx, id: &str, source: &str, files: &[String]) -> Result<Vec<String>> {
    if source == id {
        bail!("An instance cannot sync with itself.");
    }
    let from = sync_source(ctx, source)?;
    let to = ctx.game_dir(id);
    tokio::fs::create_dir_all(&to).await?;
    let mut copied = Vec::new();
    for f in files {
        if !SYNC_FILES.contains(&f.as_str()) {
            continue;
        }
        let src = from.join(f);
        if src.is_file() {
            tokio::fs::copy(&src, to.join(f)).await?;
            copied.push(f.clone());
        }
    }
    Ok(copied)
}

pub async fn read_icon_data_url(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn ctx() -> Ctx {
        static CTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ctx::new(std::env::temp_dir().join(format!("vox-content-{}-{}-{}", std::process::id(), now_ms(), id)))
    }
    fn game(c: &Ctx) -> PathBuf {
        let g = c.game_dir("inst-1");
        std::fs::create_dir_all(&g).unwrap();
        g
    }
    fn jar(path: &Path, files: &[(&str, &str)]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (n, d) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[tokio::test]
    async fn mod_listing_toggle_delete_and_jar_names() {
        let c = ctx();
        let mods = game(&c).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        jar(&mods.join("fabric-thing.jar"), &[("fabric.mod.json", r#"{"id":"thing","name":"Thing Mod","version":"1.2.3"}"#)]);
        jar(&mods.join("forge-thing.jar"), &[("META-INF/mods.toml", "[[mods]]\nmodId=\"x\"\nversion=\"4.5\"\ndisplayName=\"Forge Thing\"\n")]);
        jar(&mods.join("plain.jar"), &[("a.txt", "x")]);
        let list = list_files(&c, "inst-1", "mods").await.unwrap();
        let by = |n: &str| list.iter().find(|f| f.name == n).unwrap().clone();
        assert_eq!((by("fabric-thing.jar").title.as_str(), by("fabric-thing.jar").version.as_str()), ("Thing Mod", "1.2.3"));
        assert_eq!((by("forge-thing.jar").title.as_str(), by("forge-thing.jar").version.as_str()), ("Forge Thing", "4.5"));
        assert_eq!(by("plain.jar").title, "plain");

        set_file_enabled(&c, "inst-1", "mods", "plain.jar", false).await.unwrap();
        assert!(mods.join("plain.jar.disabled").exists() && !mods.join("plain.jar").exists());
        let list = list_files(&c, "inst-1", "mods").await.unwrap();
        assert!(!list.iter().find(|f| f.name == "plain.jar").unwrap().enabled);
        set_file_enabled(&c, "inst-1", "mods", "plain.jar", true).await.unwrap();
        assert!(mods.join("plain.jar").exists());
        delete_file(&c, "inst-1", "mods", "plain.jar").await.unwrap();
        assert!(!mods.join("plain.jar").exists());
        assert!(delete_file(&c, "inst-1", "mods", "..").await.is_err());
        assert!(content_path(&c, "inst-1", "mods", "../../x").unwrap().starts_with(c.game_dir("inst-1").join("mods")), "directory parts are stripped, never followed");
        assert!(list_files(&c, "inst-1", "saves").await.is_err());
    }

    #[tokio::test]
    async fn worlds_are_read_from_level_dat() {
        let c = ctx();
        let w = game(&c).join("saves/My World");
        std::fs::create_dir_all(&w).unwrap();
        let level = Tag::Compound(vec![(
            "Data".into(),
            Tag::Compound(vec![("LevelName".into(), Tag::String("Cool Base".into())), ("LastPlayed".into(), Tag::Long(1_700_000_000_000)), ("GameType".into(), Tag::Int(1))]),
        )]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&nbt::write(&level)).unwrap();
        std::fs::write(w.join("level.dat"), gz.finish().unwrap()).unwrap();
        std::fs::create_dir_all(game(&c).join("saves/Broken")).unwrap();
        let worlds = list_worlds(&c, "inst-1").await.unwrap();
        assert_eq!(worlds.len(), 2);
        let cool = worlds.iter().find(|w| w.folder == "My World").unwrap();
        assert_eq!((cool.name.as_str(), cool.game_type.as_str(), cool.last_played), ("Cool Base", "Creative", 1_700_000_000_000));
        let broken = worlds.iter().find(|w| w.folder == "Broken").unwrap();
        assert_eq!(broken.name, "Broken", "a folder without a readable level.dat falls back to its folder name");
        let zip = backup_world(&c, "inst-1", "My World").await.unwrap();
        assert!(zip.exists());
        delete_world(&c, "inst-1", "My World").await.unwrap();
        assert!(delete_world(&c, "inst-1", "..").await.is_err());
        assert_eq!(list_worlds(&c, "inst-1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn servers_keep_game_data_for_servers_that_stay() {
        let c = ctx();
        game(&c);
        assert!(list_servers(&c, "inst-1").await.unwrap().is_empty());
        let root = Tag::Compound(vec![(
            "servers".into(),
            Tag::List(10, vec![Tag::Compound(vec![
                ("name".into(), Tag::String("Hypixel".into())),
                ("ip".into(), Tag::String("mc.hypixel.net".into())),
                ("icon".into(), Tag::String("QUJD".into())),
                ("acceptTextures".into(), Tag::Byte(1)),
            ])]),
        )]);
        std::fs::write(servers_file(&c, "inst-1"), nbt::write(&root)).unwrap();
        let l = list_servers(&c, "inst-1").await.unwrap();
        assert_eq!((l[0].name.as_str(), l[0].icon.as_deref()), ("Hypixel", Some("data:image/png;base64,QUJD")));
        save_servers(&c, "inst-1", vec![
            Server { name: "Renamed".into(), ip: "mc.hypixel.net".into(), icon: None },
            Server { name: "Mine".into(), ip: "localhost:25565".into(), icon: None },
        ])
        .await
        .unwrap();
        let again = nbt::parse(&std::fs::read(servers_file(&c, "inst-1")).unwrap()).unwrap();
        let items = server_items(&again);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("name").and_then(Tag::as_str), Some("Renamed"));
        assert_eq!(items[0].get("acceptTextures"), Some(&Tag::Byte(1)), "unknown fields survive");
        assert_eq!(items[0].get("icon").and_then(Tag::as_str), Some("QUJD"));
        assert!(save_servers(&c, "inst-1", vec![Server { name: "x".into(), ip: " ".into(), icon: None }]).await.is_err());
    }

    #[tokio::test]
    async fn logs_screenshots_and_option_sync() {
        let c = ctx();
        let g = game(&c);
        std::fs::create_dir_all(g.join("logs")).unwrap();
        std::fs::write(g.join("logs/latest.log"), "hello log").unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(b"old log").unwrap();
        std::fs::write(g.join("logs/2024-01-01-1.log.gz"), gz.finish().unwrap()).unwrap();
        assert_eq!(list_logs(&c, "inst-1").await.unwrap().len(), 2);
        assert_eq!(read_log(&c, "inst-1", "logs/latest.log").await.unwrap(), "hello log");
        assert_eq!(read_log(&c, "inst-1", "logs/2024-01-01-1.log.gz").await.unwrap(), "old log");
        assert!(read_log(&c, "inst-1", "../accounts.json").await.is_err());
        assert!(read_log(&c, "inst-1", "mods/x.log").await.is_err());

        std::fs::create_dir_all(g.join("screenshots")).unwrap();
        std::fs::write(g.join("screenshots/a.png"), b"x").unwrap();
        std::fs::write(g.join("screenshots/notes.txt"), b"x").unwrap();
        assert_eq!(list_screenshots(&c, "inst-1").await.unwrap().len(), 1);
        delete_screenshot(&c, "inst-1", "a.png").await.unwrap();
        assert!(list_screenshots(&c, "inst-1").await.unwrap().is_empty());

        let other = c.game_dir("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("options.txt"), "key_key.jump:key.keyboard.space\n").unwrap();
        std::fs::write(other.join("servers.dat"), b"srv").unwrap();
        std::fs::write(other.join("secret.txt"), b"no").unwrap();
        let copied = sync_options(&c, "inst-1", "other", &["options.txt".into(), "secret.txt".into(), "optionsof.txt".into()]).await.unwrap();
        assert_eq!(copied, ["options.txt"], "only whitelisted files that exist are copied");
        assert_eq!(std::fs::read_to_string(g.join("options.txt")).unwrap(), "key_key.jump:key.keyboard.space\n");
        assert!(!g.join("secret.txt").exists() && !g.join("servers.dat").exists());
        assert!(sync_options(&c, "inst-1", "inst-1", &[]).await.is_err());
        assert!(sync_options(&c, "inst-1", "../x", &[]).await.is_err());
    }
}
