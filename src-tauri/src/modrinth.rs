//! Modrinth: search, versions, installing mods / packs with required dependencies, and update checks.

use crate::mojang::Report;
use crate::types::{InstanceConfig, Loader};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const DEFAULT_API: &str = "https://api.modrinth.com/v2";

pub fn api() -> String {
    std::env::var("VOX_MODRINTH_URL").unwrap_or_else(|_| DEFAULT_API.to_string())
}

/// Folders inside an instance's game directory that hold downloadable content.
pub const CONTENT_KINDS: &[&str] = &["mods", "resourcepacks", "shaderpacks"];

pub fn project_type_for(kind: &str) -> &'static str {
    match kind {
        "mods" => "mod",
        "resourcepacks" => "resourcepack",
        "shaderpacks" => "shader",
        "modpacks" => "modpack",
        _ => "mod",
    }
}

/// Loaders whose mods may run on `loader` (Quilt runs most Fabric mods).
pub fn compatible_loaders(loader: Loader) -> Vec<&'static str> {
    match loader {
        Loader::Fabric => vec!["fabric"],
        Loader::Quilt => vec!["quilt", "fabric"],
        Loader::Forge => vec!["forge"],
        Loader::Neoforge => vec!["neoforge"],
        Loader::Vanilla => vec![],
    }
}

// ---- data returned to the UI ---------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub project_type: String,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_hits: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct VFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub sha1: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VFile>,
    pub dependencies: Vec<Dependency>,
}

