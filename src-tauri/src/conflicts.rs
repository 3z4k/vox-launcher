//! Looks inside an instance's mods before launch and warns about the problems that usually crash
//! the game: two copies of the same mod, mods built for another loader or Minecraft version, and
//! required mods that are missing. Everything is read from the jars, so it works offline.

use crate::types::{InstanceConfig, Loader};
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// "error" will very likely stop the game starting; "warn" might.
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub files: Vec<String>,
}

#[derive(Default, Debug)]
struct ModInfo {
    file: String,
    id: String,
    name: String,
    provides: Vec<String>,
    fabric: bool,
    quilt: bool,
    forge: bool,
    neoforge: bool,
    /// Version requirements on Minecraft (any one matching is enough).
    mc: Vec<Req>,
    /// Required mods that are not Minecraft / the loader.
    needs: Vec<String>,
}

#[derive(Debug, Clone)]
enum Req {
    Fabric(String),
    Maven(String),
}

// ---- version matching ------------------------------------------------------------------

fn parse_ver(s: &str) -> Option<Vec<u64>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

fn cmp(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

fn fabric_term(term: &str, mc: &[u64]) -> bool {
    let term = term.trim();
    if term.is_empty() || term == "*" {
        return true;
    }
    for op in [">=", "<=", ">", "<", "=", "~", "^"] {
        if let Some(v) = term.strip_prefix(op) {
            let Some(base) = parse_ver(v.trim_end_matches(".x").trim_end_matches(".*")) else { return true };
            return match op {
                ">=" => cmp(mc, &base) != std::cmp::Ordering::Less,
                "<=" => cmp(mc, &base) != std::cmp::Ordering::Greater,
                ">" => cmp(mc, &base) == std::cmp::Ordering::Greater,
                "<" => cmp(mc, &base) == std::cmp::Ordering::Less,
                "=" => cmp(mc, &base) == std::cmp::Ordering::Equal,
                "~" => {
                    let mut hi = base.clone();
                    hi.truncate(2);
                    while hi.len() < 2 {
                        hi.push(0);
                    }
                    hi[1] += 1;
                    cmp(mc, &base) != std::cmp::Ordering::Less && cmp(mc, &hi) == std::cmp::Ordering::Less
                }
                _ => {
                    let hi = vec![base[0] + 1];
                    cmp(mc, &base) != std::cmp::Ordering::Less && cmp(mc, &hi) == std::cmp::Ordering::Less
                }
            };
        }
    }
    if term.ends_with(".x") || term.ends_with(".*") {
        let Some(prefix) = parse_ver(&term[..term.len() - 2]) else { return true };
        return mc.len() >= prefix.len() && mc[..prefix.len()] == prefix[..];
    }
    match parse_ver(term) {
        Some(v) => cmp(mc, &v) == std::cmp::Ordering::Equal,
        None => true,
    }
}

fn fabric_matches(req: &str, mc: &[u64]) -> bool {
    req.split_whitespace().all(|t| fabric_term(t, mc))
}

fn maven_matches(range: &str, mc: &[u64]) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    let re = regex::Regex::new(r"[\[\(][^\]\)]*[\]\)]").unwrap();
    let sets: Vec<&str> = re.find_iter(range).map(|m| m.as_str()).collect();
    if sets.is_empty() {
        // A bare version means "this or newer".
        return parse_ver(range).map(|v| cmp(mc, &v) != std::cmp::Ordering::Less).unwrap_or(true);
    }
    sets.iter().any(|s| {
        let (lo_inc, hi_inc) = (s.starts_with('['), s.ends_with(']'));
        let inner = &s[1..s.len() - 1];
        match inner.split_once(',') {
            None => parse_ver(inner).map(|v| cmp(mc, &v) == std::cmp::Ordering::Equal).unwrap_or(true),
            Some((lo, hi)) => {
                let lo_ok = parse_ver(lo).map(|v| match cmp(mc, &v) {
                    std::cmp::Ordering::Less => false,
                    std::cmp::Ordering::Equal => lo_inc,
                    _ => true,
                }).unwrap_or(true);
                let hi_ok = parse_ver(hi).map(|v| match cmp(mc, &v) {
                    std::cmp::Ordering::Greater => false,
                    std::cmp::Ordering::Equal => hi_inc,
                    _ => true,
                }).unwrap_or(true);
                lo_ok && hi_ok
            }
        }
    })
}

