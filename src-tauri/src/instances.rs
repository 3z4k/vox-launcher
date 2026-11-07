//! Instances and settings, stored as plain JSON files.

use crate::types::*;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use rand::Rng;
use std::path::Path;

pub fn assert_safe_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        bail!("Invalid instance id.")
    }
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    let out: String = out.chars().take(32).collect();
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "instance".into() } else { out }
}

fn config_file(ctx: &Ctx, id: &str) -> std::path::PathBuf {
    ctx.instance_dir(id).join("instance.json")
}

pub async fn list_instances(ctx: &Ctx) -> Result<Vec<InstanceConfig>> {
    tokio::fs::create_dir_all(ctx.instances_dir()).await?;
    let mut rd = tokio::fs::read_dir(ctx.instances_dir()).await?;
    let mut out = Vec::new();
    while let Some(e) = rd.next_entry().await? {
        if !e.file_type().await?.is_dir() {
            continue;
        }
        if let Ok(cfg) = read_json::<InstanceConfig>(&e.path().join("instance.json")).await {
            out.push(cfg);
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.last_played.unwrap_or(c.created)));
    Ok(out)
}

pub async fn get_instance(ctx: &Ctx, id: &str) -> Result<InstanceConfig> {
    assert_safe_id(id)?;
    read_json(&config_file(ctx, id)).await.map_err(|_| anyhow!("Instance not found."))
}

pub async fn create_instance(ctx: &Ctx, input: NewInstance) -> Result<InstanceConfig> {
    let name = {
        let n = input.name.trim();
        if n.is_empty() { format!("Minecraft {}", input.mc_version) } else { n.to_string() }
    };
    let suffix: [u8; 2] = rand::thread_rng().gen();
    let id = format!("{}-{}", slugify(&name), hex::encode(suffix));
    let vanilla = input.loader == Loader::Vanilla;
    let cfg = InstanceConfig {
        id: id.clone(),
        name,
        mc_version: input.mc_version.clone(),
        loader: input.loader,
        loader_version: if vanilla { String::new() } else { input.loader_version },
        version_id: if vanilla { input.mc_version } else { String::new() },
        created: now_ms(),
        last_played: None,
        played_ms: 0,
        group: input.group.trim().to_string(),
        icon: input.icon,
        notes: String::new(),
        memory_mb: None,
        min_memory_mb: None,
        java_path: String::new(),
        jvm_args: String::new(),
        extra_game_args: String::new(),
        width: None,
        height: None,
        fullscreen: None,
        wrapper: String::new(),
        pre_launch: String::new(),
        post_exit: String::new(),
        env_vars: String::new(),
        sync_options_from: String::new(),
        join_server: String::new(),
        join_world: String::new(),
        verify_next: None,
    };
    tokio::fs::create_dir_all(ctx.game_dir(&id)).await?;
    write_json(&config_file(ctx, &id), &cfg).await?;
    Ok(cfg)
}

