//! Crash helper: turns a crash report / game log into plain-English findings with a suggested fix.
//! It only matches messages whose wording is stable across versions; anything else falls back to a generic hint.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct FixAction {
    /// "memory" | "verify" | "mods" | "java" | "account" | "drivers"
    pub kind: String,
    pub label: String,
    /// For "memory": the suggested maximum in MB.
    pub value: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub id: String,
    pub title: String,
    pub explanation: String,
    pub fix: String,
    /// Lines from the report that back the finding (mod names, missing requirements, ...).
    pub evidence: Vec<String>,
    pub action: Option<FixAction>,
}

#[derive(Clone, Debug, Default)]
pub struct Env {
    pub mc_version: String,
    pub loader: String,
    pub memory_mb: u32,
}

fn f(id: &str, title: &str, explanation: &str, fix: &str, evidence: Vec<String>, action: Option<FixAction>) -> Finding {
    Finding { id: id.into(), title: title.into(), explanation: explanation.into(), fix: fix.into(), evidence, action }
}

fn act(kind: &str, label: &str, value: Option<u32>) -> Option<FixAction> {
    Some(FixAction { kind: kind.into(), label: label.into(), value })
}

fn lines_with<'a>(text: &'a str, needles: &[&str], max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in text.lines() {
        let t = l.trim();
        if needles.iter().any(|n| t.contains(n)) {
            let t: String = t.chars().take(220).collect();
            if !out.contains(&t) {
                out.push(t);
            }
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

fn java_for_class_version(v: u32) -> Option<u32> {
    // class file major version -> Java release (52 = Java 8)
    (v >= 52).then(|| v - 44)
}

fn suggested_memory(current: u32) -> u32 {
    let base = if current == 0 { 2048 } else { current };
    let bumped = (base + base / 2 + 511) / 512 * 512;
    bumped.clamp(4096, 16384).max(base.min(16384))
}

/// Value following `marker` up to the next non-token character.
fn after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let i = text.find(marker)? + marker.len();
    let rest = &text[i..];
    let end = rest.find(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == '(').unwrap_or(rest.len());
    Some(&rest[..end])
}

pub fn analyze(text: &str, env: &Env, exit_code: Option<i32>) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();

    // ---- memory -------------------------------------------------------------------------------------
    if text.contains("java.lang.OutOfMemoryError: Java heap space") || text.contains("GC overhead limit exceeded") || text.contains("OutOfMemoryError: Metaspace") {
        let to = suggested_memory(env.memory_mb);
        out.push(f(
            "oom", "The game ran out of memory",
            &format!("Minecraft was given {} MB and used all of it. Large modpacks, shaders and high render distance need more.", if env.memory_mb == 0 { 2048 } else { env.memory_mb }),
            &format!("Give this instance more memory (about {to} MB), and lower render distance if it keeps happening."),
            lines_with(text, &["OutOfMemoryError"], 2), act("memory", &format!("Set memory to {to} MB"), Some(to)),
        ));
    }
    if text.contains("Could not reserve enough space") || text.contains("There is insufficient memory for the Java Runtime") || text.contains("unable to create native thread") || text.contains("unable to create new native thread") {
        out.push(f(
            "alloc", "Your PC could not give Java that much memory",
            "Java asked Windows for more memory than is free right now (or more than a 32-bit Java can use).",
            "Close other programs, or lower this instance's maximum memory in its settings.",
            lines_with(text, &["Could not reserve", "insufficient memory", "native thread"], 2), act("memory", "Set memory to 4096 MB", Some(4096)),
        ));
    }

    // ---- java version ---------------------------------------------------------------------------------
    if text.contains("UnsupportedClassVersionError") || text.contains("compiled by a more recent version of the Java Runtime") {
        let need = after(text, "class file version ").and_then(|v| v.split('.').next()).and_then(|v| v.parse::<u32>().ok()).and_then(java_for_class_version);
        out.push(f(
            "java-version", "The wrong Java version is being used",
            &match need {
                Some(n) => format!("A part of the game or a mod needs Java {n} or newer, but an older Java started it."),
                None => "A part of the game or a mod needs a newer Java than the one that started it.".into(),
            },
            "Clear the Java path (in the instance and in Settings → Java) so Vox downloads the right Java automatically.",
            lines_with(text, &["UnsupportedClassVersionError"], 1), act("java", "Open Java settings", None),
        ));
    }

    // ---- mod problems -----------------------------------------------------------------------------------
    let missing_req = lines_with(text, &["which is missing", "but only the wrong version", "requires any version of", "requires version", "Missing or unsupported mandatory dependencies", "Requested by:"], 6);
    if text.contains("Incompatible mod set") || text.contains("Mod resolution failed") || text.contains("Missing or unsupported mandatory dependencies") || text.contains("Could not find required mod") {
        let wrong_mc = missing_req.iter().any(|l| l.contains("of minecraft") || l.contains("minecraft ("));
        out.push(f(
            "mod-deps", if wrong_mc { "A mod is for a different Minecraft version" } else { "A mod is missing something it needs" },
            if wrong_mc { "At least one mod was built for another Minecraft version than this instance uses." } else { "A mod needs another mod (or a different version of it) that is not installed." },
            "Install the missing mod or update the listed mods. The Mods tab can check for updates. If a mod does not support this Minecraft version, remove it.",
            missing_req.clone(), act("mods", "Open the Mods tab", None),
        ));
    }
    if text.contains("DuplicateModsFoundException") || text.contains("Found duplicate mods") || text.contains("Duplicate mod") {
        out.push(f("duplicate-mods", "The same mod is installed twice", "Two files in the mods folder provide the same mod, which the game does not allow.", "Open the Mods tab and delete the older copy.",
            lines_with(text, &["uplicate"], 4), act("mods", "Open the Mods tab", None)));
    }
    if let Some(m) = after(text, "Mixin apply for mod ") {
        out.push(f("mixin", &format!("The mod “{m}” failed to patch the game"), "This usually means the mod does not match this Minecraft version or clashes with another mod.",
            "Update that mod, or disable it (and any mod that depends on it).", lines_with(text, &["Mixin apply", "MixinApplyError", "InvalidMixinException"], 3), act("mods", "Open the Mods tab", None)));
    } else if text.contains("MixinApplyError") || text.contains("InvalidMixinException") || text.contains("Mixin transformation") {
        out.push(f("mixin", "A mod failed to patch the game", "A mod's changes to Minecraft could not be applied. This usually means a version mismatch or two mods clashing.",
            "Update your mods, or remove recently added ones one at a time.", lines_with(text, &["Mixin", "mixin"], 3), act("mods", "Open the Mods tab", None)));
    }
    if text.contains("NoSuchMethodError") || text.contains("NoSuchFieldError") || text.contains("NoClassDefFoundError") || text.contains("ClassNotFoundException") {
        if !out.iter().any(|x| x.id == "mod-deps" || x.id == "mixin") {
            out.push(f("mod-mismatch", "A mod does not match the rest of your setup", "The game looked for code that is not there, which normally means a mod (or its library) is the wrong version.",
                "Update your mods, make sure every mod's required libraries (such as Fabric API) are installed, or remove recent additions.",
                lines_with(text, &["NoSuchMethodError", "NoSuchFieldError", "NoClassDefFoundError", "ClassNotFoundException"], 3), act("mods", "Open the Mods tab", None)));
        }
    }
    let suspected = lines_with(text, &["Suspected Mod"], 3);
    if !suspected.is_empty() && !suspected.iter().all(|l| l.contains("Unknown") || l.ends_with("NONE")) {
        out.push(f("suspected", "The crash report names the likely mod", "Minecraft's own crash analysis points at these mods as the probable cause.",
            "Update or disable the listed mod and try again.", suspected, act("mods", "Open the Mods tab", None)));
    }
    if text.contains("Ticking entity") || text.contains("Ticking block entity") || text.contains("Exception ticking world") {
        let ty = after(text, "Entity Type: ").map(|s| s.to_string());
        out.push(f("ticking", "The game crashed while updating something in your world",
            &match &ty { Some(t) => format!("An entity or block ({t}) caused an error while the world was running. It is often from a mod."), None => "An entity or block caused an error while the world was running. It is often from a mod.".into() },
            "Back up the world first. Then update or remove the mod that adds that item. Check the Mods tab.", lines_with(text, &["Entity Type:", "Block: Block{", "Ticking"], 3), act("mods", "Open the Mods tab", None)));
    }

    // ---- graphics and native crashes ---------------------------------------------------------------------------
    if text.contains("GLFW error 65542") || text.contains("The driver does not appear to support OpenGL") || text.contains("Pixel format not accelerated") || text.contains("GLFW error 65543") || text.contains("WGL: Failed to make context current") {
        out.push(f("opengl", "Your graphics driver could not start the game window", "Minecraft needs OpenGL, and the graphics driver did not provide it. This often happens with outdated drivers, remote desktop, or a laptop using its weak graphics chip.",
            "Update your graphics driver from NVIDIA, AMD or Intel. On a laptop, set javaw.exe to use the high-performance GPU in Windows Graphics settings.",
            lines_with(text, &["GLFW error", "OpenGL", "Pixel format"], 3), act("drivers", "How to update", None)));
    }
    if text.contains("A fatal error has been detected by the Java Runtime Environment") || text.contains("EXCEPTION_ACCESS_VIOLATION") || matches!(exit_code, Some(-1073741819) | Some(-1073740791) | Some(-805306369)) {
        let frame = lines_with(text, &["Problematic frame", "# C  ["], 2);
        let joined = frame.join(" ").to_lowercase();
        let hint = if joined.contains("nvoglv") || joined.contains("nvwgf") || joined.contains("nvd3d") { " The crash happened inside the NVIDIA driver, so updating it is the first thing to try." }
            else if joined.contains("atio") || joined.contains("amdxx") || joined.contains("aticfx") { " The crash happened inside the AMD driver, so updating it is the first thing to try." }
            else if joined.contains("ig9") || joined.contains("igd") || joined.contains("ig7") { " The crash happened inside the Intel graphics driver, so updating it is the first thing to try." }
            else { "" };
        out.push(f("native-crash", "Java itself crashed",
            &format!("This is a crash below Minecraft, in Java or a system library, not a normal game error.{hint}"),
            "Update your graphics driver, turn off screen-capture or overlay software (Discord, Nahimic, MSI Afterburner overlays), and try again. If it repeats, lower memory or try a different Java in Settings.",
            frame, act("drivers", "How to update", None)));
    }
    if text.contains("UnsatisfiedLinkError") && (text.contains("lwjgl") || text.contains("LWJGL")) {
        out.push(f("natives", "A game library failed to load", "Minecraft's native libraries (LWJGL) are missing or damaged, often after an interrupted download or antivirus cleanup.",
            "Verify the game files. Vox will re-download anything missing on the next launch.", lines_with(text, &["UnsatisfiedLinkError"], 2), act("verify", "Verify files on next launch", None)));
    }

    // ---- damaged files / account ----------------------------------------------------------------------------------
    if text.contains("Could not find or load main class") || text.contains("ZipException") || text.contains("zip END header not found") || text.contains("invalid LOC header") || text.contains("Invalid or corrupt jarfile") {
        out.push(f("corrupt", "A game or mod file is damaged", "A downloaded file is incomplete or corrupt.",
            "Verify the game files. If a specific mod is named, delete and re-install it.", lines_with(text, &["ZipException", "END header", "LOC header", "corrupt", "main class"], 3), act("verify", "Verify files on next launch", None)));
    }
    if text.contains("Invalid session") || text.contains("Failed to verify username") || text.contains("Authentication servers are down") || text.contains("401 Unauthorized") {
        out.push(f("session", "Your Minecraft login was rejected", "The login session is out of date, or you joined an online server with an offline account.",
            "Sign out and sign in again under Accounts. Offline accounts cannot join online-mode servers.", lines_with(text, &["Invalid session", "Failed to verify", "Authentication servers", "401"], 2), act("account", "Open Accounts", None)));
    }

    if out.is_empty() {
        let desc = lines_with(text, &["Description:"], 1);
        let mut ev = desc;
        ev.extend(lines_with(text, &["Exception in thread", "Caused by:"], 3));
        out.push(f("unknown", "Vox does not recognise this crash",
            &match exit_code { Some(c) if c != 0 => format!("The game stopped with exit code {c}, and the report has no known cause."), _ => "The report has no cause Vox knows about.".into() },
            "Try: verify the game files, then disable mods you added recently, then try a lower memory or a fresh instance. Copy the report to share it when asking for help.",
            ev, act("verify", "Verify files on next launch", None)));
    }
    out
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Where the text came from, e.g. "crash-reports/crash-2026-11-05.txt".
    pub source: String,
    pub findings: Vec<Finding>,
    /// The text that was analysed (trimmed), for "copy report".
    pub text: String,
}

