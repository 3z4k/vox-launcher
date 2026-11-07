//! Account faces: the public skin texture is found through the session server and cached.

mod common;

use base64::Engine;
use vox_launcher_lib::util::{now_ms, Ctx};

#[tokio::test]
async fn finds_and_caches_the_public_skin() {
    let skin = b"\x89PNG-fake-skin".to_vec();
    let s2 = skin.clone();
    let srv = common::start(move |req, base| {
        if req.path.starts_with("/session/") {
            let tex = serde_json::json!({"textures":{"SKIN":{"url": format!("{base}/tex/abc")}}});
            let val = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&tex).unwrap());
            (200, serde_json::to_vec(&serde_json::json!({"properties":[{"name":"textures","value":val}]})).unwrap())
        } else if req.path == "/tex/abc" {
            (200, s2.clone())
        } else {
            (404, vec![])
        }
    });
    std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
    std::env::set_var("VOX_SESSION_URL", format!("{}/session", srv.base));
    let c = Ctx::new(std::env::temp_dir().join(format!("vox-head-{}-{}", std::process::id(), now_ms())));
    let uuid = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
    let want = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&skin));
    assert_eq!(vox_launcher_lib::skins::public_skin(&c, uuid).await.unwrap(), want);
    let hits = srv.hits.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(vox_launcher_lib::skins::public_skin(&c, uuid).await.unwrap(), want);
    assert_eq!(srv.hits.load(std::sync::atomic::Ordering::SeqCst), hits, "second call comes from the cache");
    assert!(vox_launcher_lib::skins::public_skin(&c, "not-a-uuid").await.is_none());
}
