//! The launch pipeline: sign in, find Java, install the loader, download files, build the command line, start the game.

use crate::auth::get_session;
use crate::content::{sync_options, AUTO_SYNC_FILES};
use crate::core::*;
use crate::instances::{get_instance, get_settings, update_instance};
use crate::java::{resolve_java, ResolveOpts};
use crate::loaders::install_loader;
use crate::mojang::*;
use crate::types::Loader;
use crate::util::*;
use anyhow::Result;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};

pub type LogFn = Arc<dyn Fn(String) + Send + Sync>;
pub type ExitFn = Arc<dyn Fn(Option<i32>) + Send + Sync>;

#[derive(Clone)]
pub struct LaunchHooks {
    pub progress: Report,
    pub log: LogFn,
    pub on_exit: ExitFn,
}

pub const LAUNCHER_NAME: &str = "Vox";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Passes at most ~10 progress updates per second (plus every stage change and the final tick) to the UI.
pub fn throttled(report: Report) -> Report {
    let state: Mutex<(String, std::time::Instant)> = Mutex::new((String::new(), std::time::Instant::now() - std::time::Duration::from_secs(1)));
    Arc::new(move |stage, current, total| {
        let mut s = state.lock().unwrap();
        if s.0 != stage || current >= total || s.1.elapsed().as_millis() >= 100 {
            s.0 = stage.to_string();
            s.1 = std::time::Instant::now();
            drop(s);
            report(stage, current, total);
        }
    })
}

/// Handle to a running game.
pub struct Running {
    kill: Arc<tokio::sync::Notify>,
}

impl Running {
    pub fn kill(&self) {
        self.kill.notify_one();
    }
}

/// Downloads everything the instance needs, then starts the game. Returns once the process is running.
pub async fn launch_instance(ctx: &Ctx, id: &str, hooks: LaunchHooks) -> Result<Running> {
    launch_instance_with(ctx, id, hooks, None).await
}

/// Where to drop the player right after the game starts. Overrides the instance's own setting when given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Join {
    /// "host" or "host:port"
    pub server: Option<String>,
    /// A folder name inside the instance's saves folder.
    pub world: Option<String>,
}

/// "play.example.com", "play.example.com:25566" or "[::1]:25565" -> (host, port). Rejects anything odd, so it can never inject extra arguments.
pub fn parse_server(text: &str) -> Option<(String, u16)> {
    let t = text.trim();
    if t.is_empty() || t.len() > 255 || !t.chars().all(|c| c.is_ascii_alphanumeric() || ".-_:[]".contains(c)) {
        return None;
    }
    let (host, port) = if let Some(rest) = t.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        (h.to_string(), tail.strip_prefix(':').map(|p| p.to_string()))
    } else if t.matches(':').count() == 1 {
        let (h, p) = t.split_once(':')?;
        (h.to_string(), Some(p.to_string()))
    } else if t.contains(':') {
        return None; // a bare IPv6 address is ambiguous; it needs [brackets]
    } else {
        (t.to_string(), None)
    };
    if host.is_empty() || host.starts_with('-') {
        return None;
    }
    let port = match port {
        Some(p) => p.parse::<u16>().ok().filter(|p| *p > 0)?,
        None => 25565,
    };
    Some((host, port))
}

