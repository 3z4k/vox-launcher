//! Finding a Java, and downloading a Temurin JRE when none fits.

use crate::mojang::Report;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::process::Command;

#[cfg(windows)]
pub const EXE: &str = "java.exe";
#[cfg(not(windows))]
pub const EXE: &str = "java";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub major: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedJava {
    path: PathBuf,
    major: u32,
    mtime: u64,
}

/// Parses `java -version` output: "1.8.0_392" -> 8, "21.0.4" -> 21, "17" -> 17.
pub fn parse_java_major(output: &str) -> Option<u32> {
    let re = regex::Regex::new(r#"version "(\d+)(?:\.(\d+))?"#).unwrap();
    let c = re.captures(output)?;
    let major: u32 = c[1].parse().ok()?;
    if major == 1 {
        Some(c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(8))
    } else {
        Some(major)
    }
}

/// Runs `java -version` and returns the major version, or None if it is not a working Java.
pub async fn java_major(bin: &Path) -> Option<u32> {
    let mut cmd = Command::new(bin);
    cmd.arg("-version").kill_on_drop(true);
    hide_window(&mut cmd);
    let out = tokio::time::timeout(std::time::Duration::from_secs(8), cmd.output()).await.ok()?.ok()?;
    parse_java_major(&format!("{}\n{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout)))
}

async fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            if e.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                out.push(e.path());
            }
        }
    }
    out
}

async fn candidate_paths(ctx: &Ctx) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !found.contains(&p) {
            found.push(p);
        }
    };
    for d in child_dirs(&ctx.runtimes_dir()).await {
        add(d.join("bin").join(EXE));
    }
    if let Some(h) = std::env::var_os("JAVA_HOME") {
        add(PathBuf::from(h).join("bin").join(EXE));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            add(d.join(EXE));
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                for vendor in ["Java", "Eclipse Adoptium", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Programs/Eclipse Adoptium"] {
                    roots.push(base.join(vendor));
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.push("/Library/Java/JavaVirtualMachines".into());
        if let Some(h) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(h).join("Library/Java/JavaVirtualMachines"));
        }
    } else {
        for r in ["/usr/lib/jvm", "/usr/lib64/jvm", "/opt/java", "/opt/jdk"] {
            roots.push(r.into());
        }
        if let Some(h) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(h).join(".sdkman/candidates/java"));
        }
    }
    for r in roots {
        for d in child_dirs(&r).await {
            add(d.join("bin").join(EXE));
            add(d.join("Contents").join("Home").join("bin").join(EXE));
        }
    }
    found
}

fn mtime_of(p: &Path) -> Option<u64> {
    let m = std::fs::metadata(p).ok()?;
    if !m.is_file() {
        return None;
    }
    m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

fn detected() -> &'static Mutex<Option<(Vec<JavaInstall>, bool)>> {
    static D: std::sync::OnceLock<Mutex<Option<(Vec<JavaInstall>, bool)>>> = std::sync::OnceLock::new();
    D.get_or_init(|| Mutex::new(None))
}

async fn scan_javas(ctx: &Ctx) -> Vec<JavaInstall> {
    let mut jobs = Vec::new();
    for p in candidate_paths(ctx).await {
        // skips paths that do not exist without starting a process
        let Some(mtime) = mtime_of(&p) else { continue };
        jobs.push(tokio::spawn(async move { java_major(&p).await.map(|major| CachedJava { path: p, major, mtime }) }));
    }
    let mut results: Vec<CachedJava> = Vec::new();
    for j in jobs {
        if let Ok(Some(c)) = j.await {
            results.push(c);
        }
    }
    let _ = write_json(&ctx.cache_dir().join("javas.json"), &results).await;
    results.into_iter().map(|c| JavaInstall { path: c.path, major: c.major }).collect()
}

/// Finds installed Javas. The result is remembered on disk (checked by modification time), so a normal
/// launch does not start a Java process per candidate. `force` rescans.
pub async fn detect_javas(ctx: &Ctx, force: bool) -> Vec<JavaInstall> {
    if !force {
        if let Some((list, _)) = detected().lock().unwrap().clone() {
            return list;
        }
        let cached: Vec<CachedJava> = read_json_or(&ctx.cache_dir().join("javas.json"), vec![]).await;
        let still: Vec<JavaInstall> = cached
            .into_iter()
            .filter(|c| mtime_of(&c.path) == Some(c.mtime))
            .map(|c| JavaInstall { path: c.path, major: c.major })
            .collect();
        if !still.is_empty() {
            *detected().lock().unwrap() = Some((still.clone(), true));
            return still;
        }
    }
    let list = scan_javas(ctx).await;
    *detected().lock().unwrap() = Some((list.clone(), false));
    list
}

