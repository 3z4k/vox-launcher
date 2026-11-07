//! Import instances from other launchers: Prism / MultiMC, the official Minecraft launcher and the CurseForge app.
//!
//! Nothing in the other launcher is changed. Files are copied into a new Vox instance, symlinks are skipped, and
//! account files (such as the official launcher's saved sign-ins) are never copied.

use crate::instances::{create_instance, update_instance};
use crate::mojang::Report;
use crate::types::{InstanceConfig, Loader, NewInstance};
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Where other launchers keep things. Tests point these at temporary folders.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    /// %APPDATA% on Windows.
    pub appdata: Option<PathBuf>,
    pub home: Option<PathBuf>,
    /// The standard .minecraft folder.
    pub mc_dir: Option<PathBuf>,
}

impl Roots {
    pub fn from_env() -> Roots {
        Roots {
            appdata: std::env::var_os("APPDATA").map(PathBuf::from),
            home: std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from),
            mc_dir: std::env::var_os("VOX_MC_DIR").map(PathBuf::from).or_else(crate::content::minecraft_dir),
        }
    }
}

/// An instance (or profile) in another launcher that can be imported.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// prism | multimc | official | curseforge
    pub source: String,
    pub source_label: String,
    /// Unique and stable: the instance folder, or "<minecraft folder>#<profile id>" for the official launcher.
    pub key: String,
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    pub loader_version: String,
    pub mods: u32,
    pub worlds: u32,
    /// Why this cannot be imported (shown greyed out). None = fine.
    pub problem: Option<String>,

    #[serde(skip)]
    pub game_dir: PathBuf,
    /// Copy every file except known junk (instance folders), or only known Minecraft folders (the shared .minecraft).
    #[serde(skip)]
    pub copy_everything: bool,
    #[serde(skip)]
    pub memory_mb: Option<u32>,
    #[serde(skip)]
    pub min_memory_mb: Option<u32>,
    #[serde(skip)]
    pub jvm_args: String,
    #[serde(skip)]
    pub java_path: String,
    #[serde(skip)]
    pub width: Option<u32>,
    #[serde(skip)]
    pub height: Option<u32>,
    #[serde(skip)]
    pub notes: String,
    #[serde(skip)]
    pub played_ms: u64,
}

fn blank(source: &str, label: &str, key: String, name: String, game_dir: PathBuf, copy_everything: bool) -> Found {
    Found {
        source: source.into(),
        source_label: label.into(),
        key,
        name,
        mc_version: String::new(),
        loader: Loader::Vanilla,
        loader_version: String::new(),
        mods: count_files(&game_dir.join("mods"), &["jar", "disabled"]),
        worlds: count_dirs(&game_dir.join("saves")),
        problem: None,
        game_dir,
        copy_everything,
        memory_mb: None,
        min_memory_mb: None,
        jvm_args: String::new(),
        java_path: String::new(),
        width: None,
        height: None,
        notes: String::new(),
        played_ms: 0,
    }
}

fn count_files(dir: &Path, exts: &[&str]) -> u32 {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| exts.contains(&x))).count() as u32)
        .unwrap_or(0)
}

fn count_dirs(dir: &Path) -> u32 {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count() as u32).unwrap_or(0)
}

// ---- reading ------------------------------------------------------------------------------

