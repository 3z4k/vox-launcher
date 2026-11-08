//! End-to-end: a fake Mojang server on localhost plus the machine's real Java run the whole launch pipeline
//! (manifest -> version JSON -> libraries -> natives -> assets -> command line -> a real JVM process).
//! Skipped automatically when no `javac` is installed.

use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use vox_launcher_lib::launch::{launch_instance, LaunchHooks};
use vox_launcher_lib::types::*;
use vox_launcher_lib::util::Ctx;
use vox_launcher_lib::{auth, instances};

fn sha1_hex(b: &[u8]) -> String {
    hex::encode(Sha1::digest(b))
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

struct Server {
    port: u16,
    hits: Arc<AtomicUsize>,
}

fn serve(files: HashMap<String, Vec<u8>>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let (h, files) = (hits.clone(), Arc::new(files));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (h, files) = (h.clone(), files.clone());
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                h.fetch_add(1, Ordering::SeqCst);
                let (status, body) = match files.get(&path) {
                    Some(b) => ("200 OK", b.clone()),
                    None => ("404 Not Found", b"missing".to_vec()),
                };
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            });
        }
    });
    Server { port, hits }
}

fn compile_game() -> Option<Vec<u8>> {
    let dir = std::env::temp_dir().join(format!("vox-e2e-src-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("VoxMain.java"),
        r#"public class VoxMain {
  public static void main(String[] a) {
    System.out.println("VOXGAME args=" + String.join(" ", a));
    System.out.println("VOXGAME natives=" + System.getProperty("java.library.path"));
    System.out.println("VOXGAME cp=" + System.getProperty("java.class.path"));
    System.out.println("VOXGAME xmx=" + (Runtime.getRuntime().maxMemory() > 0));
    System.out.println("VOXGAME prop=" + System.getProperty("vox.test") + " env=" + System.getenv("VOX_TEST_ENV") + " inst=" + System.getenv("INST_NAME"));
  }
}"#,
    )
    .unwrap();
    let ok = Command::new("javac").arg("-d").arg(&dir).arg(dir.join("VoxMain.java")).output().ok()?.status.success();
    if !ok {
        return None;
    }
    let class = std::fs::read(dir.join("VoxMain.class")).ok()?;
    Some(zip_of(&[("VoxMain.class", &class)]))
}