fn req_ok(reqs: &[Req], mc: &[u64]) -> bool {
    reqs.is_empty() || reqs.iter().any(|r| match r {
        Req::Fabric(s) => fabric_matches(s, mc),
        Req::Maven(s) => maven_matches(s, mc),
    })
}

// ---- reading jars -----------------------------------------------------------------------

fn read_entry<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.take(512 * 1024).read_to_string(&mut s).ok()?;
    Some(s)
}

const BUILTIN: &[&str] = &["minecraft", "java", "fabricloader", "fabric-loader", "quilt_loader", "quilt_base", "forge", "neoforge", "javafml", "lowcodefml", "mixin", "mixinextras"];

fn strings_of(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => vec![],
    }
}

fn read_zip<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, file: &str, nested: bool) -> Option<ModInfo> {
    let mut info = ModInfo { file: file.to_string(), ..Default::default() };
    if let Some(s) = read_entry(zip, "fabric.mod.json") {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            info.fabric = true;
            info.id = v["id"].as_str().unwrap_or_default().to_string();
            info.name = v["name"].as_str().unwrap_or(&info.id).to_string();
            info.provides = strings_of(&v["provides"]);
            if let Some(d) = v["depends"].as_object() {
                for (k, val) in d {
                    if k == "minecraft" {
                        info.mc.extend(strings_of(val).into_iter().map(Req::Fabric));
                    } else if !BUILTIN.contains(&k.as_str()) {
                        info.needs.push(k.clone());
                    }
                }
            }
        }
    }
    if let Some(s) = read_entry(zip, "quilt.mod.json") {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            info.quilt = true;
            let l = &v["quilt_loader"];
            if info.id.is_empty() {
                info.id = l["id"].as_str().unwrap_or_default().to_string();
                info.name = l["metadata"]["name"].as_str().unwrap_or(&info.id).to_string();
            }
            if let Some(a) = l["depends"].as_array() {
                for d in a {
                    let (id, ver) = match d {
                        Value::String(s) => (s.as_str(), Value::Null),
                        _ => (d["id"].as_str().unwrap_or_default(), d["versions"].clone()),
                    };
                    if id == "minecraft" {
                        info.mc.extend(strings_of(&ver).into_iter().map(Req::Fabric));
                    } else if !id.is_empty() && !BUILTIN.contains(&id) && d["optional"].as_bool() != Some(true) {
                        info.needs.push(id.to_string());
                    }
                }
            }
        }
    }
    for (path, neo) in [("META-INF/neoforge.mods.toml", true), ("META-INF/mods.toml", false)] {
        let Some(s) = read_entry(zip, path) else { continue };
        if neo {
            info.neoforge = true;
        } else {
            info.forge = true;
        }
        let grab = |text: &str, key: &str| -> Option<String> {
            regex::Regex::new(&format!(r#"(?m)^\s*{key}\s*=\s*"([^"]*)""#)).ok()?.captures(text).map(|c| c[1].to_string())
        };
        let head = s.split("[[dependencies.").next().unwrap_or(&s).to_string();
        if info.id.is_empty() {
            info.id = grab(&head, "modId").unwrap_or_default();
            info.name = grab(&head, "displayName").unwrap_or_else(|| info.id.clone());
        }
        for block in s.split("[[dependencies.").skip(1) {
            let dep = grab(block, "modId").unwrap_or_default();
            let range = grab(block, "versionRange").unwrap_or_default();
            let required = block.contains("mandatory=true") || block.contains("mandatory = true") || block.contains("type=\"required\"") || block.contains("type = \"required\"");
            if dep == "minecraft" {
                if !range.is_empty() {
                    info.mc.push(Req::Maven(range));
                }
            } else if required && !dep.is_empty() && !BUILTIN.contains(&dep.as_str()) {
                info.needs.push(dep);
            }
        }
    }
    if info.id.is_empty() {
        return None;
    }
    if !nested {
        // Mods bundled inside this one count as installed (this is how Fabric API ships its modules).
        let names: Vec<String> = zip.file_names().filter(|n| n.starts_with("META-INF/jars/") && n.ends_with(".jar")).map(String::from).collect();
        for n in names.into_iter().take(80) {
            let mut buf = Vec::new();
            if let Ok(f) = zip.by_name(&n) {
                if f.size() > 40 * 1024 * 1024 || f.take(40 * 1024 * 1024).read_to_end(&mut buf).is_err() {
                    continue;
                }
            }
            if let Ok(mut inner) = zip::ZipArchive::new(std::io::Cursor::new(buf)) {
                if let Some(m) = read_zip(&mut inner, &n, true) {
                    info.provides.push(m.id);
                    info.provides.extend(m.provides);
                }
            }
        }
    }
    Some(info)
}

fn read_jar(path: &Path, file: &str) -> Option<ModInfo> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    read_zip(&mut zip, file, false)
}