/// Merges the editable keys of `patch` (camelCase JSON) into the stored config.
pub async fn update_instance(ctx: &Ctx, id: &str, patch: serde_json::Value) -> Result<InstanceConfig> {
    let cur = get_instance(ctx, id).await?;
    let mut obj = match serde_json::to_value(&cur)? {
        serde_json::Value::Object(o) => o,
        _ => unreachable!(),
    };
    if let Some(p) = patch.as_object() {
        for (k, v) in p {
            if EDITABLE_INSTANCE_KEYS.contains(&k.as_str()) {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let mut next: InstanceConfig =
        serde_json::from_value(serde_json::Value::Object(obj)).map_err(|e| anyhow!("Invalid instance setting: {e}"))?;
    next.name = if next.name.trim().is_empty() { cur.name.clone() } else { next.name.trim().to_string() };
    next.group = next.group.trim().to_string();
    write_json(&config_file(ctx, id), &next).await?;
    Ok(next)
}

pub async fn delete_instance(ctx: &Ctx, id: &str) -> Result<()> {
    assert_safe_id(id)?;
    match tokio::fs::remove_dir_all(ctx.instance_dir(id)).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

pub async fn duplicate_instance(ctx: &Ctx, id: &str) -> Result<InstanceConfig> {
    let src = get_instance(ctx, id).await?;
    let copy = create_instance(
        ctx,
        NewInstance {
            name: format!("{} (copy)", src.name),
            mc_version: src.mc_version.clone(),
            loader: src.loader,
            loader_version: src.loader_version.clone(),
            group: src.group.clone(),
            icon: src.icon.clone(),
        },
    )
    .await?;
    let (from, to) = (ctx.game_dir(id), ctx.game_dir(&copy.id));
    if from.exists() {
        let _ = tokio::fs::remove_dir_all(&to).await;
        tokio::task::spawn_blocking(move || copy_dir(&from, &to)).await??;
    }
    let mut patch = serde_json::to_value(&src)?;
    for k in ["id", "name", "mcVersion", "loader", "loaderVersion", "created", "lastPlayed", "playedMs", "verifyNext"] {
        patch.as_object_mut().unwrap().remove(k);
    }
    update_instance(ctx, &copy.id, patch).await
}

pub async fn open_instance_folder(ctx: &Ctx, id: &str) -> Result<()> {
    assert_safe_id(id)?;
    let dir = ctx.game_dir(id);
    tokio::fs::create_dir_all(&dir).await?;
    open::that_detached(&dir).map_err(|e| anyhow!("{e}"))
}

// --- settings ---------------------------------------------------------------

pub async fn get_settings(ctx: &Ctx) -> Settings {
    read_json_or(&ctx.settings_file(), Settings::default()).await.sanitized()
}

pub async fn save_settings(ctx: &Ctx, next: Settings) -> Result<Settings> {
    let clean = next.sanitized();
    write_json(&ctx.settings_file(), &clean).await?;
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        let d = std::env::temp_dir().join(format!("vox-inst-{}-{}", std::process::id(), now_ms()));
        Ctx::new(d)
    }

    #[test]
    fn slugs_and_safe_ids() {
        assert_eq!(slugify("My Cool Pack!"), "my-cool-pack");
        assert_eq!(slugify("***"), "instance");
        assert!(assert_safe_id("my-pack-1a2b").is_ok());
        assert!(assert_safe_id("../evil").is_err());
        assert!(assert_safe_id("-lead").is_err());
        assert!(assert_safe_id("Upper").is_err());
        assert!(assert_safe_id("").is_err());
    }

    #[tokio::test]
    async fn create_update_duplicate_delete() {
        let c = ctx();
        let inst = create_instance(&c, NewInstance { name: "  ".into(), mc_version: "1.21.1".into(), loader: Loader::Vanilla, loader_version: String::new(), group: " Fav ".into(), icon: "cube".into() })
            .await
            .unwrap();
        assert_eq!(inst.name, "Minecraft 1.21.1");
        assert_eq!(inst.version_id, "1.21.1");

        let patch = serde_json::json!({ "name": "Renamed", "memoryMb": 2048, "width": null, "mcVersion": "9.9", "id": "evil", "notes": "hi", "fullscreen": true });
        let up = update_instance(&c, &inst.id, patch).await.unwrap();
        assert_eq!((up.name.as_str(), up.memory_mb, up.mc_version.as_str(), up.id.as_str()), ("Renamed", Some(2048), "1.21.1", inst.id.as_str()));
        assert_eq!((up.notes.as_str(), up.fullscreen, up.group.as_str(), up.icon.as_str()), ("hi", Some(true), "Fav", "cube"));
        assert!(update_instance(&c, &inst.id, serde_json::json!({ "memoryMb": "lots" })).await.is_err());

        tokio::fs::write(c.game_dir(&inst.id).join("options.txt"), "x").await.unwrap();
        let dup = duplicate_instance(&c, &inst.id).await.unwrap();
        assert!(dup.name.ends_with("(copy)"));
        assert!(c.game_dir(&dup.id).join("options.txt").exists());
        assert_eq!(dup.memory_mb, Some(2048));

        assert_eq!(list_instances(&c).await.unwrap().len(), 2);
        delete_instance(&c, &inst.id).await.unwrap();
        assert_eq!(list_instances(&c).await.unwrap().len(), 1);
        assert!(get_instance(&c, "../x").await.is_err());
    }

    #[tokio::test]
    async fn settings_are_clamped_and_old_files_still_load() {
        let c = ctx();
        let s = save_settings(&c, Settings { default_memory_mb: 10, theme: "nope".into(), layout: "x".into(), accent: "red".into(), ui_scale: 500, ..Default::default() })
            .await
            .unwrap();
        assert_eq!((s.default_memory_mb, s.theme.as_str(), s.accent.as_str(), s.ui_scale), (512, "voxel", "", 150));
        assert_eq!(s.layout, "cover");
        assert_eq!(get_settings(&c).await.default_memory_mb, 512);
        for (input, want) in [("https://github.com/Me/vox.git", "Me/vox"), ("github.com/me/vox/", "me/vox"), ("me/vox", "me/vox"), ("nonsense", ""), ("a/b/c", ""), ("a b/c", "")] {
            let s = save_settings(&c, Settings { update_repo: input.into(), ..Default::default() }).await.unwrap();
            assert_eq!(s.update_repo, want, "{input}");
        }
        // a settings.json written by the 0.2 build has only five keys
        tokio::fs::write(c.settings_file(), r#"{"defaultMemoryMb":6144,"theme":"bold","discordEnabled":false}"#).await.unwrap();
        let old = get_settings(&c).await;
        assert_eq!((old.default_memory_mb, old.theme.as_str(), old.discord_enabled, old.download_threads), (6144, "bold", false, 16));
        let ok = Settings { accent: "#FF6B3D".into(), ..Default::default() }.sanitized();
        assert_eq!(ok.accent, "#FF6B3D");
    }
}