fn from_cache() -> bool {
    detected().lock().unwrap().as_ref().map(|d| d.1).unwrap_or(false)
}

fn find_binary(dir: &Path, depth: u32) -> Option<PathBuf> {
    if depth > 5 {
        return None;
    }
    let direct = dir.join("bin").join(EXE);
    if direct.is_file() {
        return Some(direct);
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(hit) = find_binary(&e.path(), depth + 1) {
                return Some(hit);
            }
        }
    }
    None
}

fn adoptium_url(major: u32) -> String {
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let base = std::env::var("VOX_ADOPTIUM_URL").unwrap_or_else(|_| "https://api.adoptium.net".into());
    format!("{base}/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse")
}

/// Downloads an Eclipse Temurin JRE for the requested major version into the runtimes folder.
pub async fn install_java(ctx: &Ctx, major: u32, report: &Report) -> Result<JavaInstall> {
    let stage = format!("Downloading Java {major}");
    report(&stage, 0, 1);
    let tmp = ctx.runtimes_dir().join(format!(".tmp-{major}"));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    tokio::fs::create_dir_all(&tmp).await?;
    let is_zip = cfg!(windows);
    let archive = tmp.join(if is_zip { "java.zip" } else { "java.tar.gz" });
    download(&ctx.http, &adoptium_url(major), &archive, &Expect::default()).await?;

    report(&format!("Extracting Java {major}"), 0, 1);
    let out = tmp.join("out");
    let (a, o) = (archive.clone(), out.clone());
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&o)?;
        if is_zip {
            zip::ZipArchive::new(std::fs::File::open(&a)?)?.extract(&o)?;
        } else {
            let gz = flate2::read::GzDecoder::new(std::fs::File::open(&a)?);
            tar::Archive::new(gz).unpack(&o)?;
        }
        Ok(())
    })
    .await??;

    let bin = find_binary(&out, 0).ok_or_else(|| anyhow!("Downloaded Java {major} but could not find its executable."))?;
    // <home>/bin/java -> move <home> into place
    let home = bin.parent().and_then(|p| p.parent()).ok_or_else(|| anyhow!("bad java layout"))?.to_path_buf();
    let final_dir = ctx.runtimes_dir().join(format!("java-{major}"));
    let _ = tokio::fs::remove_dir_all(&final_dir).await;
    tokio::fs::rename(&home, &final_dir).await?;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let final_bin = final_dir.join("bin").join(EXE);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&final_bin, std::fs::Permissions::from_mode(0o755)).await?;
    }
    *detected().lock().unwrap() = None;
    report(&format!("Java {major} ready"), 1, 1);
    Ok(JavaInstall { path: final_bin, major })
}

pub struct ResolveOpts<'a> {
    pub user_path: &'a str,
    pub auto_download: bool,
    pub report: &'a Report,
}

/// An explicit user path is always trusted. Otherwise: exact match, then (for Java 17+) any newer
/// install, then download a runtime if allowed.
pub async fn resolve_java(ctx: &Ctx, required: u32, opts: ResolveOpts<'_>) -> Result<JavaInstall> {
    if !opts.user_path.is_empty() {
        let p = PathBuf::from(opts.user_path);
        return match java_major(&p).await {
            Some(major) => Ok(JavaInstall { path: p, major }),
            None => bail!("The Java path \"{}\" does not point to a working Java.", opts.user_path),
        };
    }
    // A runtime we downloaded ourselves for this exact version needs no checking at all.
    let own = ctx.runtimes_dir().join(format!("java-{required}")).join("bin").join(EXE);
    if own.is_file() {
        return Ok(JavaInstall { path: own, major: required });
    }
    let pick = |list: &[JavaInstall]| -> Option<JavaInstall> {
        list.iter().find(|j| j.major == required).cloned().or_else(|| {
            if required >= 17 {
                let mut newer: Vec<_> = list.iter().filter(|j| j.major > required).cloned().collect();
                newer.sort_by_key(|j| j.major);
                newer.into_iter().next()
            } else {
                None
            }
        })
    };
    let mut found = pick(&detect_javas(ctx, false).await);
    if found.is_none() && from_cache() {
        found = pick(&detect_javas(ctx, true).await); // the remembered list may be stale
    }
    if let Some(j) = found {
        return Ok(j);
    }
    if !opts.auto_download {
        bail!("Java {required} is required but was not found. Install it or enable automatic Java download in Settings.");
    }
    install_java(ctx, required, opts.report).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_versions() {
        assert_eq!(parse_java_major("openjdk version \"21.0.4\" 2024-07-16"), Some(21));
        assert_eq!(parse_java_major("java version \"1.8.0_392\""), Some(8));
        assert_eq!(parse_java_major("openjdk version \"17\" 2021-09-14"), Some(17));
        assert_eq!(parse_java_major("command not found"), None);
    }
}