impl ModVersion {
    pub fn from_json(v: &Value) -> Option<ModVersion> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let strs = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let files = v["files"]
            .as_array()?
            .iter()
            .filter_map(|f| {
                Some(VFile {
                    url: f["url"].as_str()?.to_string(),
                    filename: f["filename"].as_str()?.to_string(),
                    primary: f["primary"].as_bool().unwrap_or(false),
                    size: f["size"].as_u64().unwrap_or(0),
                    sha1: f["hashes"]["sha1"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect();
        let dependencies = v["dependencies"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|d| Dependency {
                        project_id: d["project_id"].as_str().map(String::from),
                        version_id: d["version_id"].as_str().map(String::from),
                        kind: d["dependency_type"].as_str().unwrap_or("optional").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(ModVersion {
            id: s("id"),
            project_id: s("project_id"),
            name: s("name"),
            version_number: s("version_number"),
            version_type: s("version_type"),
            date_published: s("date_published"),
            game_versions: strs("game_versions"),
            loaders: strs("loaders"),
            files,
            dependencies,
        })
    }

    pub fn primary_file(&self) -> Option<&VFile> {
        self.files.iter().find(|f| f.primary).or_else(|| self.files.first())
    }
}

// ---- search & versions ---------------------------------------------------------------

/// `kind`: mods | resourcepacks | shaderpacks | modpacks. `loader` and `mc` narrow the results when given.
pub async fn search(ctx: &Ctx, query: &str, kind: &str, mc: &str, loader: Option<Loader>, index: &str, offset: u32) -> Result<SearchResult> {
    let mut facets: Vec<Value> = vec![json!([format!("project_type:{}", project_type_for(kind))])];
    if !mc.is_empty() {
        facets.push(json!([format!("versions:{mc}")]));
    }
    if kind == "mods" {
        if let Some(l) = loader {
            let cats: Vec<String> = compatible_loaders(l).iter().map(|c| format!("categories:{c}")).collect();
            if !cats.is_empty() {
                facets.push(json!(cats));
            }
        }
    }
    let index = if ["relevance", "downloads", "follows", "newest", "updated"].contains(&index) { index } else { "relevance" };
    let resp = ctx
        .http
        .get(format!("{}/search", api()))
        .query(&[
            ("query", query.to_string()),
            ("facets", Value::Array(facets).to_string()),
            ("limit", "24".to_string()),
            ("offset", offset.to_string()),
            ("index", index.to_string()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("Modrinth search failed (HTTP {}).", resp.status().as_u16());
    }
    let v: Value = resp.json().await?;
    let hits = v["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| SearchHit {
                    project_id: h["project_id"].as_str().unwrap_or_default().into(),
                    slug: h["slug"].as_str().unwrap_or_default().into(),
                    title: h["title"].as_str().unwrap_or_default().into(),
                    description: h["description"].as_str().unwrap_or_default().into(),
                    icon_url: h["icon_url"].as_str().filter(|s| !s.is_empty()).map(String::from),
                    author: h["author"].as_str().unwrap_or_default().into(),
                    downloads: h["downloads"].as_u64().unwrap_or(0),
                    follows: h["follows"].as_u64().unwrap_or(0),
                    project_type: h["project_type"].as_str().unwrap_or_default().into(),
                    categories: h["categories"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SearchResult { hits, total_hits: v["total_hits"].as_u64().unwrap_or(0) })
}

pub async fn project_versions(ctx: &Ctx, project_id: &str, kind: &str, mc: &str, loader: Option<Loader>) -> Result<Vec<ModVersion>> {
    let mut q: Vec<(&str, String)> = Vec::new();
    if kind == "mods" {
        if let Some(l) = loader {
            let ls = compatible_loaders(l);
            if !ls.is_empty() {
                q.push(("loaders", json!(ls).to_string()));
            }
        }
    }
    if !mc.is_empty() {
        q.push(("game_versions", json!([mc]).to_string()));
    }
    let resp = ctx.http.get(format!("{}/project/{}/version", api(), enc(project_id))).query(&q).send().await?;
    if !resp.status().is_success() {
        bail!("Could not load versions (HTTP {}).", resp.status().as_u16());
    }
    let v: Value = resp.json().await?;
    Ok(v.as_array().map(|a| a.iter().filter_map(ModVersion::from_json).collect()).unwrap_or_default())
}

pub async fn get_version(ctx: &Ctx, version_id: &str) -> Result<ModVersion> {
    let resp = ctx.http.get(format!("{}/version/{}", api(), enc(version_id))).send().await?;
    if !resp.status().is_success() {
        bail!("Could not load that version (HTTP {}).", resp.status().as_u16());
    }
    ModVersion::from_json(&resp.json().await?).ok_or_else(|| anyhow!("Unexpected reply from Modrinth."))
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---- per-instance metadata about installed files --------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaEntry {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

pub type Meta = BTreeMap<String, MetaEntry>; // key: "mods/file.jar"

pub fn meta_path(ctx: &Ctx, id: &str) -> PathBuf {
    ctx.game_dir(id).join(".vox-content.json")
}
pub async fn load_meta(ctx: &Ctx, id: &str) -> Meta {
    read_json_or(&meta_path(ctx, id), Meta::new()).await
}
pub async fn save_meta(ctx: &Ctx, id: &str, m: &Meta) -> Result<()> {
    write_json(&meta_path(ctx, id), m).await
}

/// A file name that is safe to create inside a content folder.
pub fn safe_filename(name: &str) -> Result<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    if base.is_empty() || base == "." || base == ".." || base.contains(':') {
        bail!("Unsafe file name \"{name}\".");
    }
    Ok(base.to_string())
}

fn check_kind(kind: &str) -> Result<()> {
    if CONTENT_KINDS.contains(&kind) {
        Ok(())
    } else {
        bail!("Unknown content type \"{kind}\".")
    }
}

// ---- installing ----------------------------------------------------------------------

/// Downloads one version's primary file into the instance and records it. Returns the saved file name.
pub async fn install_version(ctx: &Ctx, inst: &InstanceConfig, kind: &str, v: &ModVersion, title: &str, icon_url: Option<String>) -> Result<String> {
    check_kind(kind)?;
    let file = v.primary_file().ok_or_else(|| anyhow!("This version has no downloadable file."))?;
    let filename = safe_filename(&file.filename)?;
    let dest = ctx.game_dir(&inst.id).join(kind).join(&filename);
    let expect = Expect { sha1: (!file.sha1.is_empty()).then(|| file.sha1.clone()), size: (file.size > 0).then_some(file.size) };
    if !file_is_good(&dest, &expect, true).await {
        download(&ctx.http, &file.url, &dest, &expect).await?;
    }
    let mut meta = load_meta(ctx, &inst.id).await;
    // replace an older file of the same project
    let key = format!("{kind}/{filename}");
    let stale: Vec<String> = meta.iter().filter(|(k, e)| *k != &key && k.starts_with(&format!("{kind}/")) && e.project_id == v.project_id).map(|(k, _)| k.clone()).collect();
    for k in stale {
        let _ = tokio::fs::remove_file(ctx.game_dir(&inst.id).join(&k)).await;
        let _ = tokio::fs::remove_file(ctx.game_dir(&inst.id).join(format!("{k}.disabled"))).await;
        meta.remove(&k);
    }
    meta.insert(
        key,
        MetaEntry { project_id: v.project_id.clone(), version_id: v.id.clone(), title: title.to_string(), version_number: v.version_number.clone(), icon_url },
    );
    save_meta(ctx, &inst.id, &meta).await?;
    Ok(filename)
}

async fn project_title(ctx: &Ctx, project_id: &str) -> (String, Option<String>) {
    let Ok(r) = ctx.http.get(format!("{}/project/{}", api(), enc(project_id))).send().await else { return (project_id.to_string(), None) };
    let v: Value = r.json().await.unwrap_or(Value::Null);
    (v["title"].as_str().unwrap_or(project_id).to_string(), v["icon_url"].as_str().map(String::from))
}

/// Installs a version and, for mods, every *required* dependency that is not installed yet.
/// Returns the file names that were added.
pub async fn install_with_dependencies(
    ctx: &Ctx,
    inst: &InstanceConfig,
    kind: &str,
    v: &ModVersion,
    title: &str,
    icon_url: Option<String>,
    report: &Report,
) -> Result<Vec<String>> {
    check_kind(kind)?;
    let mut added = Vec::new();
    let mut queue: Vec<(ModVersion, String, Option<String>, u8)> = vec![(v.clone(), title.to_string(), icon_url, 0)];
    let mut seen: HashSet<String> = HashSet::new();
    let installed: HashSet<String> = load_meta(ctx, &inst.id).await.values().map(|e| e.project_id.clone()).collect();
    let loader = (inst.loader != Loader::Vanilla).then_some(inst.loader);
    while let Some((ver, name, icon, depth)) = queue.pop() {
        if !seen.insert(ver.project_id.clone()) {
            continue;
        }
        report(&format!("Installing {name}"), added.len() as u64, (added.len() + queue.len() + 1) as u64);
        added.push(install_version(ctx, inst, kind, &ver, &name, icon).await?);
        if kind != "mods" || depth >= 3 {
            continue;
        }
        for d in ver.dependencies.iter().filter(|d| d.kind == "required") {
            let Some(pid) = d.project_id.clone() else { continue };
            if installed.contains(&pid) || seen.contains(&pid) {
                continue;
            }
            let dep_ver = match &d.version_id {
                Some(vid) => get_version(ctx, vid).await.ok(),
                None => project_versions(ctx, &pid, kind, &inst.mc_version, loader).await.ok().and_then(|mut l| (!l.is_empty()).then(|| l.remove(0))),
            };
            if let Some(dv) = dep_ver {
                let (t, i) = project_title(ctx, &pid).await;
                queue.push((dv, t, i, depth + 1));
            }
        }
    }
    report("Done", added.len() as u64, added.len() as u64);
    Ok(added)
}

// ---- updates -------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub filename: String,
    pub project_id: String,
    pub title: String,
    pub current_version: String,
    pub new_version: ModVersion,
}

async fn post_json(ctx: &Ctx, path: &str, body: &Value) -> Result<Value> {
    let r = ctx.http.post(format!("{}{}", api(), path)).json(body).send().await?;
    if !r.status().is_success() {
        bail!("Modrinth returned HTTP {}.", r.status().as_u16());
    }
    Ok(r.json().await?)
}

/// Hashes every enabled file in `kind` (name -> sha1).
pub async fn hash_folder(ctx: &Ctx, id: &str, kind: &str) -> Result<Vec<(String, String)>> {
    let dir = ctx.game_dir(id).join(kind);
    let mut names = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let n = e.file_name().to_string_lossy().to_string();
            let is_content = n.ends_with(".jar") || n.ends_with(".zip");
            if is_content && e.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                names.push(n);
            }
        }
    }
    let mut out = Vec::new();
    for n in names {
        out.push((n.clone(), sha1_file(&dir.join(&n)).await?));
    }
    Ok(out)
}

/// Recognises files that were added by hand (or by another launcher) so they get a name and can be updated.
pub async fn identify_files(ctx: &Ctx, inst: &InstanceConfig, kind: &str) -> Result<()> {
    check_kind(kind)?;
    let hashes = hash_folder(ctx, &inst.id, kind).await?;
    let mut meta = load_meta(ctx, &inst.id).await;
    let unknown: Vec<&(String, String)> = hashes.iter().filter(|(n, _)| !meta.contains_key(&format!("{kind}/{n}"))).collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let found = post_json(ctx, "/version_files", &json!({ "hashes": unknown.iter().map(|(_, h)| h).collect::<Vec<_>>(), "algorithm": "sha1" })).await?;
    let mut project_ids: Vec<String> = Vec::new();
    for (_, h) in &unknown {
        if let Some(pid) = found[h.as_str()]["project_id"].as_str() {
            project_ids.push(pid.to_string());
        }
    }
    project_ids.sort();
    project_ids.dedup();
    let mut titles: HashMap<String, (String, Option<String>)> = HashMap::new();
    if !project_ids.is_empty() {
        let r = ctx.http.get(format!("{}/projects", api())).query(&[("ids", json!(project_ids).to_string())]).send().await?;
        let v: Value = r.json().await.unwrap_or(Value::Null);
        for p in v.as_array().cloned().unwrap_or_default() {
            titles.insert(p["id"].as_str().unwrap_or_default().to_string(), (p["title"].as_str().unwrap_or_default().to_string(), p["icon_url"].as_str().map(String::from)));
        }
    }
    for (name, h) in unknown {
        let ver = &found[h.as_str()];
        let Some(pid) = ver["project_id"].as_str() else { continue };
        let (title, icon) = titles.get(pid).cloned().unwrap_or((name.clone(), None));
        meta.insert(
            format!("{kind}/{name}"),
            MetaEntry { project_id: pid.into(), version_id: ver["id"].as_str().unwrap_or_default().into(), title, version_number: ver["version_number"].as_str().unwrap_or_default().into(), icon_url: icon },
        );
    }
    save_meta(ctx, &inst.id, &meta).await
}

/// Finds newer versions for every enabled file in `kind`.
pub async fn check_updates(ctx: &Ctx, inst: &InstanceConfig, kind: &str) -> Result<Vec<UpdateInfo>> {
    check_kind(kind)?;
    identify_files(ctx, inst, kind).await?;
    let hashes = hash_folder(ctx, &inst.id, kind).await?;
    if hashes.is_empty() {
        return Ok(vec![]);
    }
    let mut body = json!({ "hashes": hashes.iter().map(|(_, h)| h).collect::<Vec<_>>(), "algorithm": "sha1" });
    if kind == "mods" && inst.loader != Loader::Vanilla {
        body["loaders"] = json!(compatible_loaders(inst.loader));
    }
    body["game_versions"] = json!([inst.mc_version]);
    let latest = post_json(ctx, "/version_files/update", &body).await?;
    let meta = load_meta(ctx, &inst.id).await;
    let mut out = Vec::new();
    for (name, hash) in hashes {
        let Some(v) = latest.get(&hash).and_then(ModVersion::from_json) else { continue };
        let up_to_date = v.files.iter().any(|f| f.sha1.eq_ignore_ascii_case(&hash));
        if up_to_date {
            continue;
        }
        let m = meta.get(&format!("{kind}/{name}"));
        out.push(UpdateInfo {
            filename: name,
            project_id: v.project_id.clone(),
            title: m.map(|m| m.title.clone()).unwrap_or_default(),
            current_version: m.map(|m| m.version_number.clone()).unwrap_or_default(),
            new_version: v,
        });
    }
    Ok(out)
}

/// Installs `version_id` over the file `old_filename`.
pub async fn apply_update(ctx: &Ctx, inst: &InstanceConfig, kind: &str, old_filename: &str, version_id: &str) -> Result<String> {
    check_kind(kind)?;
    let old = safe_filename(old_filename)?;
    let v = get_version(ctx, version_id).await?;
    let meta = load_meta(ctx, &inst.id).await;
    let (title, icon) = meta
        .get(&format!("{kind}/{old}"))
        .map(|m| (m.title.clone(), m.icon_url.clone()))
        .unwrap_or_else(|| (old.clone(), None));
    let new_name = install_version(ctx, inst, kind, &v, &title, icon).await?;
    if new_name != old {
        let dir = ctx.game_dir(&inst.id).join(kind);
        let _ = tokio::fs::remove_file(dir.join(&old)).await;
        let mut meta = load_meta(ctx, &inst.id).await;
        meta.remove(&format!("{kind}/{old}"));
        save_meta(ctx, &inst.id, &meta).await?;
    }
    Ok(new_name)
}

pub fn content_dir(ctx: &Ctx, id: &str, kind: &str) -> Result<PathBuf> {
    check_kind(kind)?;
    Ok(ctx.game_dir(id).join(kind))
}

pub fn file_exists(p: &Path) -> bool {
    p.exists()
}