// ---- the check ------------------------------------------------------------------------

pub fn check_dir(mods: &Path, inst: &InstanceConfig) -> Vec<Conflict> {
    let mut infos: Vec<ModInfo> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".jar") && e.path().is_file() {
                if let Some(i) = read_jar(&e.path(), &name) {
                    infos.push(i);
                }
            }
        }
    }
    infos.sort_by(|a, b| a.file.cmp(&b.file));
    let mut out = Vec::new();
    if infos.is_empty() {
        return out;
    }

    if inst.loader == Loader::Vanilla {
        out.push(Conflict {
            severity: "warn".into(),
            title: "This instance has no mod loader".into(),
            detail: "The mods in this instance won't do anything. Add Fabric, Quilt, Forge or NeoForge in the instance settings.".into(),
            files: infos.iter().map(|i| i.file.clone()).collect(),
        });
        return out;
    }

    // Two copies of the same mod.
    let mut by_id: BTreeMap<&str, Vec<&ModInfo>> = BTreeMap::new();
    for i in &infos {
        by_id.entry(i.id.as_str()).or_default().push(i);
    }
    for (id, list) in &by_id {
        if list.len() > 1 {
            out.push(Conflict {
                severity: "error".into(),
                title: format!("{} is installed twice", list[0].name),
                detail: format!("These files are the same mod ({id}). The game will refuse to start with both. Remove or disable one."),
                files: list.iter().map(|i| i.file.clone()).collect(),
            });
        }
    }

    // Built for a different loader.
    for i in &infos {
        let ok = match inst.loader {
            Loader::Fabric => i.fabric,
            Loader::Quilt => i.quilt || i.fabric,
            Loader::Forge => i.forge,
            Loader::Neoforge => i.neoforge || i.forge,
            Loader::Vanilla => true,
        };
        if !ok {
            let made = if i.fabric || i.quilt { "Fabric/Quilt" } else if i.neoforge { "NeoForge" } else { "Forge" };
            out.push(Conflict {
                severity: "error".into(),
                title: format!("{} is made for {made}", i.name),
                detail: format!("This instance uses {}. The mod won't load and may stop the game from starting.", match inst.loader { Loader::Fabric => "Fabric", Loader::Quilt => "Quilt", Loader::Forge => "Forge", Loader::Neoforge => "NeoForge", Loader::Vanilla => "no loader" }),
                files: vec![i.file.clone()],
            });
        }
    }

    // Built for a different Minecraft version.
    if let Some(mc) = parse_ver(&inst.mc_version) {
        for i in &infos {
            if !req_ok(&i.mc, &mc) {
                out.push(Conflict {
                    severity: "warn".into(),
                    title: format!("{} may not support Minecraft {}", i.name, inst.mc_version),
                    detail: "The mod says it needs a different Minecraft version. It may crash the game or be ignored. Look for a version made for this Minecraft.".into(),
                    files: vec![i.file.clone()],
                });
            }
        }
    }

    // Required mods that are missing.
    let mut present: BTreeSet<&str> = BTreeSet::new();
    for i in &infos {
        present.insert(i.id.as_str());
        present.extend(i.provides.iter().map(|s| s.as_str()));
    }
    let has_fabric_api = present.contains("fabric-api") || present.contains("fabric");
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for i in &infos {
        for n in &i.needs {
            let is_fabric_module = n.starts_with("fabric-") || n == "fabric";
            if present.contains(n.as_str()) || (is_fabric_module && has_fabric_api) || (n == "fabric" && has_fabric_api) {
                continue;
            }
            missing.entry(n.clone()).or_default().push(i.file.clone());
        }
    }
    for (dep, files) in missing {
        let who = files.len();
        out.push(Conflict {
            severity: "error".into(),
            title: if dep == "fabric-api" || dep == "fabric" { "Fabric API is missing".into() } else { format!("A required mod is missing: {dep}") },
            detail: format!("{who} mod{} need{} it to run. Install it from the Discover page.", if who == 1 { "" } else { "s" }, if who == 1 { "s" } else { "" }),
            files,
        });
    }
    out.sort_by_key(|c| if c.severity == "error" { 0 } else { 1 });
    out
}

