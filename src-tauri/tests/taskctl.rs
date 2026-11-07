//! Pause, resume and cancel for background downloads, and the byte counter behind the speed display.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use vox_launcher_lib::util::{self, now_ms, Expect, TaskCtl, CTL};

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("vox-taskctl-{}-{}-{name}", std::process::id(), now_ms()))
}

#[tokio::test]
async fn pause_resume_cancel_and_count_bytes() {
    let body = vec![7u8; 300_000];
    let b2 = body.clone();
    let srv = common::start(move |_, _| (200, b2.clone()));
    let http = reqwest::Client::new();
    let url = format!("{}/f", srv.base);

    // Without a task, downloads behave exactly as before.
    let plain = tmp("plain");
    util::download(&http, &url, &plain, &Expect::default()).await.unwrap();
    assert_eq!(std::fs::read(&plain).unwrap().len(), body.len());

    // Paused: nothing is written until it is resumed, then it finishes and the bytes are counted.
    let ctl = Arc::new(TaskCtl::default());
    ctl.paused.store(true, Ordering::Relaxed);
    let dest = tmp("paused");
    let (h2, u2, d2, c2) = (http.clone(), url.clone(), dest.clone(), ctl.clone());
    let job = tokio::spawn(async move { CTL.scope(c2, async move { util::download(&h2, &u2, &d2, &Expect::default()).await }).await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!job.is_finished(), "a paused download must wait");
    assert!(!dest.exists());
    ctl.paused.store(false, Ordering::Relaxed);
    job.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap().len(), body.len());
    assert_eq!(ctl.bytes.load(Ordering::Relaxed), body.len() as u64);

    // Cancelled: fails with "Cancelled", is not retried, and leaves no partial file behind.
    let before = srv.hits.load(Ordering::SeqCst);
    let ctl = Arc::new(TaskCtl::default());
    ctl.cancelled.store(true, Ordering::Relaxed);
    let dest = tmp("cancelled");
    let d2 = dest.clone();
    let err = CTL.scope(ctl, async { util::download(&http, &url, &d2, &Expect::default()).await }).await.unwrap_err();
    assert!(format!("{err:#}").contains("Cancelled"), "{err:#}");
    assert_eq!(srv.hits.load(Ordering::SeqCst), before + 1, "a cancelled download is not retried");
    assert!(!dest.exists() && !std::path::Path::new(&format!("{}.part", dest.display())).exists());

    // The registry hands out the same controls for the same task id until it is dropped.
    let a = util::ctl_for("t-1");
    assert!(Arc::ptr_eq(&a, &util::ctl_get("t-1").unwrap()));
    util::ctl_drop("t-1");
    assert!(util::ctl_get("t-1").is_none() && a.finished.load(Ordering::Relaxed));
}