#[tokio::test]
async fn full_launch_against_a_fake_mojang_then_a_warm_relaunch_does_no_network() {
    let Some(client_jar) = compile_game() else {
        eprintln!("skipped: no javac available");
        return;
    };
    let java_bin = String::from_utf8(Command::new("sh").arg("-c").arg("command -v java").output().unwrap().stdout).unwrap().trim().to_string();
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    std::env::set_var("no_proxy", "127.0.0.1,localhost");

    // Build the fake world.
    let lib_jar = zip_of(&[("dummy.txt", b"lib")]);
    let natives_jar = zip_of(&[("libvoxfoo.so", b"native"), ("voxfoo.dll", b"native"), ("META-INF/skip.txt", b"nope")]);
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..60 {
        assets.push((format!("minecraft/sounds/s{i}.ogg"), format!("sound-{i}").into_bytes()));
    }
    let objects: serde_json::Map<String, serde_json::Value> = assets
        .iter()
        .map(|(n, d)| (n.clone(), serde_json::json!({ "hash": sha1_hex(d), "size": d.len() })))
        .collect();
    let index = serde_json::to_vec(&serde_json::json!({ "objects": objects })).unwrap();

    // Server port is needed inside the JSON, so reserve it by binding first through a two-step build.
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let base = format!("http://127.0.0.1:{port}");

    let version = serde_json::json!({
        "id": "9.9-test", "type": "release", "mainClass": "VoxMain",
        "javaVersion": { "majorVersion": 8 },
        "assetIndex": { "id": "t", "sha1": sha1_hex(&index), "size": index.len(), "url": format!("{base}/assets-index.json") },
        "downloads": { "client": { "sha1": sha1_hex(&client_jar), "size": client_jar.len(), "url": format!("{base}/client.jar") } },
        "libraries": [
            { "name": "com.example:lib:1.0", "downloads": { "artifact": { "path": "com/example/lib/1.0/lib-1.0.jar", "sha1": sha1_hex(&lib_jar), "size": lib_jar.len(), "url": format!("{base}/lib.jar") } } },
            { "name": "com.example:lib:1.0:natives-test",
              "downloads": { "artifact": { "path": "com/example/lib/1.0/lib-1.0-natives-test.jar", "sha1": sha1_hex(&natives_jar), "size": natives_jar.len(), "url": format!("{base}/natives.jar") } } }
        ],
        "arguments": {
            "game": ["--username", "${auth_player_name}", "--version", "${version_name}", "--assetsDir", "${assets_root}", "--assetIndex", "${assets_index_name}"],
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        }
    });
    let manifest = serde_json::json!({ "versions": [{ "id": "9.9-test", "type": "release", "releaseTime": "2024-01-01T00:00:00+00:00", "url": format!("{base}/version.json") }] });

    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    files.insert("/manifest.json".into(), serde_json::to_vec(&manifest).unwrap());
    files.insert("/version.json".into(), serde_json::to_vec(&version).unwrap());
    files.insert("/client.jar".into(), client_jar);
    files.insert("/lib.jar".into(), lib_jar);
    files.insert("/natives.jar".into(), natives_jar);
    files.insert("/assets-index.json".into(), index);
    for (_, d) in &assets {
        let h = sha1_hex(d);
        files.insert(format!("/res/{}/{}", &h[..2], h), d.clone());
    }
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    // `serve` binds itself; hand it the port we reserved by closing ours first.
    drop(listener);
    let _ = Server { port: 0, hits: Arc::new(AtomicUsize::new(0)) };
    let server = serve_on(port, files);
    std::env::set_var("VOX_MANIFEST_URL", format!("{base}/manifest.json"));
    std::env::set_var("VOX_RESOURCES_URL", format!("{base}/res"));

    // The launcher side.
    let ctx = Ctx::new(std::env::temp_dir().join(format!("vox-e2e-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&*ctx.root);
    let java_bin_for_settings = java_bin.clone();
    instances::save_settings(&ctx, Settings { java_path: java_bin, ..Default::default() }).await.unwrap();
    auth::add_offline_account(&ctx, "Tester_01").await.unwrap();
    let versions = vox_launcher_lib::mojang::list_versions(&ctx).await.unwrap();
    assert_eq!(versions[0].id, "9.9-test");
    let inst = instances::create_instance(&ctx, NewInstance { name: "E2E".into(), mc_version: "9.9-test".into(), loader: Loader::Vanilla, loader_version: String::new(), group: String::new(), icon: String::new() })
        .await
        .unwrap();

    let run = |label: &'static str| {
        let ctx = ctx.clone();
        let id = inst.id.clone();
        async move {
            let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
            let progress: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
            let (tx, rx) = tokio::sync::oneshot::channel::<Option<i32>>();
            let tx = Mutex::new(Some(tx));
            let (l, p) = (logs.clone(), progress.clone());
            let started = std::time::Instant::now();
            launch_instance(
                &ctx,
                &id,
                LaunchHooks {
                    progress: Arc::new(move |stage, _, _| {
                        let mut p = p.lock().unwrap();
                        if p.last().map(|s| s != stage).unwrap_or(true) {
                            p.push(stage.to_string());
                        }
                    }),
                    log: Arc::new(move |line| l.lock().unwrap().push(line)),
                    on_exit: Arc::new(move |code| {
                        if let Some(tx) = tx.lock().unwrap().take() {
                            let _ = tx.send(code);
                        }
                    }),
                },
            )
            .await
            .unwrap_or_else(|e| panic!("{label}: launch failed: {e:?}"));
            let prepared = started.elapsed();
            let code = tokio::time::timeout(std::time::Duration::from_secs(30), rx).await.expect("game did not exit").unwrap();
            let logs = logs.lock().unwrap().clone();
            let progress = progress.lock().unwrap().clone();
            (code, logs, progress, prepared)
        }
    };

    // --- cold launch ---
    let (code, logs, progress, cold) = run("cold").await;
    let all = logs.join("\n");
    assert_eq!(code, Some(0), "game exit code; logs:\n{all}");
    assert!(all.contains("VOXGAME args=--username Tester_01 --version 9.9-test"), "{all}");
    assert!(all.contains("Offline mode: playing as Tester_01"), "{all}");
    assert!(all.contains("VOXGAME cp=") && all.contains("client.jar") == false && all.contains("9.9-test.jar"), "{all}");
    assert!(progress.iter().any(|s| s == "Downloading libraries") && progress.iter().any(|s| s == "Downloading assets"), "{progress:?}");
    let natives = ctx.natives_dir(&inst.id);
    assert!(natives.join("libvoxfoo.so").exists() && natives.join("voxfoo.dll").exists());
    assert!(!natives.join("skip.txt").exists(), "META-INF is excluded");
    let obj_count = walk_count(&ctx.assets_dir().join("objects"));
    assert_eq!(obj_count, 60, "every asset downloaded");
    let hits_cold = server.hits.load(Ordering::SeqCst);
    assert!(hits_cold >= 60 + 5, "cold launch made {hits_cold} requests");
    assert!(instances::get_instance(&ctx, &inst.id).await.unwrap().last_played.is_some());

    // --- warm launch: nothing may touch the network, and it has to be quick ---
    let (code, logs, _, warm) = run("warm").await;
    assert_eq!(code, Some(0), "{}", logs.join("\n"));
    assert_eq!(server.hits.load(Ordering::SeqCst), hits_cold, "a warm launch must not make any HTTP request");
    println!("cold launch prep: {cold:?}, warm launch prep: {warm:?}, requests cold: {hits_cold}");
    assert!(warm < std::time::Duration::from_millis(500), "warm launch took {warm:?}");

    // --- per-instance settings: JVM args, env, game args, fullscreen, option sync, pre/post commands, playtime ---
    let other = ctx.game_dir("source-inst");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("options.txt"), "fov:99\n").unwrap();
    let marker_pre = ctx.root.join("pre.marker");
    let marker_post = ctx.root.join("post.marker");
    let touch = |p: &std::path::Path| if cfg!(windows) { format!("echo x> \"{}\"", p.display()) } else { format!("touch '{}'", p.display()) };
    instances::save_settings(
        &ctx,
        Settings { java_path: java_bin_for_settings.clone(), jvm_args: "-Dvox.test=from-global".into(), env_vars: "VOX_TEST_ENV=hello".into(), default_min_memory_mb: 256, default_memory_mb: 1024, ..Default::default() },
    )
    .await
    .unwrap();
    instances::update_instance(
        &ctx,
        &inst.id,
        serde_json::json!({
            "jvmArgs": "-Dvox.test=from-instance", "extraGameArgs": "--quickPlayPath saves/x", "fullscreen": true,
            "syncOptionsFrom": "source-inst", "preLaunch": touch(&marker_pre), "postExit": touch(&marker_post), "minMemoryMb": 128, "memoryMb": 512
        }),
    )
    .await
    .unwrap();
    let played_before = instances::get_instance(&ctx, &inst.id).await.unwrap().played_ms;
    let (code, logs, _, _) = run("settings").await;
    let all = logs.join("\n");
    assert_eq!(code, Some(0), "{all}");
    assert!(all.contains("VOXGAME prop=from-instance env=hello inst=E2E"), "instance JVM arg wins over the global one; env vars and INST_* are set:\n{all}");
    assert!(all.contains("-Xmx512M -Xms128M"), "{all}");
    assert!(all.contains("--fullscreen") && all.contains("--quickPlayPath saves/x"), "{all}");
    assert_eq!(std::fs::read_to_string(ctx.game_dir(&inst.id).join("options.txt")).unwrap(), "fov:99\n", "options synced from the other instance");
    assert!(all.contains("Synced options.txt"), "{all}");
    assert!(marker_pre.exists(), "pre-launch command ran");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(marker_post.exists(), "post-exit command ran");
    assert!(instances::get_instance(&ctx, &inst.id).await.unwrap().played_ms > played_before, "playtime is recorded");

    // a failing pre-launch command stops the launch before Java starts
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "preLaunch": "exit 9", "syncOptionsFrom": "", "fullscreen": null })).await.unwrap();
    let r = launch_instance(&ctx, &inst.id, LaunchHooks { progress: Arc::new(|_, _, _| {}), log: Arc::new(|_| {}), on_exit: Arc::new(|_| {}) }).await;
    assert!(r.err().map(|e| e.to_string().contains("pre-launch")).unwrap_or(false));
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "preLaunch": "", "postExit": "", "jvmArgs": "", "extraGameArgs": "", "memoryMb": null, "minMemoryMb": null })).await.unwrap();

    // --- quick join: a server from the instance settings, an unsafe address, and a world ---
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "joinServer": "play.example.com:25566" })).await.unwrap();
    let (code, logs, _, _) = run("join server").await;
    let all = logs.join("\n");
    assert_eq!(code, Some(0), "{all}");
    assert!(all.contains("--server play.example.com --port 25566"), "older versions get --server/--port:\n{all}");
    assert!(all.contains("Joining play.example.com:25566"), "{all}");
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "joinServer": "evil --demo" })).await.unwrap();
    let (_, logs, _, _) = run("join bad address").await;
    let all = logs.join("\n");
    assert!(all.contains("not a valid address") && !all.contains("--server"), "an unsafe address never reaches the command line:\n{all}");
    std::fs::create_dir_all(ctx.game_dir(&inst.id).join("saves/My World")).unwrap();
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "joinServer": "", "joinWorld": "My World" })).await.unwrap();
    let (_, logs, _, _) = run("join world").await;
    let all = logs.join("\n");
    assert!(all.contains("cannot open a world directly"), "this fake version has no quick play support, so it starts normally:\n{all}");
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "joinWorld": "../escape" })).await.unwrap();
    let (_, logs, _, _) = run("join bad world").await;
    assert!(logs.join("\n").contains("Ignoring the world"));
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "joinWorld": "" })).await.unwrap();

    // --- a corrupted asset is repaired only when "verify" is requested ---
    let victim = first_file(&ctx.assets_dir().join("objects"));
    let good = std::fs::read(&victim).unwrap();
    std::fs::write(&victim, vec![b'x'; good.len()]).unwrap(); // same size, wrong content
    let (_, _, _, _) = run("same-size corruption").await;
    assert_ne!(std::fs::read(&victim).unwrap(), good, "size-only trust is the documented fast path");
    instances::update_instance(&ctx, &inst.id, serde_json::json!({ "verifyNext": true })).await.unwrap();
    let (code, _, _, _) = run("verify").await;
    assert_eq!(code, Some(0));
    assert_eq!(std::fs::read(&victim).unwrap(), good, "verify re-downloads the corrupted file");
    assert_eq!(instances::get_instance(&ctx, &inst.id).await.unwrap().verify_next, Some(false));
}

fn serve_on(port: u16, files: HashMap<String, Vec<u8>>) -> Server {
    // Same handler as `serve`, but on a chosen port.
    let listener = loop {
        match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(l) => break l,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };
    let hits = Arc::new(AtomicUsize::new(0));
    let (h, files) = (hits.clone(), Arc::new(files));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (h, files) = (h.clone(), files.clone());
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                h.fetch_add(1, Ordering::SeqCst);
                let (status, body) = match files.get(&path) {
                    Some(b) => ("200 OK", b.clone()),
                    None => ("404 Not Found", b"missing".to_vec()),
                };
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            });
        }
    });
    Server { port, hits }
}

fn walk_count(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| if e.path().is_dir() { walk_count(&e.path()) } else { 1 }).sum())
        .unwrap_or(0)
}

fn first_file(dir: &std::path::Path) -> std::path::PathBuf {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            return first_file(&e.path());
        }
        return e.path();
    }
    panic!("empty");
}

#[allow(dead_code)]
fn unused() {
    let _ = serve(HashMap::new());
}