fn read_json_file(p: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Prism / MultiMC `instance.cfg`: plain `key=value` lines.
fn read_cfg(p: &Path) -> std::collections::HashMap<String, String> {
    let text = std::fs::read_to_string(p).unwrap_or_default();
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

fn truthy(v: Option<&String>) -> bool {
    v.is_some_and(|s| s.eq_ignore_ascii_case("true"))
}

/// A Prism or MultiMC instance folder.
pub fn read_prism(dir: &Path, source: &str) -> Option<Found> {
    let cfg_path = dir.join("instance.cfg");
    let pack = read_json_file(&dir.join("mmc-pack.json"))?;
    if !cfg_path.is_file() {
        return None;
    }
    let cfg = read_cfg(&cfg_path);
    let game_dir = [".minecraft", "minecraft"].iter().map(|n| dir.join(n)).find(|p| p.is_dir()).unwrap_or_else(|| dir.join("minecraft"));
    let name = cfg.get("name").filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    let label = if source == "multimc" { "MultiMC" } else { "Prism Launcher" };
    let mut f = blank(source, label, dir.to_string_lossy().to_string(), name, game_dir, true);
    for c in pack["components"].as_array().cloned().unwrap_or_default() {
        let (uid, ver) = (c["uid"].as_str().unwrap_or_default(), c["version"].as_str().unwrap_or_default().to_string());
        match uid {
            "net.minecraft" => f.mc_version = ver,
            "net.fabricmc.fabric-loader" => (f.loader, f.loader_version) = (Loader::Fabric, ver),
            "org.quiltmc.quilt-loader" => (f.loader, f.loader_version) = (Loader::Quilt, ver),
            "net.neoforged" => (f.loader, f.loader_version) = (Loader::Neoforge, ver),
            "net.minecraftforge" => (f.loader, f.loader_version) = (Loader::Forge, ver),
            _ => {}
        }
    }
    if f.mc_version.is_empty() {
        f.problem = Some("This instance does not say which Minecraft version it uses.".into());
    }
    if truthy(cfg.get("OverrideMemory")) {
        f.memory_mb = cfg.get("MaxMemAlloc").and_then(|v| v.parse().ok());
        f.min_memory_mb = cfg.get("MinMemAlloc").and_then(|v| v.parse().ok());
    }
    if truthy(cfg.get("OverrideJavaArgs")) {
        f.jvm_args = cfg.get("JvmArgs").cloned().unwrap_or_default();
    }
    if truthy(cfg.get("OverrideJavaLocation")) {
        f.java_path = cfg.get("JavaPath").cloned().unwrap_or_default();
    }
    if truthy(cfg.get("OverrideWindow")) {
        f.width = cfg.get("MinecraftWinWidth").and_then(|v| v.parse().ok());
        f.height = cfg.get("MinecraftWinHeight").and_then(|v| v.parse().ok());
    }
    f.notes = cfg.get("notes").cloned().unwrap_or_default();
    f.played_ms = cfg.get("totalTimePlayed").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * 1000;
    Some(f)
}

/// A CurseForge app instance folder (it holds `minecraftinstance.json`).
pub fn read_curseforge(dir: &Path) -> Option<Found> {
    let j = read_json_file(&dir.join("minecraftinstance.json"))?;
    let name = j["name"].as_str().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    let mut f = blank("curseforge", "CurseForge", dir.to_string_lossy().to_string(), name, dir.to_path_buf(), true);
    f.mc_version = j["gameVersion"].as_str().or_else(|| j["baseModLoader"]["minecraftVersion"].as_str()).unwrap_or_default().to_string();
    if let Some(l) = j["baseModLoader"]["name"].as_str() {
        match loader_from_curseforge(l, &f.mc_version) {
            Some((loader, lv)) => (f.loader, f.loader_version) = (loader, lv),
            None => f.problem = Some(format!("The mod loader \"{l}\" is not supported.")),
        }
    }
    if f.mc_version.is_empty() {
        f.problem = Some("This instance does not say which Minecraft version it uses.".into());
    }
    f.memory_mb = j["allocatedMemory"].as_u64().filter(|m| *m >= 512).map(|m| m as u32);
    f.jvm_args = j["javaArgsOverride"].as_str().unwrap_or_default().to_string();
    Some(f)
}

/// "forge-47.2.0", "fabric-0.15.7-1.20.1", "neoforge-20.4.80-beta", "quilt-0.26.0-1.20.1"
fn loader_from_curseforge(name: &str, mc: &str) -> Option<(Loader, String)> {
    let (kind, rest) = name.split_once('-')?;
    let rest = rest.strip_suffix(&format!("-{mc}")).unwrap_or(rest);
    let loader = match kind {
        "forge" => Loader::Forge,
        "fabric" => Loader::Fabric,
        "quilt" => Loader::Quilt,
        "neoforge" => Loader::Neoforge,
        _ => return None,
    };
    (!rest.is_empty()).then(|| (loader, rest.to_string()))
}

/// The Minecraft version and loader behind an official-launcher version id such as
/// "1.20.1", "fabric-loader-0.15.7-1.20.1", "1.20.1-forge-47.2.0" or "neoforge-20.4.80-beta".
pub fn parse_version_id(id: &str) -> Option<(String, Loader, String)> {
    let mc_re = r"\d+\.\d+(?:\.\d+)?";
    let re = |p: &str| Regex::new(p).unwrap();
    if let Some(c) = re(&format!(r"^fabric-loader-(.+)-({mc_re})$")).captures(id) {
        return Some((c[2].to_string(), Loader::Fabric, c[1].to_string()));
    }
    if let Some(c) = re(&format!(r"^quilt-loader-(.+)-({mc_re})$")).captures(id) {
        return Some((c[2].to_string(), Loader::Quilt, c[1].to_string()));
    }
    if let Some(c) = re(&format!(r"^({mc_re})-forge-?(?:{mc_re}-)?(.+)$")).captures(id) {
        return Some((c[1].to_string(), Loader::Forge, c[2].to_string()));
    }
    if let Some(c) = re(r"^neoforge-(\d+)\.(\d+)\..+$").captures(id) {
        let (major, minor) = (&c[1], &c[2]);
        let mc = if minor == "0" { format!("1.{major}") } else { format!("1.{major}.{minor}") };
        return Some((mc, Loader::Neoforge, id["neoforge-".len()..].to_string()));
    }
    // plain releases, release candidates, pre-releases and snapshots
    if re(&format!(r"^{mc_re}(?:-(?:pre|rc)\d+)?$")).is_match(id) || re(r"^\d{2}w\d{2}[a-z]$").is_match(id) {
        return Some((id.to_string(), Loader::Vanilla, String::new()));
    }
    None
}

/// Every profile of the official launcher (they all share the same game folder unless a profile sets its own).
pub fn read_official(mc_dir: &Path, latest_release: Option<&str>, latest_snapshot: Option<&str>) -> Vec<Found> {
    let Some(j) = read_json_file(&mc_dir.join("launcher_profiles.json")) else { return vec![] };
    let mut out = Vec::new();
    let mut profiles: Vec<(String, Value)> = j["profiles"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    profiles.sort_by(|a, b| a.1["name"].as_str().unwrap_or_default().cmp(b.1["name"].as_str().unwrap_or_default()));
    for (pid, p) in profiles {
        let version_id = match p["lastVersionId"].as_str().unwrap_or("latest-release") {
            "latest-release" => latest_release.map(String::from),
            "latest-snapshot" => latest_snapshot.map(String::from),
            other => Some(other.to_string()),
        };
        let game_dir = p["gameDir"].as_str().filter(|d| !d.is_empty()).map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(|| mc_dir.to_path_buf());
        let name = p["name"].as_str().filter(|n| !n.is_empty()).map(String::from).or_else(|| version_id.clone()).unwrap_or_else(|| "Minecraft".into());
        let mut f = blank("official", "Minecraft Launcher", format!("{}#{pid}", mc_dir.to_string_lossy()), name, game_dir, false);
        match version_id.as_deref().and_then(parse_version_id) {
            Some((mc, loader, lv)) => (f.mc_version, f.loader, f.loader_version) = (mc, loader, lv),
            None => f.problem = Some(format!("The version \"{}\" cannot be imported (it may be modified, such as OptiFine or a custom build).", version_id.unwrap_or_else(|| "latest".into()))),
        }
        if let Some(args) = p["javaArgs"].as_str() {
            f.jvm_args = args.to_string();
        }
        if let Some(java) = p["javaDir"].as_str() {
            f.java_path = java.to_string();
        }
        if let (Some(w), Some(h)) = (p["resolution"]["width"].as_u64(), p["resolution"]["height"].as_u64()) {
            (f.width, f.height) = (Some(w as u32), Some(h as u32));
        }
        out.push(f);
    }
    out
}

fn prism_roots(r: &Roots) -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(a) = &r.appdata {
        bases.push(a.join("PrismLauncher"));
    }
    if let Some(h) = &r.home {
        bases.push(h.join(".local/share/PrismLauncher"));
        bases.push(h.join("Library/Application Support/PrismLauncher"));
    }
    for b in bases {
        // a custom instance folder can be set in prismlauncher.cfg
        if let Ok(text) = std::fs::read_to_string(b.join("prismlauncher.cfg")) {
            if let Some(d) = text.lines().find_map(|l| l.strip_prefix("InstanceDir=")) {
                let d = PathBuf::from(d.trim());
                dirs.push(("prism".into(), if d.is_absolute() { d } else { b.join(d) }));
            }
        }
        dirs.push(("prism".into(), b.join("instances")));
    }
    if let Some(a) = &r.appdata {
        dirs.push(("multimc".into(), a.join("MultiMC/instances")));
    }
    if let Some(h) = &r.home {
        dirs.push(("multimc".into(), h.join(".local/share/multimc/instances")));
    }
    dirs
}

fn curseforge_roots(r: &Roots) -> Vec<PathBuf> {
    let Some(h) = &r.home else { return vec![] };
    vec![h.join("curseforge/minecraft/Instances"), h.join("Documents/curseforge/minecraft/Instances")]
}

fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// Everything importable that can be found in the usual places.
pub fn detect(roots: &Roots, latest_release: Option<&str>, latest_snapshot: Option<&str>) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for (source, dir) in prism_roots(roots) {
        for inst in child_dirs(&dir) {
            if let Some(f) = read_prism(&inst, &source) {
                if !out.iter().any(|o| o.key == f.key) {
                    out.push(f);
                }
            }
        }
    }
    for dir in curseforge_roots(roots) {
        for inst in child_dirs(&dir) {
            if let Some(f) = read_curseforge(&inst) {
                out.push(f);
            }
        }
    }
    if let Some(mc) = &roots.mc_dir {
        out.extend(read_official(mc, latest_release, latest_snapshot));
    }
    out
}

/// Looks inside a folder the person picked: one instance, a launcher's instances folder, or a .minecraft folder.
pub fn scan_folder(path: &Path, latest_release: Option<&str>, latest_snapshot: Option<&str>) -> Vec<Found> {
    if let Some(f) = read_prism(path, "prism").or_else(|| read_curseforge(path)) {
        return vec![f];
    }
    let official = read_official(path, latest_release, latest_snapshot);
    if !official.is_empty() {
        return official;
    }
    child_dirs(path).iter().filter_map(|c| read_prism(c, "prism").or_else(|| read_curseforge(c))).collect()
}

/// Finds one importable item again from the key the window sent back.
pub fn find_by_key(key: &str, latest_release: Option<&str>, latest_snapshot: Option<&str>) -> Result<Found> {
    let (base, profile) = match key.split_once('#') {
        Some((b, p)) => (b, Some(p)),
        None => (key, None),
    };
    let base = PathBuf::from(base);
    if !base.is_dir() {
        bail!("That instance folder no longer exists.");
    }
    let all = scan_folder(&base, latest_release, latest_snapshot);
    all.into_iter().find(|f| f.key == key || (profile.is_none() && Path::new(&f.key) == base)).ok_or_else(|| anyhow!("Could not read that instance."))
}

// ---- copying ------------------------------------------------------------------------------

/// Folders and files copied from the shared official .minecraft folder.
const KNOWN_DIRS: &[&str] = &["mods", "config", "resourcepacks", "shaderpacks", "datapacks", "screenshots", "defaultconfigs", "kubejs", "scripts", "schematics", "journeymap"];
const KNOWN_FILES: &[&str] = &["options.txt", "optionsof.txt", "optionsshaders.txt", "servers.dat"];
/// Never copied from an instance folder: logs, game files Vox downloads itself, and anything with sign-in data.
const SKIP: &[&str] = &[
    "logs", "crash-reports", "natives", "versions", "libraries", "assets", "runtime", "cache", "webcache", ".fabric", ".mixin.out",
    "launcher_profiles.json", "launcher_accounts.json", "launcher_msa_credentials.bin", "launcher_accounts_microsoft_store.json", "usercache.json",
    ".vox-content.json", ".vox-ready.json", "minecraftinstance.json", "instance.cfg", "mmc-pack.json", "patches",
];

fn skipped(name: &str) -> bool {
    SKIP.iter().any(|s| name.eq_ignore_ascii_case(s)) || name.ends_with(".log") || name.starts_with("launcher_")
}

/// What to copy: (source, relative destination) pairs, regular files only.
fn plan_copy(f: &Found, include_worlds: bool) -> Vec<(PathBuf, PathBuf)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            let p = e.path();
            if t.is_dir() {
                walk(base, &p, out);
            } else if t.is_file() {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push((p.clone(), rel.to_path_buf()));
                }
            }
        }
    }
    let mut all = Vec::new();
    let Ok(rd) = std::fs::read_dir(&f.game_dir) else { return all };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue;
        }
        let p = e.path();
        if t.is_dir() {
            let wanted = if name == "saves" { include_worlds } else if f.copy_everything { !skipped(&name) } else { KNOWN_DIRS.contains(&name.as_str()) };
            if wanted {
                let mut files = Vec::new();
                walk(&f.game_dir, &p, &mut files);
                all.extend(files);
            }
        } else if t.is_file() {
            let wanted = if f.copy_everything { !skipped(&name) } else { KNOWN_FILES.contains(&name.as_str()) };
            if wanted {
                all.push((p, PathBuf::from(&name)));
            }
        }
    }
    all
}

