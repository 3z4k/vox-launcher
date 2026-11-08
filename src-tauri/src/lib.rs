//! Vox Launcher engine. The UI talks to these commands through Tauri's IPC.

pub mod auth;
pub mod config;
pub mod content;
pub mod core;
pub mod discord;
pub mod instances;
pub mod java;
pub mod launch;
pub mod loaders;
pub mod modrinth;
pub mod mojang;
pub mod nbt;
pub mod packs;
pub mod paths;
pub mod skins;
pub mod crash;
pub mod importer;
pub mod shortcut;
pub mod snapshots;
pub mod conflicts;
pub mod update;
pub mod client_mod;
pub mod types;
pub mod util;

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use types::*;
use util::Ctx;

struct AppState {
    ctx: Ctx,
    /// Instance id from `--launch <id>` that the window has not started yet.
    pending_launch: Mutex<Option<String>>,
    running: Mutex<HashMap<String, launch::Running>>,
    preparing: Mutex<HashSet<String>>,
    discord: discord::Discord,
}

type S<'a> = State<'a, Arc<AppState>>;
type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    id: String,
    stage: String,
    current: u64,
    total: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskEvent {
    task_id: String,
    stage: String,
    current: u64,
    total: u64,
}
#[derive(Clone, Serialize)]
struct LogEvent {
    id: String,
    line: String,
}
#[derive(Clone, Serialize)]
struct StateEvent {
    id: String,
    running: bool,
    code: Option<i32>,
}

