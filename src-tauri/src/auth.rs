//! Microsoft -> Xbox Live -> XSTS -> Minecraft services sign-in (OAuth device-code flow), plus offline accounts.
//! Tokens are encrypted with AES-GCM; the key lives in the OS keyring (file fallback).

use crate::config::get_build_config;
use crate::core::{is_valid_username, offline_uuid};
use crate::types::{AccountInfo, DeviceCode};
use crate::util::*;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredAccount {
    id: String,
    name: String,
    #[serde(default)]
    refresh: String,
    #[serde(default, rename = "mcToken", skip_serializing_if = "Option::is_none")]
    mc_token: Option<String>,
    #[serde(default, rename = "mcExpires", skip_serializing_if = "Option::is_none")]
    mc_expires: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    offline: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct AccountStore {
    active: Option<String>,
    #[serde(default)]
    accounts: Vec<StoredAccount>,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: &'static str,
    pub offline: bool,
}

// ---- token sealing ---------------------------------------------------------

async fn key(ctx: &Ctx) -> Result<[u8; 32]> {
    let ctx2 = ctx.clone();
    tokio::task::spawn_blocking(move || -> Result<[u8; 32]> {
        let decode = |s: &str| -> Option<[u8; 32]> { B64.decode(s.trim()).ok()?.try_into().ok() };
        if let Ok(entry) = keyring::Entry::new("com.vox.launcher", "token-key") {
            if let Ok(s) = entry.get_password() {
                if let Some(k) = decode(&s) {
                    return Ok(k);
                }
            }
        }
        let file = ctx2.root.join(".token-key");
        if let Some(k) = std::fs::read_to_string(&file).ok().and_then(|s| decode(&s)) {
            return Ok(k);
        }
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        let encoded = B64.encode(k);
        let stored = keyring::Entry::new("com.vox.launcher", "token-key").and_then(|e| e.set_password(&encoded)).is_ok();
        if !stored {
            std::fs::create_dir_all(&*ctx2.root)?;
            std::fs::write(&file, &encoded)?;
        }
        Ok(k)
    })
    .await?
}

pub fn seal_with(key: &[u8; 32], value: &str) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), value.as_bytes()).map_err(|_| anyhow!("encryption failed"))?;
    Ok(format!("aes:{}", B64.encode([nonce.as_slice(), ct.as_slice()].concat())))
}

pub fn unseal_with(key: &[u8; 32], value: &str) -> Result<String> {
    if let Some(rest) = value.strip_prefix("aes:") {
        let raw = B64.decode(rest)?;
        if raw.len() < 13 {
            bail!("corrupt token");
        }
        let cipher = Aes256Gcm::new(key.into());
        let pt = cipher.decrypt(Nonce::from_slice(&raw[..12]), &raw[12..]).map_err(|_| anyhow!("Stored sign-in could not be decrypted."))?;
        return Ok(String::from_utf8(pt)?);
    }
    if value.starts_with("enc:") {
        bail!("This sign-in was saved by an older version. Remove the account and sign in again.");
    }
    Ok(String::from_utf8(B64.decode(value.trim_start_matches("plain:"))?)?)
}

// ---- storage ---------------------------------------------------------------

async fn load(ctx: &Ctx) -> AccountStore {
    read_json_or(&ctx.accounts_file(), AccountStore::default()).await
}
async fn save(ctx: &Ctx, s: &AccountStore) -> Result<()> {
    write_json(&ctx.accounts_file(), s).await
}

pub async fn list_accounts(ctx: &Ctx) -> Vec<AccountInfo> {
    let s = load(ctx).await;
    let active = s.active.clone().or_else(|| s.accounts.first().map(|a| a.id.clone()));
    s.accounts
        .iter()
        .map(|a| AccountInfo { id: a.id.clone(), name: a.name.clone(), active: Some(&a.id) == active.as_ref(), offline: a.offline })
        .collect()
}