pub async fn launch_instance_with(ctx: &Ctx, id: &str, hooks: LaunchHooks, join: Option<Join>) -> Result<Running> {
    let mut inst = get_instance(ctx, id).await?;
    let settings = get_settings(ctx).await;
    let report = throttled(hooks.progress.clone());

    // Sign in first: with no account, say so right away instead of after a large download.
    report("Signing in", 0, 1);
    let session = get_session(ctx).await?;

    // 1. Java (needed first: Forge/NeoForge installers are Java programs).
    report("Preparing Minecraft", 0, 1);
    let base = resolve_version(ctx, &inst.mc_version).await?;
    let user_java = if inst.java_path.is_empty() { settings.java_path.clone() } else { inst.java_path.clone() };
    let mut java = resolve_java(
        ctx,
        base.java_version.as_ref().map(|j| j.major_version).unwrap_or(8),
        ResolveOpts { user_path: &user_java, auto_download: settings.auto_download_java, report: &report },
    )
    .await?;

    // 2. Mod loader.
    let loader_ready = !inst.version_id.is_empty() && tokio::fs::try_exists(ctx.version_json_path(&inst.version_id)).await.unwrap_or(false);
    if inst.loader != Loader::Vanilla && !loader_ready {
        let version_id = install_loader(ctx, &inst, &java.path, &report).await?;
        inst = update_instance(ctx, id, serde_json::json!({ "versionId": version_id })).await?;
    }
    let version_id = if inst.loader == Loader::Vanilla { inst.mc_version.clone() } else { inst.version_id.clone() };
    let resolved = resolve_version(ctx, &version_id).await?;

    // The loader may need a newer Java than vanilla did.
    let needed = resolved.java_version.as_ref().map(|j| j.major_version).unwrap_or(8);
    if user_java.is_empty() && java.major < needed {
        java = resolve_java(ctx, needed, ResolveOpts { user_path: "", auto_download: settings.auto_download_java, report: &report }).await?;
    }

    // 3. Game files.
    let plan = ensure_game(ctx, &resolved, &report, inst.verify_next.unwrap_or(false), settings.download_threads as usize).await?;

    // 4. Directories and natives.
    let game_directory = ctx.game_dir(id);
    tokio::fs::create_dir_all(&game_directory).await?;
    let natives_directory = ctx.natives_dir(id);
    extract_natives(&plan, &natives_directory).await?;
    let assets_root = materialize_legacy_assets(ctx, &plan, &game_directory).await?;

    // Option sync (keybinds, video settings...) from another install.
    if !inst.sync_options_from.is_empty() {
        let files: Vec<String> = AUTO_SYNC_FILES.iter().map(|f| f.to_string()).collect();
        match sync_options(ctx, id, &inst.sync_options_from, &files).await {
            Ok(c) if !c.is_empty() => (hooks.log)(format!("Synced {} from {}.", c.join(", "), if inst.sync_options_from == "mc" { ".minecraft" } else { "another instance" })),
            Ok(_) => {}
            Err(e) => (hooks.log)(format!("Option sync skipped: {e}")),
        }
    }

    // 5. Command line.
    let sep = if cfg!(windows) { ";" } else { ":" };
    let width = inst.width.filter(|w| *w > 0).or(Some(settings.default_width).filter(|w| *w > 0));
    let height = inst.height.filter(|h| *h > 0).or(Some(settings.default_height).filter(|h| *h > 0));
    let fullscreen = inst.fullscreen.unwrap_or(settings.default_fullscreen);
    let has_resolution = width.is_some() && height.is_some() && !fullscreen;
    let s = |p: &std::path::Path| p.to_string_lossy().to_string();
    let vars: Vars = HashMap::from([
        ("auth_player_name", session.name.clone()),
        ("auth_uuid", session.uuid.clone()),
        ("auth_access_token", session.access_token.clone()),
        ("auth_session", format!("token:{}:{}", session.access_token, session.uuid)),
        ("auth_xuid", String::new()),
        ("clientid", String::new()),
        ("user_type", session.user_type.to_string()),
        ("user_properties", "{}".to_string()),
        ("version_name", version_id.clone()),
        ("version_type", resolved.kind.clone().unwrap_or_else(|| "release".into())),
        ("game_directory", s(&game_directory)),
        ("assets_root", s(&assets_root)),
        ("game_assets", s(&assets_root)),
        ("assets_index_name", plan.asset_index_id.clone()),
        ("natives_directory", s(&natives_directory)),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", LAUNCHER_VERSION.to_string()),
        ("classpath", plan.classpath.iter().map(|p| s(p)).collect::<Vec<_>>().join(sep)),
        ("classpath_separator", sep.to_string()),
        ("library_directory", s(&ctx.libraries_dir())),
        ("resolution_width", width.map(|w| w.to_string()).unwrap_or_default()),
        ("resolution_height", height.map(|h| h.to_string()).unwrap_or_default()),
    ])
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    // Quick play: join a server or open a world straight away.
    let join = join.unwrap_or_else(|| Join {
        server: (!inst.join_server.trim().is_empty()).then(|| inst.join_server.clone()),
        world: (!inst.join_world.trim().is_empty()).then(|| inst.join_world.clone()),
    });
    let server = join.server.as_deref().and_then(|t| {
        let p = parse_server(t);
        if p.is_none() {
            (hooks.log)(format!("Ignoring the server address \"{t}\": it is not a valid address."));
        }
        p
    });
    let world = match (&server, join.world.as_deref()) {
        (None, Some(w)) => {
            let ok = !w.is_empty() && !w.contains(['/', '\\']) && w != ".." && w != "." && game_directory.join("saves").join(w).is_dir();
            if !ok {
                (hooks.log)(format!("Ignoring the world \"{w}\": it was not found in this instance."));
            }
            ok.then(|| w.to_string())
        }
        _ => None,
    };
    let mut vars = vars;
    let mut features = HashMap::from([("has_custom_resolution".to_string(), has_resolution)]);
    if let Some((h, p)) = &server {
        features.insert("is_quick_play_multiplayer".into(), true);
        vars.insert("quickPlayMultiplayer".into(), format!("{h}:{p}"));
    }
    if let Some(w) = &world {
        features.insert("is_quick_play_singleplayer".into(), true);
        vars.insert("quickPlaySingleplayer".into(), w.clone());
    }
    let (jvm, mut game) = build_args(&resolved, &vars, &features, &current_platform());
    if let Some((h, p)) = &server {
        if !game.iter().any(|a| a == "--quickPlayMultiplayer") {
            // Minecraft before 1.20 has no quick play; it takes --server and --port instead.
            game.extend(["--server".into(), h.clone(), "--port".into(), p.to_string()]);
        }
        (hooks.log)(format!("Joining {h}:{p} after the game starts."));
    }
    if let Some(w) = &world {
        if game.iter().any(|a| a == "--quickPlaySingleplayer") {
            (hooks.log)(format!("Opening the world \"{w}\" after the game starts."));
        } else {
            (hooks.log)("This Minecraft version cannot open a world directly (1.20 or newer is needed). Starting normally.".to_string());
        }
    }

    let max_mem = inst.memory_mb.unwrap_or(settings.default_memory_mb);
    let min_mem = inst.min_memory_mb.unwrap_or(settings.default_min_memory_mb).min(max_mem);
    let mut args: Vec<String> = vec![format!("-Xmx{max_mem}M"), format!("-Xms{min_mem}M")];
    args.extend(plan.log_arg.clone());
    args.extend(split_args(&settings.jvm_args));
    args.extend(split_args(&inst.jvm_args));
    args.extend(jvm);
    args.push(resolved.main_class.clone());
    args.extend(game);
    if fullscreen {
        args.push("--fullscreen".into());
    }
    args.extend(split_args(&inst.extra_game_args));

    let redact = |line: &str| -> String {
        if session.offline { line.to_string() } else { line.replace(&session.access_token, "***") }
    };
    let printable = args.iter().map(|a| redact(a)).collect::<Vec<_>>().join(" ");
    if session.offline {
        (hooks.log)(format!("Offline mode: playing as {} (singleplayer, LAN and offline-mode servers only).", session.name));
    }
    (hooks.log)(format!("Using Java {}: {}", java.major, java.path.display()));
    (hooks.log)(format!("Launching: {} {}", java.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), printable));

    // Wrapper command (for example "gamemoderun" or "mangohud"): it starts first and receives Java as its arguments.
    let wrapper = split_args(if inst.wrapper.trim().is_empty() { &settings.wrapper } else { &inst.wrapper });
    let (program, full_args): (std::path::PathBuf, Vec<String>) = match wrapper.split_first() {
        Some((w, rest)) => {
            let mut a: Vec<String> = rest.to_vec();
            a.push(java.path.to_string_lossy().to_string());
            a.extend(args.clone());
            ((*w).clone().into(), a)
        }
        None => (java.path.clone(), args.clone()),
    };
    let env = parse_env(&format!("{}\n{}", settings.env_vars, inst.env_vars));
    let inst_env: Vec<(String, String)> = vec![
        ("INST_ID".into(), inst.id.clone()),
        ("INST_NAME".into(), inst.name.clone()),
        ("INST_DIR".into(), s(&ctx.instance_dir(id))),
        ("INST_MC_DIR".into(), s(&game_directory)),
        ("INST_MC_VER".into(), inst.mc_version.clone()),
        ("INST_LOADER".into(), inst.loader.as_str().to_string()),
    ];

    let pre = if inst.pre_launch.trim().is_empty() { settings.pre_launch.clone() } else { inst.pre_launch.clone() };
    if !pre.trim().is_empty() {
        (hooks.log)(format!("Running pre-launch command: {pre}"));
        let code = run_shell(&pre, &game_directory, &inst_env, &env).await?;
        if code != Some(0) {
            anyhow::bail!("The pre-launch command failed (exit code {}).", code.map(|c| c.to_string()).unwrap_or("?".into()));
        }
    }

    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&full_args).current_dir(&game_directory).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null()).kill_on_drop(true);
    cmd.envs(inst_env.iter().cloned()).envs(env.iter().cloned());
    hide_window(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            (hooks.log)(format!("Failed to start Java: {e}"));
            (hooks.on_exit)(None);
            return Err(anyhow::anyhow!("Failed to start Java: {e}"));
        }
    };

    let token = session.access_token.clone();
    let offline = session.offline;
    for stream in [child.stdout.take().map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Send + Unpin>), child.stderr.take().map(|e| Box::new(e) as _)]
        .into_iter()
        .flatten()
    {
        let (log, token) = (hooks.log.clone(), token.clone());
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    log(if offline { line } else { line.replace(&token, "***") });
                }
            }
        });
    }

    let kill = Arc::new(tokio::sync::Notify::new());
    let kill2 = kill.clone();
    let on_exit = hooks.on_exit.clone();
    let post = if inst.post_exit.trim().is_empty() { settings.post_exit.clone() } else { inst.post_exit.clone() };
    let (ctx2, id2, log2) = (ctx.clone(), id.to_string(), hooks.log.clone());
    let started = std::time::Instant::now();
    let (post_dir, post_inst_env, post_env) = (game_directory.clone(), inst_env.clone(), env.clone());
    tokio::spawn(async move {
        let code = tokio::select! {
            r = child.wait() => r.ok().and_then(|s| s.code()),
            _ = kill2.notified() => {
                let _ = child.kill().await;
                child.wait().await.ok().and_then(|s| s.code())
            }
        };
        if let Ok(cur) = get_instance(&ctx2, &id2).await {
            let total = cur.played_ms + started.elapsed().as_millis() as u64;
            let _ = update_instance(&ctx2, &id2, serde_json::json!({ "playedMs": total })).await;
        }
        if !post.trim().is_empty() {
            log2(format!("Running post-exit command: {post}"));
            let _ = run_shell(&post, &post_dir, &post_inst_env, &post_env).await;
        }
        on_exit(code);
    });

    update_instance(ctx, id, serde_json::json!({ "lastPlayed": now_ms(), "verifyNext": false })).await?;
    Ok(Running { kill })
}

