//! Instance snapshots: a saved copy of an instance's mods, mod configs and settings that can be
//! rolled back to. Worlds are never touched. Jars are hard-linked where possible so a snapshot
//! costs almost no disk space.

use crate::instances::{assert_safe_id, get_instance};
use crate::types::{InstanceConfig, Loader};
use crate::util::{now_ms, read_json, write_json, Ctx};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub created: u64,
    pub mc_version: String,
    pub loader: Loader,
    pub loader_version: String,
    pub mods: Vec<String>,
}

fn dir(ctx: &Ctx, id: &str) -> PathBuf {
    ctx.instance_dir(id).join("snapshots")
}

fn assert_snap_id(s: &str) -> Result<()> {
    let ok = s.len() > 2 && s.starts_with("s-") && s[2..].chars().all(|c| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        bail!("Invalid snapshot id.")
    }
}

fn link_or_copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map(|_| ())
}

fn copy_tree(src: &Path, dst: &Path, link: bool) -> std::io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to, link)?;
        } else if link {
            link_or_copy(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

fn mod_names(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d)
        .map(|rd| rd.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

pub async fn list(ctx: &Ctx, id: &str) -> Result<Vec<Snapshot>> {
    assert_safe_id(id)?;
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir(ctx, id)) {
        for e in rd.flatten() {
            if let Ok(s) = read_json::<Snapshot>(&e.path().join("snapshot.json")).await {
                out.push(s);
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.created));
    Ok(out)
}

pub async fn create(ctx: &Ctx, id: &str, name: &str) -> Result<Snapshot> {
    let inst = get_instance(ctx, id).await?;
    let mut created = now_ms();
    while dir(ctx, id).join(format!("s-{created}")).exists() {
        created += 1;
    }
    let sid = format!("s-{created}");
    let root = dir(ctx, id).join(&sid);
    let game = ctx.game_dir(id);
    let (r2, g2) = (root.clone(), game.clone());
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        copy_tree(&g2.join("mods"), &r2.join("mods"), true)?;
        copy_tree(&g2.join("config"), &r2.join("config"), false)?;
        std::fs::create_dir_all(&r2)?;
        let meta = g2.join(".vox-content.json");
        if meta.is_file() {
            std::fs::copy(meta, r2.join("content.json"))?;
        }
        Ok(())
    })
    .await??;
    let name = name.trim();
    let snap = Snapshot {
        id: sid,
        name: if name.is_empty() { "Snapshot".into() } else { name.chars().take(80).collect() },
        created,
        mc_version: inst.mc_version.clone(),
        loader: inst.loader,
        loader_version: inst.loader_version.clone(),
        mods: mod_names(&root.join("mods")),
    };
    write_json(&root.join("instance.json"), &inst).await?;
    write_json(&root.join("snapshot.json"), &snap).await?;
    Ok(snap)
}

pub async fn delete(ctx: &Ctx, id: &str, snap: &str) -> Result<()> {
    assert_safe_id(id)?;
    assert_snap_id(snap)?;
    match tokio::fs::remove_dir_all(dir(ctx, id).join(snap)).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Puts the instance back the way the snapshot saved it. A "before restoring" snapshot is made first,
/// so a restore can itself be undone.
pub async fn restore(ctx: &Ctx, id: &str, snap: &str) -> Result<InstanceConfig> {
    assert_safe_id(id)?;
    assert_snap_id(snap)?;
    let root = dir(ctx, id).join(snap);
    let meta: Snapshot = read_json(&root.join("snapshot.json")).await.map_err(|_| anyhow!("That snapshot no longer exists."))?;
    let saved: InstanceConfig = read_json(&root.join("instance.json")).await?;
    let cur = get_instance(ctx, id).await?;
    create(ctx, id, &format!("Before restoring “{}”", meta.name)).await?;

    let game = ctx.game_dir(id);
    let (r2, g2) = (root.clone(), game.clone());
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        for sub in ["mods", "config"] {
            let target = g2.join(sub);
            if target.exists() {
                std::fs::remove_dir_all(&target)?;
            }
            copy_tree(&r2.join(sub), &target, sub == "mods")?;
        }
        let m = g2.join(".vox-content.json");
        let _ = std::fs::remove_file(&m);
        if r2.join("content.json").is_file() {
            std::fs::copy(r2.join("content.json"), m)?;
        }
        Ok(())
    })
    .await??;

    let next = InstanceConfig {
        id: cur.id.clone(),
        name: cur.name.clone(),
        created: cur.created,
        last_played: cur.last_played,
        played_ms: cur.played_ms,
        group: cur.group.clone(),
        icon: cur.icon.clone(),
        notes: cur.notes.clone(),
        ..saved
    };
    write_json(&ctx.instance_dir(id).join("instance.json"), &next).await?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{create_instance, update_instance};
    use crate::types::NewInstance;

    #[tokio::test]
    async fn snapshot_and_restore_roundtrip() {
        let c = Ctx::new(std::env::temp_dir().join(format!("vox-snap-{}-{}", std::process::id(), now_ms())));
        let inst = create_instance(&c, NewInstance { name: "Pack".into(), mc_version: "1.20.1".into(), loader: Loader::Fabric, loader_version: "0.15".into(), group: String::new(), icon: "cube".into() }).await.unwrap();
        let game = c.game_dir(&inst.id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir_all(game.join("saves/World1")).unwrap();
        std::fs::write(game.join("mods/a.jar"), b"A1").unwrap();
        std::fs::write(game.join("saves/World1/level.dat"), b"world").unwrap();
        let s = create(&c, &inst.id, "  Good state ").await.unwrap();
        assert_eq!(s.name, "Good state");
        assert_eq!(s.mods, vec!["a.jar"]);

        // Change things: replace a mod, add one, change a setting, play in the world.
        std::fs::remove_file(game.join("mods/a.jar")).unwrap();
        std::fs::write(game.join("mods/a-new.jar"), b"A2").unwrap();
        std::fs::write(game.join("mods/b.jar"), b"B").unwrap();
        std::fs::write(game.join("saves/World1/level.dat"), b"world-progress").unwrap();
        update_instance(&c, &inst.id, serde_json::json!({"memoryMb": 8192, "name": "Renamed"})).await.unwrap();

        let back = restore(&c, &inst.id, &s.id).await.unwrap();
        assert_eq!(back.memory_mb, None);
        assert_eq!(back.name, "Renamed", "identity fields are kept");
        assert_eq!(mod_names(&game.join("mods")), vec!["a.jar"]);
        assert_eq!(std::fs::read(game.join("mods/a.jar")).unwrap(), b"A1");
        assert_eq!(std::fs::read(game.join("saves/World1/level.dat")).unwrap(), b"world-progress", "worlds are never rolled back");

        // A restore leaves a safety snapshot behind, and the old state can be restored from it.
        let all = list(&c, &inst.id).await.unwrap();
        assert_eq!(all.len(), 2);
        let safety = all.iter().find(|s| s.name.starts_with("Before restoring")).unwrap();
        assert_eq!(safety.mods, vec!["a-new.jar", "b.jar"]);
        restore(&c, &inst.id, &safety.id).await.unwrap();
        assert_eq!(mod_names(&game.join("mods")), vec!["a-new.jar", "b.jar"]);

        delete(&c, &inst.id, &s.id).await.unwrap();
        assert!(restore(&c, &inst.id, &s.id).await.is_err());
        assert!(restore(&c, &inst.id, "../x").await.is_err());
        let _ = std::fs::remove_dir_all(&*c.root);
    }
}