fn task_report(app: &AppHandle, task_id: String) -> mojang::Report {
    let app = app.clone();
    Arc::new(move |stage, current, total| {
        let _ = app.emit("task:progress", TaskEvent { task_id: task_id.clone(), stage: stage.into(), current, total });
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpeedEvent {
    task_id: String,
    bytes: u64,
    bps: u64,
    paused: bool,
}

/// Runs a task's work with pause/cancel support and reports its speed twice a second.
async fn scoped<T>(app: &AppHandle, task_id: &str, fut: impl std::future::Future<Output = T>) -> T {
    let ctl = util::ctl_for(task_id);
    let (app2, id, c2) = (app.clone(), task_id.to_string(), ctl.clone());
    tauri::async_runtime::spawn(async move {
        let mut last = 0u64;
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        while !c2.finished.load(std::sync::atomic::Ordering::Relaxed) {
            tick.tick().await;
            let now = c2.bytes.load(std::sync::atomic::Ordering::Relaxed);
            let paused = c2.paused.load(std::sync::atomic::Ordering::Relaxed);
            let _ = app2.emit("task:speed", SpeedEvent { task_id: id.clone(), bytes: now, bps: if paused { 0 } else { now.saturating_sub(last) * 2 }, paused });
            last = now;
        }
    });
    let out = util::CTL.scope(ctl, fut).await;
    util::ctl_drop(task_id);
    out
}

#[tauri::command]
fn task_pause(id: String, paused: bool) {
    if let Some(c) = util::ctl_get(&id) {
        c.paused.store(paused, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tauri::command]
fn task_cancel(id: String) {
    if let Some(c) = util::ctl_get(&id) {
        c.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        c.paused.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The GitHub repository to read releases from: the user's setting, else the one built into this launcher.
async fn repo_of(ctx: &Ctx) -> String {
    let set = instances::get_settings(ctx).await;
    if !set.update_repo.is_empty() {
        return set.update_repo;
    }
    config::get_build_config(ctx).await.update_repo
}

async fn refresh_discord(s: &AppState) {
    let set = instances::get_settings(&s.ctx).await;
    let app_id = config::get_build_config(&s.ctx).await.discord_app_id;
    s.discord.refresh(set.discord_enabled, &app_id, set.discord_show_time, set.discord_show_server);
}

// ---- versions & loaders ------------------------------------------------------------

#[tauri::command]
async fn list_versions(s: S<'_>) -> CmdResult<Vec<mojang::McVersion>> {
    mojang::list_versions(&s.ctx).await.map_err(err)
}

#[tauri::command]
async fn list_loader_versions(s: S<'_>, loader: Loader, mc_version: String) -> CmdResult<Vec<String>> {
    loaders::list_loader_versions(&s.ctx, loader, &mc_version).await.map_err(err)
}

// ---- instances ---------------------------------------------------------------------

#[tauri::command]
async fn list_instances(s: S<'_>) -> CmdResult<Vec<InstanceConfig>> {
    instances::list_instances(&s.ctx).await.map_err(err)
}

#[tauri::command]
async fn create_instance(s: S<'_>, input: NewInstance) -> CmdResult<InstanceConfig> {
    instances::create_instance(&s.ctx, input).await.map_err(err)
}

#[tauri::command]
async fn update_instance(s: S<'_>, id: String, patch: serde_json::Value) -> CmdResult<InstanceConfig> {
    instances::update_instance(&s.ctx, &id, patch).await.map_err(err)
}

#[tauri::command]
async fn delete_instance(s: S<'_>, id: String) -> CmdResult<()> {
    if s.running.lock().unwrap().contains_key(&id) || s.preparing.lock().unwrap().contains(&id) {
        return Err("Stop the instance before deleting it.".into());
    }
    instances::delete_instance(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn duplicate_instance(s: S<'_>, id: String) -> CmdResult<InstanceConfig> {
    instances::duplicate_instance(&s.ctx, &id).await.map_err(err)
}

const OPENABLE: &[&str] = &["", "mods", "saves", "resourcepacks", "shaderpacks", "screenshots", "logs", "config", "world-backups", "crash-reports"];

/// Opens the instance folder (or one of its well-known subfolders) in the file manager.
#[tauri::command]
async fn open_path(s: S<'_>, id: String, sub: String) -> CmdResult<()> {
    instances::assert_safe_id(&id).map_err(err)?;
    if !OPENABLE.contains(&sub.as_str()) {
        return Err("That folder cannot be opened.".into());
    }
    let dir = s.ctx.game_dir(&id).join(&sub);
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;
    open::that_detached(dir).map_err(err)
}

// ---- snapshots -----------------------------------------------------------------------

#[tauri::command]
async fn list_snapshots(s: S<'_>, id: String) -> CmdResult<Vec<snapshots::Snapshot>> {
    snapshots::list(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn create_snapshot(s: S<'_>, id: String, name: String) -> CmdResult<snapshots::Snapshot> {
    snapshots::create(&s.ctx, &id, &name).await.map_err(err)
}

#[tauri::command]
async fn restore_snapshot(s: S<'_>, id: String, snapshot: String) -> CmdResult<InstanceConfig> {
    if s.running.lock().unwrap().contains_key(&id) {
        return Err("Close the game before restoring a snapshot.".into());
    }
    snapshots::restore(&s.ctx, &id, &snapshot).await.map_err(err)
}

#[tauri::command]
async fn delete_snapshot(s: S<'_>, id: String, snapshot: String) -> CmdResult<()> {
    snapshots::delete(&s.ctx, &id, &snapshot).await.map_err(err)
}

#[tauri::command]
fn is_portable() -> bool {
    paths::is_portable()
}

#[tauri::command]
fn data_dir(s: S<'_>) -> String {
    s.ctx.root.to_string_lossy().to_string()
}

#[tauri::command]
async fn open_data_dir(s: S<'_>) -> CmdResult<()> {
    tokio::fs::create_dir_all(&*s.ctx.root).await.map_err(err)?;
    open::that_detached(&*s.ctx.root).map_err(err)
}

// ---- file dialogs -------------------------------------------------------------------

/// `kind`: import | skin | content | java | any. Returns None when cancelled.
#[tauri::command]
async fn pick_file(app: AppHandle, kind: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file();
        d = match kind.as_str() {
            "import" => d.add_filter("Modpack or instance export", &["mrpack", "zip"]),
            "skin" => d.add_filter("PNG skin", &["png"]),
            "content" => d.add_filter("Mods and packs", &["jar", "zip"]),
            _ => d,
        };
        d.blocking_pick_file().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().to_string()))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
async fn pick_save(app: AppHandle, default_name: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(default_name)
            .add_filter("Zip archive", &["zip"])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn import_file(app: AppHandle, s: S<'_>, path: String, task_id: String) -> CmdResult<InstanceConfig> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let threads = instances::get_settings(&s.ctx).await.download_threads as usize;
        packs::import_file(&s.ctx, &PathBuf::from(path), &task_report(&app, task_id), threads).await.map_err(err)
    }).await
}

#[tauri::command]
async fn export_instance(app: AppHandle, s: S<'_>, id: String, path: String, include_worlds: bool, task_id: String) -> CmdResult<()> {
    scoped(&app.clone(), &task_id.clone(), async move {
        packs::export_instance(&s.ctx, &id, &PathBuf::from(path), include_worlds, &task_report(&app, task_id)).await.map_err(err)
    }).await
}

/// Saves a theme (JSON text from the UI) to a file the player picks.
#[tauri::command]
async fn export_theme(app: AppHandle, json: String, default_name: String) -> CmdResult<bool> {
    if json.len() > 64 * 1024 {
        return Err("That theme is too large.".into());
    }
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_file_name(default_name).add_filter("Vox theme", &["voxtheme", "json"]).blocking_save_file().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(err)?;
    let Some(path) = path else { return Ok(false) };
    tokio::fs::write(path, json).await.map_err(err)?;
    Ok(true)
}

/// Lets the player pick a theme file and returns its text (the UI validates it).
#[tauri::command]
async fn import_theme(app: AppHandle) -> CmdResult<Option<String>> {
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("Vox theme", &["voxtheme", "json"]).blocking_pick_file().and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(err)?;
    let Some(path) = path else { return Ok(None) };
    let meta = tokio::fs::metadata(&path).await.map_err(err)?;
    if meta.len() > 64 * 1024 {
        return Err("That file is too large to be a theme.".into());
    }
    Ok(Some(tokio::fs::read_to_string(path).await.map_err(|_| "That file isn't a readable theme.".to_string())?))
}

// ---- import from other launchers ------------------------------------------------------

/// Newest release and newest snapshot, used for official-launcher profiles set to "latest".
async fn latest_versions(ctx: &Ctx) -> (Option<String>, Option<String>) {
    match mojang::get_manifest(ctx).await {
        Ok(m) => (m.versions.iter().find(|v| v.kind == "release").map(|v| v.id.clone()), m.versions.first().map(|v| v.id.clone())),
        Err(_) => (None, None),
    }
}

#[tauri::command]
async fn detect_imports(s: S<'_>) -> CmdResult<Vec<importer::Found>> {
    let (rel, snap) = latest_versions(&s.ctx).await;
    tokio::task::spawn_blocking(move || importer::detect(&importer::Roots::from_env(), rel.as_deref(), snap.as_deref())).await.map_err(err)
}

#[tauri::command]
async fn scan_import_folder(s: S<'_>, path: String) -> CmdResult<Vec<importer::Found>> {
    let (rel, snap) = latest_versions(&s.ctx).await;
    tokio::task::spawn_blocking(move || importer::scan_folder(&PathBuf::from(path), rel.as_deref(), snap.as_deref())).await.map_err(err)
}

#[tauri::command]
async fn import_instances(app: AppHandle, s: S<'_>, keys: Vec<String>, include_worlds: bool, task_id: String) -> CmdResult<Vec<InstanceConfig>> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let report = task_report(&app, task_id);
        let (rel, snap) = latest_versions(&s.ctx).await;
        let mut made = Vec::new();
        for key in keys {
            let (r, sn) = (rel.clone(), snap.clone());
            let found = tokio::task::spawn_blocking(move || importer::find_by_key(&key, r.as_deref(), sn.as_deref())).await.map_err(err)?.map_err(err)?;
            made.push(importer::import(&s.ctx, &found, include_worlds, &report).await.map_err(err)?);
        }
        Ok(made)
    }).await
}

// ---- accounts & skins --------------------------------------------------------------

/// Skin textures of the Microsoft accounts, keyed by account id, for drawing their faces.
#[tauri::command]
async fn account_skins(s: S<'_>) -> CmdResult<std::collections::HashMap<String, String>> {
    let ids = auth::online_accounts(&s.ctx).await;
    let found = futures_util::future::join_all(ids.iter().map(|id| skins::public_skin(&s.ctx, id))).await;
    Ok(ids.into_iter().zip(found).filter_map(|(id, d)| d.map(|d| (id, d))).collect())
}

#[tauri::command]
async fn list_accounts(s: S<'_>) -> CmdResult<Vec<AccountInfo>> {
    Ok(auth::list_accounts(&s.ctx).await)
}

#[tauri::command]
async fn login(app: AppHandle, s: S<'_>) -> CmdResult<AccountInfo> {
    auth::login(&s.ctx, move |code| {
        let _ = app.emit("auth:code", code);
    })
    .await
    .map_err(err)
}

#[tauri::command]
fn cancel_login() {
    auth::cancel_login();
}

#[tauri::command]
async fn add_offline_account(s: S<'_>, username: String) -> CmdResult<AccountInfo> {
    auth::add_offline_account(&s.ctx, &username).await.map_err(err)
}

#[tauri::command]
async fn remove_account(s: S<'_>, id: String) -> CmdResult<()> {
    auth::remove_account(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn set_active_account(s: S<'_>, id: String) -> CmdResult<()> {
    auth::set_active_account(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn get_skin_info(s: S<'_>) -> CmdResult<skins::SkinInfo> {
    skins::get_skin_info(&s.ctx).await.map_err(err)
}

#[tauri::command]
async fn upload_skin(s: S<'_>, path: String, variant: String) -> CmdResult<()> {
    skins::upload_skin(&s.ctx, &PathBuf::from(path), &variant).await.map_err(err)
}

#[tauri::command]
async fn skin_library(s: S<'_>) -> CmdResult<Vec<skins::LibSkin>> {
    skins::library_list(&s.ctx).await.map_err(err)
}

#[tauri::command]
async fn skin_library_add(s: S<'_>, path: Option<String>, name: String, variant: String) -> CmdResult<skins::LibSkin> {
    match path {
        Some(p) => skins::library_add(&s.ctx, &PathBuf::from(p), &name, &variant).await,
        None => skins::library_add_current(&s.ctx, &name).await,
    }
    .map_err(err)
}

#[tauri::command]
async fn skin_library_apply(s: S<'_>, id: String, variant: Option<String>) -> CmdResult<()> {
    skins::library_apply(&s.ctx, &id, variant.as_deref()).await.map_err(err)
}

#[tauri::command]
async fn skin_library_remove(s: S<'_>, id: String) -> CmdResult<()> {
    skins::library_remove(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn skin_library_update(s: S<'_>, id: String, name: String, variant: String) -> CmdResult<()> {
    skins::library_update(&s.ctx, &id, &name, &variant).await.map_err(err)
}

#[tauri::command]
async fn reset_skin(s: S<'_>) -> CmdResult<()> {
    skins::reset_skin(&s.ctx).await.map_err(err)
}

#[tauri::command]
async fn set_cape(s: S<'_>, cape_id: Option<String>) -> CmdResult<()> {
    skins::set_cape(&s.ctx, cape_id.as_deref()).await.map_err(err)
}

// ---- settings ----------------------------------------------------------------------

#[tauri::command]
async fn get_settings(s: S<'_>) -> CmdResult<Settings> {
    Ok(instances::get_settings(&s.ctx).await)
}

#[tauri::command]
async fn save_settings(s: S<'_>, settings: Settings) -> CmdResult<Settings> {
    let saved = instances::save_settings(&s.ctx, settings).await.map_err(err)?;
    refresh_discord(&s).await;
    Ok(saved)
}

#[tauri::command]
async fn detect_javas(s: S<'_>) -> CmdResult<Vec<java::JavaInstall>> {
    Ok(java::detect_javas(&s.ctx, true).await)
}

#[tauri::command]
async fn test_java(path: String) -> CmdResult<u32> {
    java::java_major(&PathBuf::from(&path)).await.ok_or_else(|| "That is not a working Java.".to_string())
}

// ---- mods, packs, worlds, servers, logs ------------------------------------------------

#[tauri::command]
async fn list_files(s: S<'_>, id: String, kind: String) -> CmdResult<Vec<content::FileEntry>> {
    content::list_files(&s.ctx, &id, &kind).await.map_err(err)
}

#[tauri::command]
async fn set_file_enabled(s: S<'_>, id: String, kind: String, name: String, enabled: bool) -> CmdResult<()> {
    content::set_file_enabled(&s.ctx, &id, &kind, &name, enabled).await.map_err(err)
}

#[tauri::command]
async fn delete_file(s: S<'_>, id: String, kind: String, name: String) -> CmdResult<()> {
    content::delete_file(&s.ctx, &id, &kind, &name).await.map_err(err)
}

#[tauri::command]
async fn add_file(s: S<'_>, id: String, kind: String, path: String) -> CmdResult<String> {
    content::add_file(&s.ctx, &id, &kind, &PathBuf::from(path)).await.map_err(err)
}

#[tauri::command]
async fn check_updates(s: S<'_>, id: String, kind: String) -> CmdResult<Vec<modrinth::UpdateInfo>> {
    let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
    modrinth::check_updates(&s.ctx, &inst, &kind).await.map_err(err)
}

#[tauri::command]
async fn apply_update(s: S<'_>, id: String, kind: String, filename: String, version_id: String) -> CmdResult<String> {
    let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
    modrinth::apply_update(&s.ctx, &inst, &kind, &filename, &version_id).await.map_err(err)
}

#[tauri::command]
async fn list_worlds(s: S<'_>, id: String) -> CmdResult<Vec<content::World>> {
    content::list_worlds(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn delete_world(s: S<'_>, id: String, folder: String) -> CmdResult<()> {
    content::delete_world(&s.ctx, &id, &folder).await.map_err(err)
}

#[tauri::command]
async fn backup_world(s: S<'_>, id: String, folder: String) -> CmdResult<String> {
    content::backup_world(&s.ctx, &id, &folder).await.map(|p| p.to_string_lossy().to_string()).map_err(err)
}

#[tauri::command]
async fn list_screenshots(s: S<'_>, id: String) -> CmdResult<Vec<content::Screenshot>> {
    content::list_screenshots(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn delete_screenshot(s: S<'_>, id: String, name: String) -> CmdResult<()> {
    content::delete_screenshot(&s.ctx, &id, &name).await.map_err(err)
}

#[tauri::command]
async fn list_servers(s: S<'_>, id: String) -> CmdResult<Vec<content::Server>> {
    content::list_servers(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn save_servers(s: S<'_>, id: String, servers: Vec<content::Server>) -> CmdResult<()> {
    content::save_servers(&s.ctx, &id, servers).await.map_err(err)
}

#[tauri::command]
async fn list_logs(s: S<'_>, id: String) -> CmdResult<Vec<content::LogFile>> {
    content::list_logs(&s.ctx, &id).await.map_err(err)
}

#[tauri::command]
async fn read_log(s: S<'_>, id: String, path: String) -> CmdResult<String> {
    content::read_log(&s.ctx, &id, &path).await.map_err(err)
}

#[tauri::command]
async fn sync_options(s: S<'_>, id: String, source: String, files: Vec<String>) -> CmdResult<Vec<String>> {
    content::sync_options(&s.ctx, &id, &source, &files).await.map_err(err)
}

#[tauri::command]
fn has_vanilla_dir() -> bool {
    std::env::var_os("VOX_MC_DIR").is_some() || content::minecraft_dir().is_some()
}

// ---- Modrinth ----------------------------------------------------------------------

#[tauri::command]
async fn mr_search_filtered(s: S<'_>, filters: modrinth::SearchFilters) -> CmdResult<modrinth::SearchResult> {
    modrinth::search_filtered(&s.ctx, &filters).await.map_err(err)
}

#[tauri::command]
async fn mr_search(s: S<'_>, query: String, kind: String, mc: String, loader: Option<Loader>, index: String, offset: u32) -> CmdResult<modrinth::SearchResult> {
    modrinth::search(&s.ctx, &query, &kind, &mc, loader, &index, offset).await.map_err(err)
}

#[tauri::command]
async fn mr_versions(s: S<'_>, project_id: String, kind: String, mc: String, loader: Option<Loader>) -> CmdResult<Vec<modrinth::ModVersion>> {
    modrinth::project_versions(&s.ctx, &project_id, &kind, &mc, loader).await.map_err(err)
}

#[tauri::command]
async fn mr_install(app: AppHandle, s: S<'_>, id: String, kind: String, version_id: String, title: String, icon_url: Option<String>, task_id: String) -> CmdResult<Vec<String>> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
        let v = modrinth::get_version(&s.ctx, &version_id).await.map_err(err)?;
        modrinth::install_with_dependencies(&s.ctx, &inst, &kind, &v, &title, icon_url, &task_report(&app, task_id)).await.map_err(err)
    }).await
}

/// Downloads a modpack version's file into the cache and returns where it is.
async fn fetch_pack(ctx: &Ctx, v: &modrinth::ModVersion, report: &mojang::Report) -> Result<PathBuf, String> {
    let file = v.primary_file().ok_or_else(|| "This modpack version has no file.".to_string())?;
    let name = modrinth::safe_filename(&file.filename).map_err(err)?;
    let dest = ctx.cache_dir().join("modpacks").join(name);
    report("Downloading modpack", 0, 1);
    let expect = util::Expect { sha1: (!file.sha1.is_empty()).then(|| file.sha1.clone()), size: (file.size > 0).then_some(file.size) };
    util::download(&ctx.http, &file.url, &dest, &expect).await.map_err(err)?;
    Ok(dest)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackUpdate {
    current: String,
    latest: modrinth::ModVersion,
}

/// The newest version of the modpack this instance was installed from, if it is newer than what is installed.
#[tauri::command]
async fn pack_update_check(s: S<'_>, id: String) -> CmdResult<Option<PackUpdate>> {
    let Some(st) = packs::load_state(&s.ctx, &id).await else { return Ok(None) };
    if st.project_id.is_empty() {
        return Ok(None);
    }
    let versions = modrinth::project_versions(&s.ctx, &st.project_id, "modpacks", "", None).await.map_err(err)?;
    let current = versions.iter().find(|v| v.id == st.version_id).map(|v| v.date_published.clone());
    let newest = versions.iter().filter(|v| v.version_type == "release").max_by(|a, b| a.date_published.cmp(&b.date_published)).or_else(|| versions.iter().max_by(|a, b| a.date_published.cmp(&b.date_published)));
    Ok(newest.filter(|n| n.id != st.version_id && current.as_ref().map(|c| n.date_published > *c).unwrap_or(true)).map(|n| PackUpdate { current: st.version_number.clone(), latest: n.clone() }))
}

#[tauri::command]
async fn pack_update(app: AppHandle, s: S<'_>, id: String, version_id: String, task_id: String) -> CmdResult<InstanceConfig> {
    scoped(&app.clone(), &task_id.clone(), async move {
        if s.running.lock().unwrap().contains_key(&id) {
            return Err("Close the game before updating its modpack.".to_string());
        }
        let report = task_report(&app, task_id);
        let v = modrinth::get_version(&s.ctx, &version_id).await.map_err(err)?;
        let dest = fetch_pack(&s.ctx, &v, &report).await?;
        let threads = instances::get_settings(&s.ctx).await.download_threads as usize;
        packs::update_pack_in_place(&s.ctx, &id, &dest, &v.project_id, &v.id, &v.version_number, threads, &report).await.map_err(err)
    }).await
}

#[tauri::command]
async fn check_conflicts(s: S<'_>, id: String) -> CmdResult<Vec<conflicts::Conflict>> {
    conflicts::check(&s.ctx, &id).await.map_err(err)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatedMod {
    title: String,
    from: String,
    to: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceUpdates {
    id: String,
    name: String,
    updated: Vec<UpdatedMod>,
    failed: Vec<String>,
    skipped: Option<String>,
}

/// Updates every mod that has a newer version, in every instance. Each instance gets a snapshot first.
#[tauri::command]
async fn update_all_mods(app: AppHandle, s: S<'_>, task_id: String) -> CmdResult<Vec<InstanceUpdates>> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let report = task_report(&app, task_id);
        let all = instances::list_instances(&s.ctx).await.map_err(err)?;
        let total = all.len() as u64;
        let mut out = Vec::new();
        for (n, inst) in all.into_iter().enumerate() {
            report(&format!("Checking {}", inst.name), n as u64, total);
            let mut row = InstanceUpdates { id: inst.id.clone(), name: inst.name.clone(), updated: vec![], failed: vec![], skipped: None };
            if s.running.lock().unwrap().contains_key(&inst.id) {
                row.skipped = Some("The game is running".into());
                out.push(row);
                continue;
            }
            let ups = match modrinth::check_updates(&s.ctx, &inst, "mods").await {
                Ok(u) => u,
                Err(e) => {
                    row.skipped = Some(format!("Couldn't check: {}", err(e)));
                    out.push(row);
                    continue;
                }
            };
            if ups.is_empty() {
                continue;
            }
            let _ = snapshots::create(&s.ctx, &inst.id, "Before updating all mods").await;
            for u in ups {
                report(&format!("Updating {}", if u.title.is_empty() { &u.filename } else { &u.title }), n as u64, total);
                let title = if u.title.is_empty() { u.filename.clone() } else { u.title.clone() };
                match modrinth::apply_update(&s.ctx, &inst, "mods", &u.filename, &u.new_version.id).await {
                    Ok(_) => row.updated.push(UpdatedMod { title, from: u.current_version.clone(), to: u.new_version.version_number.clone() }),
                    Err(e) if err(&e).to_lowercase().contains("cancel") => return Err("Cancelled".to_string()),
                    Err(_) => row.failed.push(title),
                }
            }
            out.push(row);
        }
        report("Done", total, total);
        Ok(out)
    }).await
}

#[tauri::command]
async fn mr_plan(s: S<'_>, id: String, kind: String, version_id: String, title: String) -> CmdResult<modrinth::Plan> {
    let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
    let v = modrinth::get_version(&s.ctx, &version_id).await.map_err(err)?;
    modrinth::plan_install(&s.ctx, &inst, &kind, &v, &title).await.map_err(err)
}

#[tauri::command]
async fn mr_install_modpack(app: AppHandle, s: S<'_>, version_id: String, task_id: String) -> CmdResult<InstanceConfig> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let report = task_report(&app, task_id);
        let v = modrinth::get_version(&s.ctx, &version_id).await.map_err(err)?;
        let file = v.primary_file().ok_or_else(|| "This modpack version has no file.".to_string())?;
        let name = modrinth::safe_filename(&file.filename).map_err(err)?;
        let dest = s.ctx.cache_dir().join("modpacks").join(name);
        report("Downloading modpack", 0, 1);
        let expect = util::Expect { sha1: (!file.sha1.is_empty()).then(|| file.sha1.clone()), size: (file.size > 0).then_some(file.size) };
        util::download(&s.ctx.http, &file.url, &dest, &expect).await.map_err(err)?;
        let threads = instances::get_settings(&s.ctx).await.download_threads as usize;
        let inst = packs::import_mrpack(&s.ctx, &dest, &report, threads).await.map_err(err)?;
        let _ = packs::set_origin(&s.ctx, &inst.id, &v.project_id, &v.id, &v.version_number).await;
        Ok(inst)
    }).await
}

// ---- launcher updates, uninstall, notifications, Vox Client -----------------------------

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn check_launcher_update(s: S<'_>) -> CmdResult<Option<update::UpdateInfo>> {
    let set = instances::get_settings(&s.ctx).await;
    if !set.check_launcher_updates {
        return Ok(None);
    }
    let repo = repo_of(&s.ctx).await;
    update::check(&s.ctx, &repo, env!("CARGO_PKG_VERSION")).await.map_err(err)
}

/// Downloads the new installer, starts it and closes the launcher so it can be replaced.
#[tauri::command]
async fn install_launcher_update(app: AppHandle, s: S<'_>, url: String, name: String, size: Option<u64>, task_id: String) -> CmdResult<()> {
    scoped(&app.clone(), &task_id.clone(), async move {
        if !cfg!(windows) {
            return Err("Automatic updates are only available on Windows. Open the release page instead.".into());
        }
        let repo = repo_of(&s.ctx).await;
        task_report(&app, task_id)("Downloading update", 0, 1);
        let path = update::download_installer(&s.ctx, &repo, &url, &name, size).await.map_err(err)?;
        open::that(&path).map_err(|e| format!("Could not start the installer: {e}"))?;
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            app2.exit(0);
        });
        Ok(())
    }).await
}

/// Starts the uninstaller that the installer registered, optionally wiping the data folder first.
#[tauri::command]
async fn uninstall_app(app: AppHandle, s: S<'_>, delete_data: bool) -> CmdResult<()> {
    if paths::is_portable() {
        return Err("This is a portable copy. Delete its folder to remove it.".into());
    }
    if !cfg!(windows) {
        return Err("Uninstall Vox from your system's app list (Linux and macOS builds are not installed by the Vox installer).".into());
    }
    let exe = std::env::current_exe().map_err(err)?;
    let un = exe.parent().map(|d| d.join("uninstall.exe")).filter(|p| p.is_file());
    let Some(un) = un else {
        return Err("The uninstaller was not found next to the launcher. Use Windows Settings → Apps → Vox Launcher → Uninstall.".into());
    };
    if delete_data {
        let root = s.ctx.root.as_ref().clone();
        let ours = root.join("instances").is_dir() || root.join("settings.json").is_file();
        if ours && root.components().count() >= 3 {
            let _ = tokio::fs::remove_dir_all(&root).await;
        }
    }
    std::process::Command::new(&un).spawn().map_err(|e| format!("Could not start the uninstaller: {e}"))?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        app2.exit(0);
    });
    Ok(())
}

/// Desktop notification, only when enabled and the launcher window is not in front.
#[tauri::command]
async fn notify(app: AppHandle, s: S<'_>, title: String, body: String, force: bool) -> CmdResult<()> {
    if !instances::get_settings(&s.ctx).await.notifications {
        return Ok(());
    }
    if !force && app.get_webview_window("main").map(|w| w.is_focused().unwrap_or(false)).unwrap_or(false) {
        return Ok(());
    }
    let _ = app.notification().builder().title(title).body(body).show();
    Ok(())
}

#[tauri::command]
async fn client_status(s: S<'_>, id: String) -> CmdResult<client_mod::ClientStatus> {
    let repo = repo_of(&s.ctx).await;
    Ok(client_mod::status(&s.ctx, &repo, &id).await)
}

#[tauri::command]
async fn client_install(app: AppHandle, s: S<'_>, id: String, task_id: String) -> CmdResult<String> {
    scoped(&app.clone(), &task_id.clone(), async move {
        let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
        let repo = repo_of(&s.ctx).await;
        client_mod::install(&s.ctx, &repo, &inst, &task_report(&app, task_id)).await.map_err(err)
    }).await
}

#[tauri::command]
async fn client_remove(s: S<'_>, id: String) -> CmdResult<()> {
    client_mod::remove(&s.ctx, &id).await.map_err(err)
}

// ---- game --------------------------------------------------------------------------

#[tauri::command]
async fn launch(app: AppHandle, s: S<'_>, id: String, join_server: Option<String>, join_world: Option<String>) -> CmdResult<()> {
    {
        let running = s.running.lock().unwrap();
        let mut preparing = s.preparing.lock().unwrap();
        if running.contains_key(&id) || preparing.contains(&id) {
            return Err("This instance is already running.".into());
        }
        preparing.insert(id.clone());
    }
    let state: Arc<AppState> = s.inner().clone();
    let join = (join_server.is_some() || join_world.is_some()).then(|| launch::Join { server: join_server, world: join_world });
    let result = do_launch(&app, &state, &id, join).await;
    state.preparing.lock().unwrap().remove(&id);
    result.map_err(err)
}

async fn do_launch(app: &AppHandle, state: &Arc<AppState>, id: &str, join: Option<launch::Join>) -> anyhow::Result<()> {
    let (a1, a2, a3) = (app.clone(), app.clone(), app.clone());
    let (i1, i2) = (id.to_string(), id.to_string());
    let st = state.clone();
    let st_log = state.clone();
    let i3 = id.to_string();
    let hooks = launch::LaunchHooks {
        progress: Arc::new(move |stage, current, total| {
            let _ = a1.emit("game:progress", ProgressEvent { id: i1.clone(), stage: stage.into(), current, total });
        }),
        log: Arc::new(move |line| {
            if st_log.discord.observe_log(&i2, &line) {
                let st3 = st_log.clone();
                tauri::async_runtime::spawn(async move { refresh_discord(&st3).await });
            }
            let _ = a2.emit("game:log", LogEvent { id: i2.clone(), line });
        }),
        on_exit: Arc::new(move |code| {
            st.running.lock().unwrap().remove(&i3);
            st.discord.game_stopped(&i3);
            let st2 = st.clone();
            let app2 = a3.clone();
            tauri::async_runtime::spawn(async move {
                refresh_discord(&st2).await;
                if st2.running.lock().unwrap().is_empty() {
                    if let Some(w) = app2.get_webview_window("main") {
                        let _ = w.unminimize();
                    }
                }
            });
            let _ = a3.emit("game:state", StateEvent { id: i3.clone(), running: false, code });
        }),
    };
    let settings = instances::get_settings(&state.ctx).await;
    let running = launch::launch_instance_with(&state.ctx, id, hooks, join).await?;
    state.running.lock().unwrap().insert(id.to_string(), running);

    let inst = instances::get_instance(&state.ctx, id).await?;
    let mut cap = inst.loader.as_str().to_string();
    if let Some(f) = cap.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    let version_line = if inst.loader == Loader::Vanilla { format!("Minecraft {}", inst.mc_version) } else { format!("Minecraft {} · {}", inst.mc_version, cap) };
    state.discord.game_started(
        id,
        discord::Game {
            details: if settings.discord_show_instance { format!("Playing {}", inst.name) } else { "Playing Minecraft".to_string() },
            state: version_line,
            started_at_ms: if settings.discord_show_time { util::now_ms() as i64 } else { 0 },
            server: None,
        },
    );
    refresh_discord(state).await;
    if settings.launcher_action == "minimize" {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.minimize();
        }
    }
    let _ = app.emit("game:state", StateEvent { id: id.to_string(), running: true, code: None });
    Ok(())
}

#[tauri::command]
async fn analyze_crash(s: S<'_>, id: String, extra: String, exit_code: Option<i32>) -> CmdResult<crash::Report> {
    let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
    let set = instances::get_settings(&s.ctx).await;
    let env = crash::Env { mc_version: inst.mc_version.clone(), loader: inst.loader.as_str().to_string(), memory_mb: inst.memory_mb.unwrap_or(set.default_memory_mb) };
    let dir = s.ctx.game_dir(&id);
    tokio::task::spawn_blocking(move || crash::gather(&dir, &extra, &env, exit_code)).await.map_err(err)
}

#[tauri::command]
fn kill(s: S<'_>, id: String) {
    if let Some(r) = s.running.lock().unwrap().get(&id) {
        r.kill();
    }
}

#[tauri::command]
fn running(s: S<'_>) -> Vec<String> {
    s.running.lock().unwrap().keys().cloned().collect()
}

/// Only Microsoft/Xbox sign-in pages and Modrinth project pages may be opened from the UI.
pub fn external_url_allowed(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return false };
    if u.scheme() != "https" {
        return false;
    }
    let Some(host) = u.host_str() else { return false };
    ["microsoft.com", "live.com", "xbox.com", "modrinth.com"].iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// The instance a shortcut asked to start, once. The window calls this when it is ready.
#[tauri::command]
fn take_launch_request(s: S<'_>) -> Option<String> {
    s.pending_launch.lock().unwrap().take()
}

#[tauri::command]
async fn create_shortcut(s: S<'_>, id: String, place: String) -> CmdResult<String> {
    let inst = instances::get_instance(&s.ctx, &id).await.map_err(err)?;
    let name = inst.name.clone();
    tokio::task::spawn_blocking(move || shortcut::create(&inst.id, &name, &place))
        .await
        .map_err(err)?
        .map(|p| p.to_string_lossy().to_string())
        .map_err(err)
}

#[tauri::command]
fn open_external(url: String) -> CmdResult<()> {
    if !external_url_allowed(&url) {
        return Err("That link is not allowed.".into());
    }
    open::that_detached(url).map_err(err)
}

pub fn run() {
    let root = std::env::var_os("VOX_DATA_DIR").map(PathBuf::from).unwrap_or_else(paths::default_root);
    if let Some(p) = paths::portable_root() {
        // Keep the web view's own cache and cookies on the stick too.
        if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", p.join("webview"));
        }
    }
    // The proxy is read before the HTTP client exists, so changing it needs a restart.
    let proxy = std::fs::read(root.join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["proxyUrl"].as_str().map(String::from))
        .unwrap_or_default();
    let state = Arc::new(AppState {
        ctx: Ctx::with_proxy(root.clone(), &proxy),
        pending_launch: Mutex::new(shortcut::launch_arg(&std::env::args().collect::<Vec<_>>())),
        running: Mutex::new(HashMap::new()),
        preparing: Mutex::new(HashSet::new()),
        discord: discord::Discord::spawn(),
    });

    let app = tauri::Builder::default()
        // A second start (for example from an instance shortcut) hands its request to the window that is already open.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let Some(id) = shortcut::launch_arg(&argv) {
                let st: State<Arc<AppState>> = app.state();
                *st.pending_launch.lock().unwrap() = Some(id.clone());
                let _ = app.emit("app:launch", id);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state.clone())
        .setup(move |app| {
            // Screenshots and world icons are shown straight from disk; allow only the data folder (and the old .minecraft for nothing else).
            let _ = std::fs::create_dir_all(&root);
            let _ = app.asset_protocol_scope().allow_directory(&root, true);
            let st: State<Arc<AppState>> = app.state();
            let st = st.inner().clone();
            tauri::async_runtime::spawn(async move { refresh_discord(&st).await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_versions,
            task_pause,
            account_skins,
            mr_search_filtered,
            export_theme,
            import_theme,
            pack_update_check,
            pack_update,
            check_conflicts,
            update_all_mods,
            task_cancel,
            list_loader_versions,
            list_instances,
            create_instance,
            update_instance,
            delete_instance,
            duplicate_instance,
            open_path,
            data_dir,
            is_portable,
            list_snapshots,
            create_snapshot,
            restore_snapshot,
            delete_snapshot,
            open_data_dir,
            pick_file,
            pick_save,
            pick_folder,
            import_file,
            detect_imports,
            scan_import_folder,
            import_instances,
            export_instance,
            list_accounts,
            login,
            cancel_login,
            add_offline_account,
            remove_account,
            set_active_account,
            get_skin_info,
            upload_skin,
            reset_skin,
            skin_library,
            skin_library_add,
            skin_library_apply,
            skin_library_remove,
            skin_library_update,
            set_cape,
            get_settings,
            save_settings,
            detect_javas,
            test_java,
            list_files,
            set_file_enabled,
            delete_file,
            add_file,
            check_updates,
            apply_update,
            list_worlds,
            delete_world,
            backup_world,
            list_screenshots,
            delete_screenshot,
            list_servers,
            save_servers,
            list_logs,
            read_log,
            sync_options,
            has_vanilla_dir,
            mr_search,
            mr_versions,
            mr_install,
            mr_plan,
            mr_install_modpack,
            launch,
            kill,
            analyze_crash,
            app_version,
            check_launcher_update,
            install_launcher_update,
            uninstall_app,
            notify,
            client_status,
            client_install,
            client_remove,
            running,
            take_launch_request,
            create_shortcut,
            open_external,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Vox Launcher");

    app.run(move |_handle, event| {
        if let tauri::RunEvent::Exit = event {
            for r in state.running.lock().unwrap().values() {
                r.kill();
            }
            state.discord.shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_trusted_links_open() {
        assert!(external_url_allowed("https://www.microsoft.com/link"));
        assert!(external_url_allowed("https://login.live.com/x"));
        assert!(external_url_allowed("https://modrinth.com/mod/sodium"));
        assert!(!external_url_allowed("http://microsoft.com"));
        assert!(!external_url_allowed("https://evilmicrosoft.com"));
        assert!(!external_url_allowed("https://microsoft.com.evil.io"));
        assert!(!external_url_allowed("file:///etc/passwd"));
    }
}