/// Lines of KEY=VALUE (blank lines and # comments ignored).
pub fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .filter(|(k, _)| !k.is_empty() && !k.contains(' '))
        .collect()
}

/// Runs a command line through the system shell and waits for it.
pub async fn run_shell(command: &str, cwd: &std::path::Path, a: &[(String, String)], b: &[(String, String)]) -> Result<Option<i32>> {
    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(cwd).envs(a.iter().cloned()).envs(b.iter().cloned()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
    let status = tokio::time::timeout(std::time::Duration::from_secs(300), cmd.status()).await.map_err(|_| anyhow::anyhow!("The command took too long (over 5 minutes)."))??;
    Ok(status.code())
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_addresses_are_parsed_strictly() {
        assert_eq!(parse_server("play.example.com"), Some(("play.example.com".into(), 25565)));
        assert_eq!(parse_server(" play.example.com:25566 "), Some(("play.example.com".into(), 25566)));
        assert_eq!(parse_server("[::1]:30000"), Some(("::1".into(), 30000)));
        assert_eq!(parse_server("[::1]"), Some(("::1".into(), 25565)));
        for bad in ["", "host:0", "host:99999", "host:abc", "-evil", "a b", "host --demo", "x;y", "a:b:c"] {
            assert_eq!(parse_server(bad), None, "{bad}");
        }
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn env_lines_are_parsed() {
        let e = parse_env("# comment\nFOO=bar\n  BAZ = a=b \n\nbad line=x\nNOEQUALS");
        assert_eq!(e, vec![("FOO".to_string(), "bar".to_string()), ("BAZ".to_string(), "a=b".to_string())]);
    }

    #[tokio::test]
    async fn shell_commands_run_with_env_and_report_exit_codes() {
        let dir = std::env::temp_dir();
        let env = vec![("VOX_TEST".to_string(), "42".to_string())];
        let ok = run_shell(if cfg!(windows) { "if \"%VOX_TEST%\"==\"42\" (exit 0) else (exit 3)" } else { "[ \"$VOX_TEST\" = 42 ]" }, &dir, &env, &[]).await.unwrap();
        assert_eq!(ok, Some(0));
        let bad = run_shell(if cfg!(windows) { "exit 7" } else { "exit 7" }, &dir, &[], &[]).await.unwrap();
        assert_eq!(bad, Some(7));
    }

    #[test]
    fn progress_is_throttled_but_never_loses_stage_changes_or_the_end() {
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        let t = throttled(Arc::new(move |_, _, _| {
            n2.fetch_add(1, Ordering::SeqCst);
        }));
        for i in 0..1000 {
            t("A", i, 1000);
        }
        let after_burst = n.load(Ordering::SeqCst);
        assert!(after_burst < 20, "burst of 1000 ticks passed {after_burst}");
        t("B", 1, 10); // new stage
        assert_eq!(n.load(Ordering::SeqCst), after_burst + 1);
        t("B", 10, 10); // final tick
        assert_eq!(n.load(Ordering::SeqCst), after_burst + 2);
    }
}
