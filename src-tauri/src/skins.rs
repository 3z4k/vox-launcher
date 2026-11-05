//! Skin and cape management for the signed-in Microsoft account.

use crate::auth::get_session;
use crate::util::*;
use anyhow::{bail, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

fn profile_url() -> String {
    std::env::var("VOX_PROFILE_URL").unwrap_or_else(|_| PROFILE.to_string())
}

#[derive(Clone, Debug, Serialize)]
pub struct Cape {
    pub id: String,
    pub alias: String,
    pub active: bool,
    /// data: URL of the cape texture
    pub data: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinInfo {
    pub name: String,
    pub uuid: String,
    /// "classic" or "slim"
    pub variant: String,
    /// data: URL of the 64x64 skin texture
    pub skin: Option<String>,
    pub capes: Vec<Cape>,
}

async fn texture(ctx: &Ctx, url: &str) -> Option<String> {
    let url = url.replacen("http://", "https://", 1);
    let r = ctx.http.get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    let bytes = r.bytes().await.ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

async fn token(ctx: &Ctx) -> Result<(String, String)> {
    let s = get_session(ctx).await?;
    if s.offline {
        bail!("Offline accounts have no skin. Sign in with a Microsoft account to manage skins.");
    }
    Ok((s.access_token, s.uuid))
}

pub async fn get_skin_info(ctx: &Ctx) -> Result<SkinInfo> {
    let (tok, _) = token(ctx).await?;
    let r = ctx.http.get(profile_url()).bearer_auth(&tok).send().await?;
    if !r.status().is_success() {
        bail!("Could not load your profile (HTTP {}).", r.status().as_u16());
    }
    let v: Value = r.json().await?;
    let active = v["skins"].as_array().and_then(|a| a.iter().find(|s| s["state"] == "ACTIVE").or_else(|| a.first())).cloned().unwrap_or(Value::Null);
    let skin = match active["url"].as_str() {
        Some(u) => texture(ctx, u).await,
        None => None,
    };
    let mut capes = Vec::new();
    for c in v["capes"].as_array().cloned().unwrap_or_default() {
        capes.push(Cape {
            id: c["id"].as_str().unwrap_or_default().into(),
            alias: c["alias"].as_str().unwrap_or("Cape").into(),
            active: c["state"] == "ACTIVE",
            data: match c["url"].as_str() {
                Some(u) => texture(ctx, u).await,
                None => None,
            },
        });
    }
    Ok(SkinInfo {
        name: v["name"].as_str().unwrap_or_default().into(),
        uuid: v["id"].as_str().unwrap_or_default().into(),
        variant: active["variant"].as_str().unwrap_or("CLASSIC").to_lowercase(),
        skin,
        capes,
    })
}

/// Width and height of a PNG, read from its header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((u32::from_be_bytes(bytes[16..20].try_into().ok()?), u32::from_be_bytes(bytes[20..24].try_into().ok()?)))
}

pub async fn upload_skin(ctx: &Ctx, file: &Path, variant: &str) -> Result<()> {
    let (tok, _) = token(ctx).await?;
    let bytes = tokio::fs::read(file).await?;
    match png_size(&bytes) {
        Some((64, 64)) | Some((64, 32)) => {}
        Some((w, h)) => bail!("Skins must be 64x64 pixels (this image is {w}x{h})."),
        None => bail!("Skins must be PNG images."),
    }
    let variant = if variant == "slim" { "slim" } else { "classic" };
    let form = reqwest::multipart::Form::new()
        .text("variant", variant)
        .part("file", reqwest::multipart::Part::bytes(bytes).file_name("skin.png").mime_str("image/png")?);
    let r = ctx.http.post(format!("{}/skins", profile_url())).bearer_auth(tok).multipart(form).send().await?;
    if !r.status().is_success() {
        bail!("Minecraft rejected the skin (HTTP {}).", r.status().as_u16());
    }
    Ok(())
}

pub async fn reset_skin(ctx: &Ctx) -> Result<()> {
    let (tok, _) = token(ctx).await?;
    let r = ctx.http.delete(format!("{}/skins/active", profile_url())).bearer_auth(tok).send().await?;
    if !r.status().is_success() {
        bail!("Could not reset the skin (HTTP {}).", r.status().as_u16());
    }
    Ok(())
}

/// `None` hides the cape.
pub async fn set_cape(ctx: &Ctx, cape_id: Option<&str>) -> Result<()> {
    let (tok, _) = token(ctx).await?;
    let url = format!("{}/capes/active", profile_url());
    let r = match cape_id {
        Some(id) => ctx.http.put(url).bearer_auth(tok).json(&json!({ "capeId": id })).send().await?,
        None => ctx.http.delete(url).bearer_auth(tok).send().await?,
    };
    if !r.status().is_success() {
        bail!("Could not change the cape (HTTP {}).", r.status().as_u16());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header_is_read() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(64u32.to_be_bytes());
        png.extend(32u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((64, 32)));
        assert_eq!(png_size(b"not a png at all, really not"), None);
        assert_eq!(png_size(b""), None);
    }
}

// ---- local skin library -------------------------------------------------------------------------
// Skins you save are kept as PNGs in <data>/skins so you can switch between them in one click.

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LibEntry {
    id: String,
    name: String,
    variant: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibSkin {
    pub id: String,
    pub name: String,
    pub variant: String,
    /// data: URL of the PNG
    pub data: String,
}

fn lib_dir(ctx: &Ctx) -> std::path::PathBuf {
    ctx.root.join("skins")
}

async fn lib_entries(ctx: &Ctx) -> Vec<LibEntry> {
    read_json_or(&lib_dir(ctx).join("library.json"), Vec::<LibEntry>::new()).await
}

fn check_png(bytes: &[u8]) -> Result<()> {
    match png_size(bytes) {
        Some((64, 64)) | Some((64, 32)) => Ok(()),
        Some((w, h)) => bail!("Skins must be 64x64 pixels (this image is {w}x{h})."),
        None => bail!("Skins must be PNG images."),
    }
}

pub async fn library_list(ctx: &Ctx) -> Result<Vec<LibSkin>> {
    let mut out = Vec::new();
    for e in lib_entries(ctx).await {
        if let Ok(bytes) = tokio::fs::read(lib_dir(ctx).join(format!("{}.png", e.id))).await {
            out.push(LibSkin { id: e.id, name: e.name, variant: e.variant, data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) });
        }
    }
    Ok(out)
}

async fn lib_save(ctx: &Ctx, bytes: Vec<u8>, name: &str, variant: &str) -> Result<LibSkin> {
    check_png(&bytes)?;
    let name = name.trim();
    if name.is_empty() {
        bail!("Give the skin a name.");
    }
    let dir = lib_dir(ctx);
    tokio::fs::create_dir_all(&dir).await?;
    let mut entries = lib_entries(ctx).await;
    let id = format!("s{}", now_ms());
    let variant = if variant == "slim" { "slim" } else { "classic" }.to_string();
    tokio::fs::write(dir.join(format!("{id}.png")), &bytes).await?;
    entries.push(LibEntry { id: id.clone(), name: name.chars().take(48).collect(), variant: variant.clone() });
    write_json(&dir.join("library.json"), &entries).await?;
    Ok(LibSkin { id, name: name.to_string(), variant, data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) })
}

/// Adds a PNG from disk to the library.
pub async fn library_add(ctx: &Ctx, file: &Path, name: &str, variant: &str) -> Result<LibSkin> {
    lib_save(ctx, tokio::fs::read(file).await?, name, variant).await
}

/// Saves the skin your account is wearing right now.
pub async fn library_add_current(ctx: &Ctx, name: &str) -> Result<LibSkin> {
    let info = get_skin_info(ctx).await?;
    let data = info.skin.ok_or_else(|| anyhow::anyhow!("Your account has no custom skin to save."))?;
    let b64 = data.split_once(',').map(|x| x.1).unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
    lib_save(ctx, bytes, name, &info.variant).await
}

pub async fn library_apply(ctx: &Ctx, id: &str, variant: Option<&str>) -> Result<()> {
    let entries = lib_entries(ctx).await;
    let e = entries.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("That skin is no longer in your library."))?;
    upload_skin(ctx, &lib_dir(ctx).join(format!("{}.png", e.id)), variant.unwrap_or(&e.variant)).await
}

