//! Modrinth browsing/installing/updating, modpack import and instance export/import against a fake server.

mod common;

use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use vox_launcher_lib::types::*;
use vox_launcher_lib::util::{now_ms, Ctx};
use vox_launcher_lib::{content, instances, modrinth, packs};

fn sha1_hex(b: &[u8]) -> String {
    hex::encode(Sha1::digest(b))
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        for (n, d) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn fabric_jar(name: &str, version: &str) -> Vec<u8> {
    zip_of(&[("fabric.mod.json", format!(r#"{{"id":"x","name":"{name}","version":"{version}"}}"#).as_bytes())])
}

fn version_json(base: &str, id: &str, project: &str, number: &str, filename: &str, jar: &[u8], deps: Value) -> Value {
    json!({
        "id": id, "project_id": project, "name": format!("v{number}"), "version_number": number, "version_type": "release",
        "date_published": "2024-01-01T00:00:00Z", "game_versions": ["1.21.1"], "loaders": ["fabric"],
        "files": [{ "url": format!("{base}/files/{filename}"), "filename": filename, "primary": true, "size": jar.len(), "hashes": { "sha1": sha1_hex(jar) } }],
        "dependencies": deps
    })
}

fn ctx() -> Ctx {
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    Ctx::new(std::env::temp_dir().join(format!("vox-mr-{}-{}", std::process::id(), now_ms())))
}

fn noop() -> vox_launcher_lib::mojang::Report {
    Arc::new(|_, _, _| {})
}

#[tokio::test]
async fn browse_install_with_dependency_update_identify_and_remove() {
    let sodium_v1 = fabric_jar("Sodium", "1.0");
    let sodium_v2 = fabric_jar("Sodium", "2.0");
    let api_jar = fabric_jar("Fabric API", "0.9");
    let manual = fabric_jar("Hand Added", "7");
    let files: HashMap<String, Vec<u8>> = HashMap::from([
        ("/files/sodium-1.0.jar".to_string(), sodium_v1.clone()),
        ("/files/sodium-2.0.jar".to_string(), sodium_v2.clone()),
        ("/files/fabric-api.jar".to_string(), api_jar.clone()),
    ]);
    let (s1, s2, ap, mn) = (sodium_v1.clone(), sodium_v2.clone(), api_jar.clone(), manual.clone());
    let server = common::start(move |req, base| {
        if let Some(f) = files.get(&req.path) {
            return (200, f.clone());
        }
        let ok = |v: Value| (200, serde_json::to_vec(&v).unwrap());
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/search") => ok(json!({ "total_hits": 1, "hits": [{ "project_id": "sodium", "slug": "sodium", "title": "Sodium", "description": "Fast", "icon_url": "https://cdn.modrinth.com/i.png", "author": "jelly", "downloads": 5, "follows": 2, "project_type": "mod", "categories": ["fabric"] }] })),
            ("GET", "/project/sodium/version") => ok(json!([version_json(base, "sodium-v1", "sodium", "1.0", "sodium-1.0.jar", &s1, json!([{ "project_id": "fabric-api", "version_id": null, "dependency_type": "required" }, { "project_id": "optional-thing", "dependency_type": "optional" }]))])),
            ("GET", "/project/fabric-api/version") => ok(json!([version_json(base, "api-v1", "fabric-api", "0.9", "fabric-api.jar", &ap, json!([]))])),
            ("GET", "/project/fabric-api") => ok(json!({ "title": "Fabric API", "icon_url": null })),
            ("GET", "/project/ghost/version") => ok(json!([])),
            ("GET", "/project/ghost") => ok(json!({ "title": "Ghost Lib", "icon_url": null })),
            ("GET", "/project/handproj") => ok(json!({ "title": "Hand Added Pretty", "icon_url": null })),
            ("GET", "/version/needy-v1") => ok(version_json(base, "needy-v1", "needy", "1.0", "needy.jar", &s1, json!([
                { "project_id": "ghost", "version_id": null, "dependency_type": "required" },
                { "project_id": "fabric-api", "version_id": null, "dependency_type": "required" },
                { "project_id": "handproj", "version_id": null, "dependency_type": "incompatible" }]))),
            ("GET", "/version/sodium-v2") => ok(version_json(base, "sodium-v2", "sodium", "2.0", "sodium-2.0.jar", &s2, json!([]))),
            ("POST", "/version_files/update") => {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                let mut out = serde_json::Map::new();
                for h in body["hashes"].as_array().unwrap() {
                    let h = h.as_str().unwrap();
                    if h == sha1_hex(&s1) {
                        out.insert(h.into(), version_json(base, "sodium-v2", "sodium", "2.0", "sodium-2.0.jar", &s2, json!([])));
                    } else if h == sha1_hex(&ap) {
                        out.insert(h.into(), version_json(base, "api-v1", "fabric-api", "0.9", "fabric-api.jar", &ap, json!([])));
                    }
                }
                ok(Value::Object(out))
            }
            ("POST", "/version_files") => {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                let mut out = serde_json::Map::new();
                for h in body["hashes"].as_array().unwrap() {
                    if h.as_str().unwrap() == sha1_hex(&mn) {
                        out.insert(h.as_str().unwrap().into(), json!({ "id": "hand-v1", "project_id": "handproj", "version_number": "7" }));
                    }
                }
                ok(Value::Object(out))
            }
            ("GET", "/projects") => ok(json!([{ "id": "handproj", "title": "Hand Added Pretty", "icon_url": "https://cdn.modrinth.com/h.png" }])),
            _ => (404, b"nope".to_vec()),
        }
    });
    std::env::set_var("VOX_MODRINTH_URL", &server.base);
    let c = ctx();
    let inst = instances::create_instance(&c, NewInstance { name: "Mods".into(), mc_version: "1.21.1".into(), loader: Loader::Fabric, loader_version: "0.16.9".into(), group: String::new(), icon: String::new() }).await.unwrap();

    // search sends the right facets
    let r = modrinth::search(&c, "sod", "mods", "1.21.1", Some(Loader::Quilt), "downloads", 0).await.unwrap();
    assert_eq!((r.total_hits, r.hits[0].title.as_str()), (1, "Sodium"));
    let q = server.log.lock().unwrap().iter().find(|l| l.contains("/search")).unwrap().clone();
    let q = url_decode(&q);
    assert!(q.contains("project_type:mod") && q.contains("versions:1.21.1") && q.contains("categories:quilt") && q.contains("categories:fabric") && q.contains("index=downloads"), "{q}");

    // filtered search: several loaders, categories and an environment; bad tags are dropped, not sent
    server.log.lock().unwrap().clear();
    let f = modrinth::SearchFilters { query: "x".into(), kind: "mods".into(), mc: String::new(), loaders: vec!["forge".into(), "neoforge".into(), "bad tag\"]".into()], categories: vec!["optimization".into(), "../x".into()], environment: "client".into(), sort: "follows".into(), offset: 24, limit: 15 };
    modrinth::search_filtered(&c, &f).await.unwrap();
    let q = url_decode(&server.log.lock().unwrap().iter().find(|l| l.contains("/search")).unwrap().clone());
    assert!(q.contains("\"categories:forge\",\"categories:neoforge\"") && q.contains("categories:optimization") && q.contains("client_side:required") && q.contains("client_side:optional") && q.contains("index=follows") && q.contains("offset=24") && q.contains("limit=15"), "{q}");
    assert!(!q.contains("versions:") && !q.contains("bad tag") && !q.contains("../x"), "{q}");

    // install with its required dependency (the optional one is skipped)
    let versions = modrinth::project_versions(&c, "sodium", "mods", "1.21.1", Some(Loader::Fabric)).await.unwrap();
    let added = modrinth::install_with_dependencies(&c, &inst, "mods", &versions[0], "Sodium", None, &noop()).await.unwrap();
    assert_eq!(added.len(), 2, "{added:?}");
    let mods_dir = c.game_dir(&inst.id).join("mods");
    assert!(mods_dir.join("sodium-1.0.jar").exists() && mods_dir.join("fabric-api.jar").exists());
    let list = content::list_files(&c, &inst.id, "mods").await.unwrap();
    assert_eq!(list.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["Fabric API", "Sodium"]);
    assert_eq!(list.iter().find(|f| f.title == "Sodium").unwrap().project_id.as_deref(), Some("sodium"));

    // update check: sodium has 2.0, fabric api is current
    let updates = modrinth::check_updates(&c, &inst, "mods").await.unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!((updates[0].filename.as_str(), updates[0].new_version.version_number.as_str(), updates[0].current_version.as_str()), ("sodium-1.0.jar", "2.0", "1.0"));
    let new_name = modrinth::apply_update(&c, &inst, "mods", "sodium-1.0.jar", "sodium-v2").await.unwrap();
    assert_eq!(new_name, "sodium-2.0.jar");
    assert!(mods_dir.join("sodium-2.0.jar").exists() && !mods_dir.join("sodium-1.0.jar").exists());
    assert!(modrinth::check_updates(&c, &inst, "mods").await.unwrap().is_empty(), "nothing left to update");

    // a hand-added jar gets recognised by hash
    std::fs::write(mods_dir.join("whatever.jar"), &manual).unwrap();
    modrinth::identify_files(&c, &inst, "mods").await.unwrap();
    let list = content::list_files(&c, &inst.id, "mods").await.unwrap();
    assert_eq!(list.iter().find(|f| f.name == "whatever.jar").unwrap().title, "Hand Added Pretty");

    // the install preview: what is needed, what is already there, and what could go wrong
    let needy = modrinth::get_version(&c, "needy-v1").await.unwrap();
    let plan = modrinth::plan_install(&c, &inst, "mods", &needy, "Needy").await.unwrap();
    assert_eq!(plan.items.len(), 1, "no installable dependency was found: {plan:?}");
    assert_eq!(plan.already_installed, ["Fabric API"]);
    assert!(plan.warnings.iter().any(|w| w.contains("Ghost Lib") && w.contains("no version")), "{:?}", plan.warnings);
    assert!(plan.warnings.iter().any(|w| w.contains("Hand Added Pretty") && w.contains("does not work")), "{:?}", plan.warnings);
    // a mod for another Minecraft version, and a mod in an instance without a loader, are flagged
    let old = instances::create_instance(&c, NewInstance { name: "Old".into(), mc_version: "1.20.1".into(), loader: Loader::Vanilla, loader_version: String::new(), group: String::new(), icon: String::new() }).await.unwrap();
    let p2 = modrinth::plan_install(&c, &old, "mods", &needy, "Needy").await.unwrap();
    assert!(p2.warnings.iter().any(|w| w.contains("Minecraft 1.21.1") && w.contains("1.20.1")), "{:?}", p2.warnings);
    assert!(p2.warnings.iter().any(|w| w.contains("no mod loader")), "{:?}", p2.warnings);

    // removing a mod also forgets its metadata
    content::delete_file(&c, &inst.id, "mods", "whatever.jar").await.unwrap();
    assert!(!modrinth::load_meta(&c, &inst.id).await.contains_key("mods/whatever.jar"));

    // unsafe file names from the API are refused
    assert!(modrinth::safe_filename("../../evil.jar").unwrap() == "evil.jar");
    assert!(modrinth::safe_filename("..").is_err());
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn mrpack_import_and_instance_export_import_roundtrip() {
    let mod_a = fabric_jar("Pack Mod", "1");
    let server_files = Arc::new(Mutex::new(HashMap::from([("/files/packmod.jar".to_string(), mod_a.clone())])));
    let sf = server_files.clone();
    let server = common::start(move |req, _| match sf.lock().unwrap().get(&req.path) {
        Some(b) => (200, b.clone()),
        None => (404, b"no".to_vec()),
    });
    let c = ctx();

    let index = |hash: &str, path: &str| {
        json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Cool Pack",
            "files": [
                { "path": path, "hashes": { "sha1": hash }, "downloads": [format!("{}/files/missing.jar", server.base), format!("{}/files/packmod.jar", server.base)], "fileSize": mod_a.len(), "env": { "client": "required", "server": "required" } },
                { "path": "mods/server-only.jar", "hashes": { "sha1": "00" }, "downloads": ["http://127.0.0.1:1/x"], "env": { "client": "unsupported", "server": "required" } }
            ],
            "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.9" }
        })
    };
    let pack_path = std::env::temp_dir().join(format!("pack-{}.mrpack", now_ms()));
    std::fs::write(
        &pack_path,
        zip_of(&[
            ("modrinth.index.json", serde_json::to_vec(&index(&sha1_hex(&mod_a), "mods/packmod.jar")).unwrap().as_slice()),
            ("overrides/config/pack.toml", b"a=1"),
            ("client-overrides/options.txt", b"fov:70"),
        ]),
    )
    .unwrap();
    let inst = packs::import_mrpack(&c, &pack_path, &noop(), 4).await.unwrap();
    assert_eq!((inst.name.as_str(), inst.mc_version.as_str(), inst.loader, inst.loader_version.as_str(), inst.group.as_str()), ("Cool Pack", "1.21.1", Loader::Fabric, "0.16.9", "Modpacks"));
    let g = c.game_dir(&inst.id);
    assert_eq!(std::fs::read(g.join("mods/packmod.jar")).unwrap(), mod_a, "second mirror used after the first 404");
    assert_eq!(std::fs::read_to_string(g.join("config/pack.toml")).unwrap(), "a=1");
    assert_eq!(std::fs::read_to_string(g.join("options.txt")).unwrap(), "fov:70");
    assert!(!g.join("mods/server-only.jar").exists(), "server-only files are skipped");

    // a broken download removes the half-made instance
    let bad_path = std::env::temp_dir().join(format!("bad-{}.mrpack", now_ms()));
    std::fs::write(&bad_path, zip_of(&[("modrinth.index.json", serde_json::to_vec(&index(&"0".repeat(40), "mods/packmod.jar")).unwrap().as_slice())])).unwrap();
    let before = instances::list_instances(&c).await.unwrap().len();
    assert!(packs::import_mrpack(&c, &bad_path, &noop(), 4).await.is_err());
    assert_eq!(instances::list_instances(&c).await.unwrap().len(), before);

    // a pack that tries to write outside the instance is refused
    let evil = std::env::temp_dir().join(format!("evil-{}.mrpack", now_ms()));
    std::fs::write(&evil, zip_of(&[("modrinth.index.json", serde_json::to_vec(&index(&sha1_hex(&mod_a), "../../escape.jar")).unwrap().as_slice())])).unwrap();
    assert!(packs::import_mrpack(&c, &evil, &noop(), 4).await.is_err());

    // export (without worlds) and import again
    std::fs::create_dir_all(g.join("saves/World1")).unwrap();
    std::fs::write(g.join("saves/World1/level.dat"), b"x").unwrap();
    std::fs::create_dir_all(g.join("logs")).unwrap();
    std::fs::write(g.join("logs/latest.log"), b"x").unwrap();
    instances::update_instance(&c, &inst.id, json!({ "memoryMb": 6144, "notes": "my notes", "preLaunch": "echo hi" })).await.unwrap();
    let zip_path = std::env::temp_dir().join(format!("export-{}.zip", now_ms()));
    packs::export_instance(&c, &inst.id, &zip_path, false, &noop()).await.unwrap();
    let copy = packs::import_file(&c, &zip_path, &noop(), 4).await.unwrap();
    assert_ne!(copy.id, inst.id);
    assert_eq!((copy.name.as_str(), copy.memory_mb, copy.notes.as_str(), copy.pre_launch.as_str()), ("Cool Pack", Some(6144), "my notes", "echo hi"));
    let cg = c.game_dir(&copy.id);
    assert!(cg.join("mods/packmod.jar").exists() && cg.join("config/pack.toml").exists());
    assert!(!cg.join("saves").exists() && !cg.join("logs").exists(), "worlds and logs were left out");
    // with worlds
    packs::export_instance(&c, &inst.id, &zip_path, true, &noop()).await.unwrap();
    let copy2 = packs::import_file(&c, &zip_path, &noop(), 4).await.unwrap();
    assert!(c.game_dir(&copy2.id).join("saves/World1/level.dat").exists());

    // unsupported archives give a clear message
    let cf = std::env::temp_dir().join(format!("cf-{}.zip", now_ms()));
    std::fs::write(&cf, zip_of(&[("manifest.json", b"{}")])).unwrap();
    let e = packs::import_file(&c, &cf, &noop(), 4).await.unwrap_err().to_string();
    assert!(e.contains("CurseForge"), "{e}");
    assert!(packs::import_file(&c, &std::env::temp_dir().join("nope.zip"), &noop(), 4).await.is_err());
}