pub async fn remove_account(ctx: &Ctx, id: &str) -> Result<()> {
    let mut s = load(ctx).await;
    s.accounts.retain(|a| a.id != id);
    if s.active.as_deref() == Some(id) {
        s.active = s.accounts.first().map(|a| a.id.clone());
    }
    save(ctx, &s).await
}

pub async fn set_active_account(ctx: &Ctx, id: &str) -> Result<()> {
    let mut s = load(ctx).await;
    if !s.accounts.iter().any(|a| a.id == id) {
        bail!("Unknown account.");
    }
    s.active = Some(id.to_string());
    save(ctx, &s).await
}

/// Adds an offline test account (no Microsoft sign-in).
pub async fn add_offline_account(ctx: &Ctx, username: &str) -> Result<AccountInfo> {
    let name = username.trim();
    if !is_valid_username(name) {
        bail!("Usernames are 3 to 16 characters: letters, numbers and underscores only.");
    }
    let mut s = load(ctx).await;
    let id = offline_uuid(name);
    s.accounts.retain(|a| a.id != id);
    s.accounts.push(StoredAccount { id: id.clone(), name: name.into(), refresh: String::new(), mc_token: None, mc_expires: None, offline: true });
    s.active = Some(id.clone());
    save(ctx, &s).await?;
    Ok(AccountInfo { id, name: name.into(), active: true, offline: true })
}

// ---- the sign-in chain -----------------------------------------------------

fn xsts_message(code: Option<u64>) -> String {
    match code {
        Some(2148916233) => "This Microsoft account has no Xbox profile. Sign in at xbox.com once to create one, then try again.".into(),
        Some(2148916235) => "Xbox Live is not available in your country or region.".into(),
        Some(2148916236) | Some(2148916237) => "This account needs adult verification on the Xbox site before it can sign in.".into(),
        Some(2148916238) => "This is a child account. Add it to a Microsoft family group, then try again.".into(),
        Some(c) => format!("Xbox sign-in was rejected (error {c})."),
        None => "Xbox sign-in was rejected.".into(),
    }
}

async fn post_json(ctx: &Ctx, url: &str, body: &Value) -> Result<(u16, Value)> {
    let r = ctx.http.post(url).header("Accept", "application/json").json(body).send().await?;
    let status = r.status().as_u16();
    Ok((status, r.json().await.unwrap_or(Value::Null)))
}

async fn post_form(ctx: &Ctx, url: &str, form: &[(&str, &str)]) -> Result<(u16, Value)> {
    let r = ctx.http.post(url).form(form).send().await?;
    let status = r.status().as_u16();
    Ok((status, r.json().await.unwrap_or(Value::Null)))
}

async fn minecraft_token(ctx: &Ctx, ms_access: &str) -> Result<(String, u64)> {
    let (st, xbl) = post_json(
        ctx,
        "https://user.auth.xboxlive.com/user/authenticate",
        &json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }),
    )
    .await?;
    if st >= 400 {
        bail!("Xbox Live authentication failed (HTTP {st}).");
    }
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or_else(|| anyhow!("Xbox Live returned an unexpected reply."))?.to_string();
    let xbl_token = xbl["Token"].as_str().unwrap_or_default().to_string();

    let (st, xsts) = post_json(
        ctx,
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        &json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }),
    )
    .await?;
    if st >= 400 {
        bail!("{}", xsts_message(xsts["XErr"].as_u64()));
    }
    let xsts_token = xsts["Token"].as_str().unwrap_or_default();

    let (st, mc) = post_json(
        ctx,
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        &json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
    )
    .await?;
    if st >= 400 {
        if st == 403 {
            bail!("Minecraft services rejected this app registration. New Azure apps must be approved by Mojang first (see the README, \"Getting a client ID\").");
        }
        bail!("Minecraft sign-in failed (HTTP {st}).");
    }
    let token = mc["access_token"].as_str().ok_or_else(|| anyhow!("Minecraft sign-in returned no token."))?.to_string();
    Ok((token, now_ms() + mc["expires_in"].as_u64().unwrap_or(86400) * 1000))
}