/// Creates a Vox instance from `f` and copies its files. Returns the new instance.
pub async fn import(ctx: &crate::util::Ctx, f: &Found, include_worlds: bool, report: &Report) -> Result<InstanceConfig> {
    if let Some(p) = &f.problem {
        bail!("{p}");
    }
    if f.mc_version.is_empty() {
        bail!("This instance does not say which Minecraft version it uses.");
    }
    if f.loader != Loader::Vanilla && f.loader_version.is_empty() {
        bail!("This instance does not say which {} version it uses.", f.loader.as_str());
    }
    report(&format!("Importing {}", f.name), 0, 1);
    let inst = create_instance(
        ctx,
        NewInstance { name: f.name.clone(), mc_version: f.mc_version.clone(), loader: f.loader, loader_version: f.loader_version.clone(), group: format!("Imported from {}", f.source_label), icon: String::new() },
    )
    .await?;

    let files = plan_copy(f, include_worlds);
    let dest_root = ctx.game_dir(&inst.id);
    let total = files.len().max(1) as u64;
    let copy_result = tokio::task::spawn_blocking({
        let report = report.clone();
        let name = f.name.clone();
        move || -> Result<()> {
            for (i, (src, rel)) in files.iter().enumerate() {
                // relative paths come from read_dir, but never let one climb out of the instance folder
                if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                    continue;
                }
                let dest = dest_root.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(src, &dest).map_err(|e| anyhow!("Could not copy {}: {e}", rel.display()))?;
                if i % 20 == 0 {
                    report(&format!("Copying {name}"), i as u64, total);
                }
            }
            Ok(())
        }
    })
    .await?;
    if let Err(e) = copy_result {
        let _ = crate::instances::delete_instance(ctx, &inst.id).await; // do not leave a half-copied instance behind
        return Err(e);
    }
    report(&format!("Copying {}", f.name), total, total);

    let mut patch = json!({ "notes": f.notes, "playedMs": f.played_ms });
    if let Some(m) = f.memory_mb {
        patch["memoryMb"] = json!(m);
    }
    if let Some(m) = f.min_memory_mb {
        patch["minMemoryMb"] = json!(m);
    }
    if !f.jvm_args.trim().is_empty() {
        patch["jvmArgs"] = json!(f.jvm_args);
    }
    if !f.java_path.is_empty() && Path::new(&f.java_path).is_file() {
        patch["javaPath"] = json!(f.java_path);
    }
    if let (Some(w), Some(h)) = (f.width, f.height) {
        patch["width"] = json!(w);
        patch["height"] = json!(h);
    }
    update_instance(ctx, &inst.id, patch).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{now_ms, Ctx};
    use std::sync::Arc;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vox-import-{tag}-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn official_version_ids() {
        assert_eq!(parse_version_id("1.20.1"), Some(("1.20.1".into(), Loader::Vanilla, "".into())));
        assert_eq!(parse_version_id("1.21"), Some(("1.21".into(), Loader::Vanilla, "".into())));
        assert_eq!(parse_version_id("24w14a"), Some(("24w14a".into(), Loader::Vanilla, "".into())));
        assert_eq!(parse_version_id("1.21-rc1"), Some(("1.21-rc1".into(), Loader::Vanilla, "".into())));
        assert_eq!(parse_version_id("fabric-loader-0.15.7-1.20.1"), Some(("1.20.1".into(), Loader::Fabric, "0.15.7".into())));
        assert_eq!(parse_version_id("quilt-loader-0.26.0-1.20.1"), Some(("1.20.1".into(), Loader::Quilt, "0.26.0".into())));
        assert_eq!(parse_version_id("1.20.1-forge-47.2.0"), Some(("1.20.1".into(), Loader::Forge, "47.2.0".into())));
        assert_eq!(parse_version_id("neoforge-20.4.80-beta"), Some(("1.20.4".into(), Loader::Neoforge, "20.4.80-beta".into())));
        assert_eq!(parse_version_id("neoforge-21.0.5"), Some(("1.21".into(), Loader::Neoforge, "21.0.5".into())));
        assert_eq!(parse_version_id("1.12.2-OptiFine_HD_U_G5"), None);
        assert_eq!(parse_version_id("latest-release"), None);
    }

    #[test]
    fn curseforge_loader_names() {
        assert_eq!(loader_from_curseforge("forge-47.2.0", "1.20.1"), Some((Loader::Forge, "47.2.0".into())));
        assert_eq!(loader_from_curseforge("fabric-0.15.7-1.20.1", "1.20.1"), Some((Loader::Fabric, "0.15.7".into())));
        assert_eq!(loader_from_curseforge("weird-1", "1.20.1"), None);
    }

    #[tokio::test]
    async fn finds_and_imports_from_every_launcher() {
        let base = tmp("all");
        let roots = Roots { appdata: Some(base.join("appdata")), home: Some(base.join("home")), mc_dir: Some(base.join("appdata/.minecraft")) };

        // Prism
        let p = base.join("appdata/PrismLauncher/instances/Cool Pack");
        write(&p.join("instance.cfg"), "name=Cool Pack\nOverrideMemory=true\nMaxMemAlloc=6144\nMinMemAlloc=1024\nOverrideJavaArgs=true\nJvmArgs=-XX:+UseZGC\ntotalTimePlayed=3600\nnotes=hello\n");
        write(&p.join("mmc-pack.json"), r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.7"}]}"#);
        write(&p.join(".minecraft/mods/a.jar"), "jar");
        write(&p.join(".minecraft/config/a.toml"), "x=1");
        write(&p.join(".minecraft/saves/World1/level.dat"), "lvl");
        write(&p.join(".minecraft/logs/latest.log"), "log");

        // CurseForge
        let c = base.join("home/curseforge/minecraft/Instances/All The Mods");
        write(&c.join("minecraftinstance.json"), r#"{"name":"All The Mods","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0"},"allocatedMemory":8192}"#);
        write(&c.join("mods/b.jar"), "jar");

        // Official launcher
        let mc = base.join("appdata/.minecraft");
        write(&mc.join("launcher_profiles.json"), r#"{"profiles":{"p1":{"name":"Vanilla Latest","lastVersionId":"latest-release","type":"latest-release"},"p2":{"name":"Weird","lastVersionId":"1.12.2-OptiFine_HD"}}}"#);
        write(&mc.join("launcher_accounts.json"), "SECRET");
        write(&mc.join("mods/c.jar"), "jar");
        write(&mc.join("options.txt"), "fov:70");
        write(&mc.join("saves/Survival/level.dat"), "lvl");
        write(&mc.join("libraries/junk.jar"), "junk");

        let found = detect(&roots, Some("1.21.4"), Some("24w46a"));
        let by = |n: &str| found.iter().find(|f| f.name == n).unwrap_or_else(|| panic!("missing {n}: {:?}", found.iter().map(|f| &f.name).collect::<Vec<_>>()));
        let prism = by("Cool Pack");
        assert_eq!((prism.mc_version.as_str(), prism.loader, prism.loader_version.as_str(), prism.mods, prism.worlds), ("1.20.1", Loader::Fabric, "0.15.7", 1, 1));
        let cf = by("All The Mods");
        assert_eq!((cf.loader, cf.loader_version.as_str(), cf.memory_mb), (Loader::Forge, "47.2.0", Some(8192)));
        assert_eq!(by("Vanilla Latest").mc_version, "1.21.4");
        assert!(by("Weird").problem.is_some());

        let ctx = Ctx::new(base.join("vox"));
        let report: Report = Arc::new(|_, _, _| {});
        let imported = import(&ctx, prism, false, &report).await.unwrap();
        let game = ctx.game_dir(&imported.id);
        assert!(game.join("mods/a.jar").is_file() && game.join("config/a.toml").is_file());
        assert!(!game.join("saves").exists(), "worlds are only copied when asked");
        assert!(!game.join("logs").exists());
        assert_eq!((imported.memory_mb, imported.jvm_args.as_str(), imported.played_ms, imported.notes.as_str()), (Some(6144), "-XX:+UseZGC", 3_600_000, "hello"));

        let with_worlds = import(&ctx, prism, true, &report).await.unwrap();
        assert!(ctx.game_dir(&with_worlds.id).join("saves/World1/level.dat").is_file());

        // the shared .minecraft: only known folders, never account files
        let vanilla = import(&ctx, by("Vanilla Latest"), true, &report).await.unwrap();
        let vg = ctx.game_dir(&vanilla.id);
        assert!(vg.join("mods/c.jar").is_file() && vg.join("options.txt").is_file() && vg.join("saves/Survival/level.dat").is_file());
        assert!(!vg.join("launcher_accounts.json").exists() && !vg.join("libraries").exists());

        assert!(import(&ctx, by("Weird"), false, &report).await.is_err());

        // keys round-trip, so the window never has to send a path we did not produce
        let again = find_by_key(&prism.key, None, None).unwrap();
        assert_eq!(again.name, "Cool Pack");
        let official = find_by_key(&by("Vanilla Latest").key, Some("1.21.4"), None).unwrap();
        assert_eq!(official.mc_version, "1.21.4");
        assert!(find_by_key("/definitely/not/here", None, None).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
