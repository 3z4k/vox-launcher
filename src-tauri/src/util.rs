//! Shared plumbing: HTTP client, verified downloads, bounded parallelism, JSON files.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{stream, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use sha1::{Digest, Sha1};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub const USER_AGENT: &str = concat!("VoxLauncher/", env!("CARGO_PKG_VERSION"));

pub fn http_client() -> reqwest::Client {
    http_client_with_proxy("")
}

/// `proxy` is an http(s)/socks URL, or empty for the system default.
pub fn http_client_with_proxy(proxy: &str) -> reqwest::Client {
    let mut b = reqwest::Client::builder();
    if !proxy.trim().is_empty() {
        match reqwest::Proxy::all(proxy.trim()) {
            Ok(p) => b = b.proxy(p),
            Err(_) => {} // an invalid proxy URL falls back to the system setting
        }
    }
    b.user_agent(USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(60))
        .pool_max_idle_per_host(32)
        .build()
        .expect("http client")
}

/// Shared handles every module needs. Cheap to clone.
#[derive(Clone)]
pub struct Ctx {
    pub root: Arc<PathBuf>,
    pub http: reqwest::Client,
}

impl Ctx {
    pub fn new(root: PathBuf) -> Self {
        Ctx { root: Arc::new(root), http: http_client() }
    }
    pub fn with_proxy(root: PathBuf, proxy: &str) -> Self {
        Ctx { root: Arc::new(root), http: http_client_with_proxy(proxy) }
    }
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

pub async fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = tokio::fs::read(path).await.with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

pub async fn read_json_or<T: DeserializeOwned>(path: &Path, default: T) -> T {
    read_json(path).await.unwrap_or(default)
}

/// Writes through a temp file and renames, so a crash never leaves a half-written file.
pub async fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(value)?).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

pub async fn get_json<T: DeserializeOwned>(http: &reqwest::Client, url: &str) -> Result<T> {
    let resp = http.get(url).send().await.with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        bail!("{} returned {}", url, resp.status());
    }
    Ok(resp.json().await?)
}

/// What the file on disk must look like to be skipped.
#[derive(Clone, Debug, Default)]
pub struct Expect {
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// An existing file is trusted when its size matches; the hash is only recomputed when `verify` is set.
/// This is the big warm-launch win: no re-hashing of thousands of untouched files.
pub async fn file_is_good(path: &Path, expect: &Expect, verify: bool) -> bool {
    let Ok(meta) = tokio::fs::metadata(path).await else { return false };
    if !meta.is_file() {
        return false;
    }
    if let Some(size) = expect.size {
        if meta.len() != size {
            return false;
        }
    }
    if verify {
        if let Some(want) = &expect.sha1 {
            return sha1_file(path).await.map(|h| h.eq_ignore_ascii_case(want)).unwrap_or(false);
        }
    }
    true
}

pub async fn sha1_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha1::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex::encode(h.finalize()))
    })
    .await?
}

/// Downloads `url` to `dest` (via `.part`), hashing while streaming. Retries three times.
pub async fn download(http: &reqwest::Client, url: &str, dest: &Path, expect: &Expect) -> Result<()> {
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut last: Option<anyhow::Error> = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400 * attempt)).await;
        }
        match download_once(http, url, &part, expect).await {
            Ok(()) => {
                // Windows refuses to rename over an existing file.
                let _ = tokio::fs::remove_file(dest).await;
                tokio::fs::rename(&part, dest).await?;
                return Ok(());
            }
            Err(e) => last = Some(e),
        }
    }
    let _ = tokio::fs::remove_file(&part).await;
    Err(last.unwrap_or_else(|| anyhow!("download failed")).context(format!("downloading {url}")))
}

async fn download_once(http: &reqwest::Client, url: &str, part: &Path, expect: &Expect) -> Result<()> {
    let resp = http.get(url).send().await?;
    if !resp.status().is_success() {
        bail!("HTTP {}", resp.status());
    }
    let mut file = tokio::fs::File::create(part).await?;
    let mut hasher = Sha1::new();
    let mut total = 0u64;
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        total += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(size) = expect.size {
        if size != total {
            bail!("size mismatch: expected {size}, got {total}");
        }
    }
    if let Some(want) = &expect.sha1 {
        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(want) {
            bail!("sha1 mismatch: expected {want}, got {got}");
        }
    }
    Ok(())
}

/// Runs `f` over `items` with at most `limit` in flight; stops scheduling new work after the first error.
pub async fn pool<T, F, Fut>(items: Vec<T>, limit: usize, f: F) -> Result<()>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut s = stream::iter(items.into_iter().map(f)).buffer_unordered(limit.max(1));
    while let Some(r) = s.next().await {
        r?;
    }
    Ok(())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Windows needs CREATE_NO_WINDOW so spawned tools never flash a console.
pub fn hide_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vox-test-{}-{}", name, now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn file_is_good_trusts_size_unless_verifying() {
        let d = tmp("good");
        let f = d.join("a.bin");
        std::fs::write(&f, b"hello").unwrap();
        let right = hex::encode(Sha1::digest(b"hello"));
        let ok = Expect { sha1: Some(right), size: Some(5) };
        assert!(file_is_good(&f, &ok, false).await);
        assert!(file_is_good(&f, &ok, true).await);
        let wrong_hash = Expect { sha1: Some("0".repeat(40)), size: Some(5) };
        assert!(file_is_good(&f, &wrong_hash, false).await, "size-only trust when not verifying");
        assert!(!file_is_good(&f, &wrong_hash, true).await);
        assert!(!file_is_good(&f, &Expect { sha1: None, size: Some(6) }, false).await);
        assert!(!file_is_good(&d.join("missing"), &ok, false).await);
    }

    #[tokio::test]
    async fn json_roundtrip_is_atomic_and_default_works() {
        let d = tmp("json");
        let f = d.join("x/y.json");
        write_json(&f, &serde_json::json!({"a": 1})).await.unwrap();
        let v: serde_json::Value = read_json(&f).await.unwrap();
        assert_eq!(v["a"], 1);
        assert!(!f.with_extension("tmp").exists());
        let missing: Vec<u8> = read_json_or(&d.join("none.json"), vec![1]).await;
        assert_eq!(missing, vec![1]);
    }

    #[tokio::test]
    async fn pool_respects_limit_and_stops_after_failure() {
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (l, p) = (live.clone(), peak.clone());
        pool((0..40).collect(), 4, move |_| {
            let (l, p) = (l.clone(), p.clone());
            async move {
                let now = l.fetch_add(1, Ordering::SeqCst) + 1;
                p.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                l.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await
        .unwrap();
        assert!(peak.load(Ordering::SeqCst) <= 4);

        let started = Arc::new(AtomicUsize::new(0));
        let s = started.clone();
        let r = pool((0..1000).collect::<Vec<u32>>(), 4, move |i| {
            let s = s.clone();
            async move {
                s.fetch_add(1, Ordering::SeqCst);
                if i == 2 {
                    bail!("boom");
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                Ok(())
            }
        })
        .await;
        assert!(r.is_err());
        assert!(started.load(Ordering::SeqCst) < 100, "must stop scheduling after a failure");
    }
}