fn newest(dir: &std::path::Path, ok: impl Fn(&str) -> bool, max_age_ms: u128) -> Option<std::path::PathBuf> {
    let now = std::time::SystemTime::now();
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        let m = e.metadata().ok()?;
        let age = now.duration_since(m.modified().ok()?).ok()?.as_millis();
        (ok(&n) && m.is_file() && age <= max_age_ms).then(|| (m.modified().ok(), e.path()))
    }).max_by_key(|(t, _)| *t).map(|(_, p)| p)
}

/// Gathers the freshest crash evidence for an instance and analyses it. `extra` is the end of the live console.
pub fn gather(game_dir: &std::path::Path, extra: &str, env: &Env, exit_code: Option<i32>) -> Report {
    const RECENT: u128 = 20 * 60 * 1000;
    let mut parts: Vec<String> = Vec::new();
    let mut source = String::from("console output");
    let read = |p: &std::path::Path| std::fs::read_to_string(p).unwrap_or_default();
    if let Some(p) = newest(&game_dir.join("crash-reports"), |n| n.ends_with(".txt"), RECENT) {
        source = format!("crash-reports/{}", p.file_name().unwrap_or_default().to_string_lossy());
        parts.push(read(&p));
    }
    if let Some(p) = newest(game_dir, |n| n.starts_with("hs_err_pid") && n.ends_with(".log"), RECENT) {
        if parts.is_empty() {
            source = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        }
        parts.push(read(&p));
    }
    let latest = read(&game_dir.join("logs").join("latest.log"));
    let tail: Vec<&str> = latest.lines().rev().take(600).collect();
    if !tail.is_empty() {
        if parts.is_empty() {
            source = "logs/latest.log".into();
        }
        parts.push(tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
    }
    if !extra.trim().is_empty() {
        parts.push(extra.to_string());
    }
    let text = parts.join("\n");
    let findings = analyze(&text, env, exit_code);
    let trimmed: String = text.chars().take(60_000).collect();
    Report { source, findings, text: trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(t: &str, code: Option<i32>) -> Vec<String> {
        analyze(t, &Env { mc_version: "1.21.4".into(), loader: "fabric".into(), memory_mb: 2048 }, code).into_iter().map(|f| f.id).collect()
    }

    #[test]
    fn memory_is_raised_but_capped() {
        assert_eq!(suggested_memory(2048), 4096);
        assert_eq!(suggested_memory(6144), 9216);
        assert_eq!(suggested_memory(16384), 16384);
        assert_eq!(suggested_memory(0), 4096);
        let f = &analyze("java.lang.OutOfMemoryError: Java heap space", &Env { memory_mb: 6144, ..Default::default() }, None)[0];
        assert_eq!(f.action.as_ref().unwrap().value, Some(9216));
    }

    #[test]
    fn java_version_names_the_needed_release() {
        let t = "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/Foo has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0";
        let r = analyze(t, &Env::default(), None);
        assert_eq!(r[0].id, "java-version");
        assert!(r[0].explanation.contains("Java 21"), "{}", r[0].explanation);
    }

    #[test]
    fn fabric_missing_dependency_is_found() {
        let t = "net.fabricmc.loader.impl.FormattedException: Mod resolution failed\nIncompatible mod set!\n - Mod 'Foo' (foo) 1.0 requires any version of fabric-api, which is missing!";
        let r = analyze(t, &Env::default(), None);
        assert_eq!(r[0].id, "mod-deps");
        assert!(r[0].evidence.iter().any(|l| l.contains("fabric-api")));
        assert_eq!(r[0].title, "A mod is missing something it needs");
    }

    #[test]
    fn wrong_minecraft_version_is_called_out() {
        let r = analyze("Incompatible mod set!\n - Mod 'Foo' (foo) requires version 1.20 of minecraft (minecraft), but only the wrong version is present: 1.21.4!", &Env::default(), None);
        assert_eq!(r[0].title, "A mod is for a different Minecraft version");
    }

    #[test]
    fn mixin_names_the_mod_and_suggested_mods_are_listed() {
        let r = ids("Mixin apply for mod sodium failed sodium.mixins.json:x", None);
        assert!(r.contains(&"mixin".to_string()));
        let r = analyze("Suspected Mods: Lithium (lithium)", &Env::default(), None);
        assert_eq!(r[0].id, "suspected");
        assert_eq!(ids("Suspected Mods: Unknown", None), vec!["unknown"]);
    }

    #[test]
    fn graphics_and_native_crashes() {
        assert!(ids("GLFW error 65542: WGL: The driver does not appear to support OpenGL", None).contains(&"opengl".to_string()));
        let r = analyze("# A fatal error has been detected by the Java Runtime Environment:\n# Problematic frame:\n# C  [nvoglv64.dll+0x1234]", &Env::default(), Some(1));
        let n = r.iter().find(|f| f.id == "native-crash").unwrap();
        assert!(n.explanation.contains("NVIDIA"));
        assert!(ids("", Some(-1073740791)).contains(&"native-crash".to_string()));
    }

    #[test]
    fn unknown_crash_gets_a_generic_answer() {
        let r = analyze("---- Minecraft Crash Report ----\nDescription: Something odd\n", &Env::default(), Some(1));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].id, "unknown");
        assert!(r[0].evidence[0].contains("Something odd"));
        assert!(ids("everything is fine", Some(0)).contains(&"unknown".to_string()));
    }

    #[test]
    fn session_and_corrupt_files() {
        assert!(ids("Invalid session (Try restarting your game)", None).contains(&"session".to_string()));
        assert!(ids("java.util.zip.ZipException: zip END header not found", None).contains(&"corrupt".to_string()));
    }

    #[test]
    fn gather_reads_the_newest_report_and_the_console() {
        let dir = std::env::temp_dir().join(format!("vox-crash-{}-{}", std::process::id(), crate::util::now_ms()));
        std::fs::create_dir_all(dir.join("crash-reports")).unwrap();
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::write(dir.join("crash-reports/crash-1.txt"), "Description: boom\njava.lang.OutOfMemoryError: Java heap space").unwrap();
        std::fs::write(dir.join("logs/latest.log"), "[main/INFO]: hi").unwrap();
        let r = gather(&dir, "Error: Could not find or load main class", &Env { memory_mb: 2048, ..Default::default() }, Some(1));
        assert_eq!(r.source, "crash-reports/crash-1.txt");
        let ids: Vec<_> = r.findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"oom") && ids.contains(&"corrupt"), "{ids:?}");
        // with nothing on disk, the console text alone is analysed
        let empty = std::env::temp_dir().join(format!("vox-crash-empty-{}", crate::util::now_ms()));
        let r = gather(&empty, "Invalid session", &Env::default(), None);
        assert_eq!(r.source, "console output");
        assert_eq!(r.findings[0].id, "session");
    }
}