pub async fn check(ctx: &crate::util::Ctx, id: &str) -> Result<Vec<Conflict>> {
    let inst = crate::instances::get_instance(ctx, id).await?;
    let dir = ctx.game_dir(id).join("mods");
    Ok(tokio::task::spawn_blocking(move || check_dir(&dir, &inst)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(dir: &Path, name: &str, files: &[(&str, String)]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join(name)).unwrap());
        for (n, d) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    fn fabric(id: &str, mc: &str, deps: &str) -> (&'static str, String) {
        ("fabric.mod.json", format!(r#"{{"id":"{id}","name":"{id}","version":"1","depends":{{"minecraft":"{mc}"{deps}}}}}"#))
    }
    fn inst(loader: Loader, mc: &str) -> InstanceConfig {
        serde_json::from_value(serde_json::json!({"id":"a","name":"A","mcVersion":mc,"loader":loader.as_str(),"created":0})).unwrap()
    }

    #[test]
    fn version_ranges() {
        let v = |s: &str| parse_ver(s).unwrap();
        assert!(fabric_matches(">=1.20 <1.21", &v("1.20.4")));
        assert!(!fabric_matches(">=1.20 <1.21", &v("1.21.1")));
        assert!(fabric_matches("1.20.x", &v("1.20.6")));
        assert!(!fabric_matches("1.20.x", &v("1.21")));
        assert!(fabric_matches("~1.21.1", &v("1.21.4")));
        assert!(!fabric_matches("~1.21.1", &v("1.22")));
        assert!(fabric_matches("^1.20", &v("1.21.1")));
        assert!(fabric_matches("*", &v("1.0")));
        assert!(maven_matches("[1.20.1,1.21)", &v("1.20.4")));
        assert!(!maven_matches("[1.20.1,1.21)", &v("1.21")));
        assert!(maven_matches("[1.21.1]", &v("1.21.1")));
        assert!(!maven_matches("[1.21.1]", &v("1.21.4")));
        assert!(maven_matches("[1.20,)", &v("1.21.1")));
    }

    #[test]
    fn finds_the_usual_problems() {
        let d = std::env::temp_dir().join(format!("vox-conf-{}-{}", std::process::id(), crate::util::now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        jar(&d, "good.jar", &[fabric("good", ">=1.21", "")]);
        jar(&d, "dup-a.jar", &[fabric("dup", ">=1.21", "")]);
        jar(&d, "dup-b.jar", &[fabric("dup", ">=1.21", "")]);
        jar(&d, "old.jar", &[fabric("old", "1.19.x", "")]);
        jar(&d, "needs.jar", &[fabric("needs", ">=1.21", r#","fabric-api":"*","libx":"*""#)]);
        jar(&d, "forge-only.jar", &[("META-INF/mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"fo\"\ndisplayName=\"Forge Only\"\n".to_string())]);
        let c = check_dir(&d, &inst(Loader::Fabric, "1.21.1"));
        let titles: Vec<&str> = c.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("dup is installed twice")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("old may not support")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("Fabric API is missing")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("libx")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("Forge Only is made for Forge")), "{titles:?}");
        assert!(!titles.iter().any(|t| t.contains("good")), "{titles:?}");
        assert_eq!(c[0].severity, "error");

        // With Fabric API bundled in, its modules count as installed.
        let inner = {
            let mut buf = std::io::Cursor::new(Vec::new());
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(br#"{"id":"fabric-api-base","version":"1"}"#).unwrap();
            z.finish().unwrap();
            buf.into_inner()
        };
        let mut z = zip::ZipWriter::new(std::fs::File::create(d.join("fabric-api.jar")).unwrap());
        z.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(br#"{"id":"fabric-api","version":"1"}"#).unwrap();
        z.start_file("META-INF/jars/fabric-api-base.jar", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&inner).unwrap();
        z.finish().unwrap();
        let c = check_dir(&d, &inst(Loader::Fabric, "1.21.1"));
        assert!(!c.iter().any(|c| c.title.contains("Fabric API")), "{c:?}");

        let c = check_dir(&d, &inst(Loader::Vanilla, "1.21.1"));
        assert_eq!(c.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