pub async fn library_remove(ctx: &Ctx, id: &str) -> Result<()> {
    let mut entries = lib_entries(ctx).await;
    entries.retain(|e| e.id != id);
    let dir = lib_dir(ctx);
    if id.chars().all(|c| c.is_ascii_alphanumeric()) {
        let _ = tokio::fs::remove_file(dir.join(format!("{id}.png"))).await;
    }
    write_json(&dir.join("library.json"), &entries).await
}

pub async fn library_update(ctx: &Ctx, id: &str, name: &str, variant: &str) -> Result<()> {
    let mut entries = lib_entries(ctx).await;
    let name = name.trim();
    if name.is_empty() {
        bail!("Give the skin a name.");
    }
    for e in entries.iter_mut().filter(|e| e.id == id) {
        e.name = name.chars().take(48).collect();
        e.variant = if variant == "slim" { "slim" } else { "classic" }.to_string();
    }
    write_json(&lib_dir(ctx).join("library.json"), &entries).await
}

#[cfg(test)]
mod library_tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v
    }

    #[tokio::test]
    async fn library_round_trip() {
        let ctx = Ctx::new(std::env::temp_dir().join(format!("vox-skins-{}-{}", std::process::id(), now_ms())));
        assert!(lib_save(&ctx, png(10, 10), "Bad", "classic").await.is_err());
        assert!(lib_save(&ctx, png(64, 64), "  ", "classic").await.is_err());
        let a = lib_save(&ctx, png(64, 64), "Me", "slim").await.unwrap();
        assert_eq!(library_list(&ctx).await.unwrap().len(), 1);
        library_update(&ctx, &a.id, "Renamed", "classic").await.unwrap();
        let l = library_list(&ctx).await.unwrap();
        assert_eq!((l[0].name.as_str(), l[0].variant.as_str()), ("Renamed", "classic"));
        library_remove(&ctx, &a.id).await.unwrap();
        assert!(library_list(&ctx).await.unwrap().is_empty());
    }
}