async fn fetch_profile(ctx: &Ctx, token: &str) -> Result<(String, String)> {
    let r = ctx.http.get("https://api.minecraftservices.com/minecraft/profile").bearer_auth(token).send().await?;
    if r.status().as_u16() == 404 {
        bail!("This Microsoft account does not own Minecraft: Java Edition.");
    }
    if !r.status().is_success() {
        bail!("Could not load the Minecraft profile (HTTP {}).", r.status().as_u16());
    }
    let j: Value = r.json().await?;
    Ok((j["id"].as_str().unwrap_or_default().to_string(), j["name"].as_str().unwrap_or_default().to_string()))
}

async fn require_client_id(ctx: &Ctx) -> Result<String> {
    let id = get_build_config(ctx).await.ms_client_id;
    if id.is_empty() {
        bail!("Microsoft sign-in is not set up in this build yet. The developer needs to add an Azure client ID (see README).");
    }
    Ok(id)
}

static LOGIN_GEN: AtomicU64 = AtomicU64::new(0);

pub fn cancel_login() {
    LOGIN_GEN.fetch_add(1, Ordering::SeqCst);
}

pub async fn login(ctx: &Ctx, on_code: impl Fn(DeviceCode)) -> Result<AccountInfo> {
    let client_id = require_client_id(ctx).await?;
    let my_gen = LOGIN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let cancelled = || LOGIN_GEN.load(Ordering::SeqCst) != my_gen;

    let (st, dc) = post_form(ctx, DEVICE_URL, &[("client_id", &client_id), ("scope", SCOPE)]).await?;
    if st >= 400 {
        bail!("{}", dc["error_description"].as_str().map(str::to_string).unwrap_or(format!("Could not start sign-in (HTTP {st}).")));
    }
    let expires_in = dc["expires_in"].as_u64().unwrap_or(900);
    on_code(DeviceCode {
        user_code: dc["user_code"].as_str().unwrap_or_default().into(),
        verification_uri: dc["verification_uri"].as_str().unwrap_or_default().into(),
        expires_in,
    });
    let device_code = dc["device_code"].as_str().unwrap_or_default().to_string();
    let mut interval = dc["interval"].as_u64().unwrap_or(5);
    let deadline = now_ms() + expires_in * 1000;

    let tokens = loop {
        if now_ms() > deadline {
            bail!("The sign-in code expired. Try again.");
        }
        // wake twice a second so Cancel is immediate
        for _ in 0..interval * 2 {
            if cancelled() {
                bail!("Sign-in cancelled.");
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let (st, r) = post_form(
            ctx,
            TOKEN_URL,
            &[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", &client_id), ("device_code", &device_code)],
        )
        .await?;
        if cancelled() {
            bail!("Sign-in cancelled.");
        }
        if st < 400 {
            break r;
        }
        match r["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            _ => bail!("{}", r["error_description"].as_str().unwrap_or("Sign-in failed.")),
        }
    };

    let (mc_token, expires) = minecraft_token(ctx, tokens["access_token"].as_str().unwrap_or_default()).await?;
    let (id, name) = fetch_profile(ctx, &mc_token).await?;
    let k = key(ctx).await?;
    let mut store = load(ctx).await;
    store.accounts.retain(|a| a.id != id);
    store.accounts.push(StoredAccount {
        id: id.clone(),
        name: name.clone(),
        refresh: seal_with(&k, tokens["refresh_token"].as_str().unwrap_or_default())?,
        mc_token: Some(seal_with(&k, &mc_token)?),
        mc_expires: Some(expires),
        offline: false,
    });
    store.active = Some(id.clone());
    save(ctx, &store).await?;
    Ok(AccountInfo { id, name, active: true, offline: false })
}

/// Returns a valid game session for the active account, refreshing tokens when needed.
pub async fn get_session(ctx: &Ctx) -> Result<Session> {
    let mut store = load(ctx).await;
    let idx = store
        .accounts
        .iter()
        .position(|a| Some(&a.id) == store.active.as_ref())
        .or(if store.accounts.is_empty() { None } else { Some(0) })
        .ok_or_else(|| anyhow!("No account signed in. Add a Microsoft account in the Accounts tab."))?;
    let acct = store.accounts[idx].clone();
    if acct.offline {
        return Ok(Session { name: acct.name, uuid: acct.id, access_token: "vox-offline".into(), user_type: "legacy", offline: true });
    }
    let k = key(ctx).await?;
    if let (Some(tok), Some(exp)) = (&acct.mc_token, acct.mc_expires) {
        if exp > now_ms() + 5 * 60_000 {
            return Ok(Session { name: acct.name, uuid: acct.id, access_token: unseal_with(&k, tok)?, user_type: "msa", offline: false });
        }
    }
    let client_id = require_client_id(ctx).await?;
    let refresh = unseal_with(&k, &acct.refresh)?;
    let (st, r) = post_form(
        ctx,
        TOKEN_URL,
        &[("grant_type", "refresh_token"), ("client_id", &client_id), ("refresh_token", &refresh), ("scope", SCOPE)],
    )
    .await?;
    if st >= 400 {
        bail!("Your session expired. Remove the account and sign in again.");
    }
    let (mc_token, expires) = minecraft_token(ctx, r["access_token"].as_str().unwrap_or_default()).await?;
    let (_, name) = fetch_profile(ctx, &mc_token).await?;
    {
        let a = &mut store.accounts[idx];
        a.name = name.clone();
        a.refresh = seal_with(&k, r["refresh_token"].as_str().unwrap_or(&refresh))?;
        a.mc_token = Some(seal_with(&k, &mc_token)?);
        a.mc_expires = Some(expires);
    }
    store.active = Some(acct.id.clone());
    save(ctx, &store).await?;
    Ok(Session { name, uuid: acct.id, access_token: mc_token, user_type: "msa", offline: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrip_and_tamper_detection() {
        let k = [7u8; 32];
        let sealed = seal_with(&k, "refresh-token-123").unwrap();
        assert!(sealed.starts_with("aes:"));
        assert!(!sealed.contains("refresh-token"));
        assert_eq!(unseal_with(&k, &sealed).unwrap(), "refresh-token-123");
        assert!(unseal_with(&[8u8; 32], &sealed).is_err());
        assert!(unseal_with(&k, "enc:abcd").is_err(), "old Electron tokens ask for a re-login");
    }

    #[tokio::test]
    async fn offline_account_flow() {
        let ctx = Ctx::new(std::env::temp_dir().join(format!("vox-auth-{}-{}", std::process::id(), now_ms())));
        assert!(add_offline_account(&ctx, "a b").await.is_err());
        let a = add_offline_account(&ctx, "Tester_01").await.unwrap();
        assert!(a.offline && a.active);
        let s = get_session(&ctx).await.unwrap();
        assert_eq!((s.name.as_str(), s.offline, s.user_type), ("Tester_01", true, "legacy"));
        assert_eq!(s.uuid, offline_uuid("Tester_01"));
        let b = add_offline_account(&ctx, "Second").await.unwrap();
        set_active_account(&ctx, &a.id).await.unwrap();
        assert_eq!(list_accounts(&ctx).await.iter().find(|x| x.active).unwrap().name, "Tester_01");
        remove_account(&ctx, &a.id).await.unwrap();
        assert_eq!(list_accounts(&ctx).await.iter().find(|x| x.active).unwrap().id, b.id);
        remove_account(&ctx, &b.id).await.unwrap();
        assert!(get_session(&ctx).await.is_err());
    }
}
