//! Updating an installed Modrinth modpack in place: worlds, the player's own mods and their edited
//! settings survive; the pack's own files are swapped.

mod common;

use serde_json::json;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use vox_launcher_lib::util::{now_ms, Ctx};
use vox_launcher_lib::{packs, snapshots};

fn sha1_hex(b: &[u8]) -> String {
    hex::encode(Sha1::digest(b))
}

fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (n, d) in files {
            z.start_file(n.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn pack(base: &str, mods: &[(&str, &[u8])], overrides: &[(&str, &[u8])], fabric: &str) -> Vec<u8> {
    let files: Vec<_> = mods
        .iter()
        .map(|(n, d)| json!({ "path": format!("mods/{n}"), "hashes": { "sha1": sha1_hex(d) }, "downloads": [format!("{base}/files/{n}")], "fileSize": d.len() }))
        .collect();
    let index = json!({ "name": "Test Pack", "files": files, "dependencies": { "minecraft": "1.21.1", "fabric-loader": fabric } });
    let mut entries = vec![("modrinth.index.json".to_string(), serde_json::to_vec(&index).unwrap())];
    for (n, d) in overrides {
        entries.push((format!("overrides/{n}"), d.to_vec()));
    }
    zip_of(&entries)
}

#[tokio::test]
async fn update_keeps_worlds_settings_and_own_mods() {
    let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
    let f2 = files.clone();
    let srv = common::start(move |req, _| match req.path.strip_prefix("/files/") {
        Some(n) => f2.lock().unwrap().get(n).map(|b| (200, b.clone())).unwrap_or((404, vec![])),
        None => (404, vec![]),
    });
    for (n, d) in [("a-1.jar", &b"A1"[..]), ("b-1.jar", b"B1"), ("a-2.jar", b"A2-new"), ("d-1.jar", b"D1")] {
        files.lock().unwrap().insert(n.to_string(), d.to_vec());
    }
    let dir = std::env::temp_dir().join(format!("vox-packup-{}-{}", std::process::id(), now_ms()));
    let ctx = Ctx::new(dir.join("data"));
    let report: vox_launcher_lib::mojang::Report = Arc::new(|_, _, _| {});

    let v1 = dir.join("v1.mrpack");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&v1, pack(&srv.base, &[("a-1.jar", b"A1"), ("b-1.jar", b"B1")], &[("config/pack.txt", b"default=1"), ("config/gone.txt", b"old")], "0.15.0")).unwrap();
    let inst = packs::import_mrpack(&ctx, &v1, &report, 4).await.unwrap();
    packs::set_origin(&ctx, &inst.id, "proj", "ver1", "1.0").await.unwrap();
    let game = ctx.game_dir(&inst.id);
    assert_eq!(std::fs::read(game.join("mods/a-1.jar")).unwrap(), b"A1");
    assert_eq!(std::fs::read(game.join("config/pack.txt")).unwrap(), b"default=1");

    // The player plays: edits a config, switches a mod off, adds their own mod, builds a world.
    std::fs::write(game.join("config/pack.txt"), b"default=1\nmine=yes").unwrap();
    std::fs::rename(game.join("mods/b-1.jar"), game.join("mods/b-1.jar.disabled")).unwrap();
    std::fs::write(game.join("mods/own.jar"), b"MINE").unwrap();
    std::fs::create_dir_all(game.join("saves/W")).unwrap();
    std::fs::write(game.join("saves/W/level.dat"), b"progress").unwrap();

    // Version 2: a is replaced by a newer file, b stays (switched off by the player), d is new, one config changes and one is dropped.
    let v2 = dir.join("v2.mrpack");
    std::fs::write(&v2, pack(&srv.base, &[("a-2.jar", b"A2-new"), ("b-1.jar", b"B1"), ("d-1.jar", b"D1")], &[("config/pack.txt", b"default=2"), ("config/new.txt", b"fresh")], "0.16.0")).unwrap();
    let out = packs::update_pack_in_place(&ctx, &inst.id, &v2, "proj", "ver2", "2.0", 4, &report).await.unwrap();

    assert!(!game.join("mods/a-1.jar").exists(), "replaced mod is removed");
    assert_eq!(std::fs::read(game.join("mods/a-2.jar")).unwrap(), b"A2-new");
    assert_eq!(std::fs::read(game.join("mods/d-1.jar")).unwrap(), b"D1");
    assert!(game.join("mods/b-1.jar.disabled").exists() && !game.join("mods/b-1.jar").exists(), "a mod the player disabled stays disabled");
    assert_eq!(std::fs::read(game.join("mods/own.jar")).unwrap(), b"MINE", "the player's own mods are untouched");
    assert_eq!(std::fs::read(game.join("saves/W/level.dat")).unwrap(), b"progress", "worlds are untouched");
    assert_eq!(std::fs::read(game.join("config/pack.txt")).unwrap(), b"default=1\nmine=yes", "an edited config is kept");
    assert_eq!(std::fs::read(game.join("config/new.txt")).unwrap(), b"fresh");
    assert!(!game.join("config/gone.txt").exists(), "an untouched file the pack dropped is removed");
    assert_eq!(out.loader_version, "0.16.0");
    assert!(out.version_id.is_empty(), "a new loader version is installed again on the next launch");

    let st = packs::load_state(&ctx, &inst.id).await.unwrap();
    assert_eq!((st.version_id.as_str(), st.version_number.as_str()), ("ver2", "2.0"));

    // A snapshot of the old state was taken, and rolling back brings the old pack mods back.
    let snaps = snapshots::list(&ctx, &inst.id).await.unwrap();
    assert!(snaps.iter().any(|s| s.name.contains("Before updating to 2.0")));
    let snap = snaps.iter().find(|s| s.name.contains("Before updating")).unwrap();
    snapshots::restore(&ctx, &inst.id, &snap.id).await.unwrap();
    assert!(game.join("mods/a-1.jar").exists());
    assert!(!game.join("mods/a-2.jar").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
