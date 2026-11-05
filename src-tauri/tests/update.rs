//! Launcher update check and Vox Client install against a fake GitHub.

mod common;

use serde_json::json;
use vox_launcher_lib::util::{now_ms, Ctx};
use vox_launcher_lib::{client_mod, update};

#[tokio::test]
async fn update_check_and_client_status() {
    let server = common::start(|req, base| match req.path.as_str() {
        "/repos/me/vox/releases/latest" => (200, serde_json::to_vec(&json!({
            "tag_name": "v0.4.0", "html_url": "https://github.com/me/vox/releases/tag/v0.4.0", "body": "Shiny things",
            "assets": [
                { "name": "Vox-Launcher_0.4.0_x64-setup.exe", "browser_download_url": "https://github.com/me/vox/releases/download/v0.4.0/Vox-Launcher_0.4.0_x64-setup.exe", "size": 123 },
                { "name": "vox-client-0.1.0+mc1.21.4.jar", "browser_download_url": format!("{base}/dl/c.jar"), "size": 3 },
            ],
        })).unwrap()),
        _ => (404, b"{}".to_vec()),
    });
    std::env::set_var("VOX_UPDATE_URL", &server.base);
    let ctx = Ctx::new(std::env::temp_dir().join(format!("vox-upd-{}-{}", std::process::id(), now_ms())));

    let u = update::check(&ctx, "me/vox", "0.3.0").await.unwrap().expect("a newer version exists");
    assert_eq!((u.version.as_str(), u.notes.as_str()), ("0.4.0", "Shiny things"));
    assert_eq!(u.asset_name.as_deref(), Some("Vox-Launcher_0.4.0_x64-setup.exe"));
    assert_eq!(u.asset_size, Some(123));
    assert!(update::check(&ctx, "me/vox", "0.4.0").await.unwrap().is_none());
    assert!(update::check(&ctx, "", "0.3.0").await.unwrap().is_none(), "no source configured means no check");
    assert!(update::check(&ctx, "me/none", "0.3.0").await.is_err(), "a missing repo is reported");

    let st = client_mod::status(&ctx, "me/vox", "inst").await;
    assert_eq!(st.supported, vec!["1.21.4".to_string()]);
    assert!(st.installed.is_none() && st.source_known);
}
