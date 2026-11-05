//! Build-time identifiers. End users never see or type these.
//!
//! - MS_CLIENT_ID: Azure application (client) ID used for Microsoft sign-in.
//! - DISCORD_APP_ID: Discord application ID used for Rich Presence.
//!
//! Change them here before building, or drop a `config.json` into the data folder to override them
//! without rebuilding: { "msClientId": "...", "discordAppId": "..." }

use crate::util::{read_json, Ctx};
use serde::Deserialize;

pub const MS_CLIENT_ID: &str = "f8dd1afd-1880-418d-bb9b-97db7300eaec";
pub const DISCORD_APP_ID: &str = "1567180705442701353";
/// GitHub repository ("owner/name") whose releases hold new launcher versions and the Vox Client mod.
/// The GitHub workflow fills it in automatically (VOX_UPDATE_REPO); it can also be set in config.json.
pub const UPDATE_REPO: &str = match option_env!("VOX_UPDATE_REPO") {
    Some(r) => r,
    None => "sylphiras/vox-launcher",
};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Override {
    ms_client_id: Option<String>,
    discord_app_id: Option<String>,
    update_repo: Option<String>,
}

pub struct BuildConfig {
    pub ms_client_id: String,
    pub discord_app_id: String,
    pub update_repo: String,
}

pub async fn get_build_config(ctx: &Ctx) -> BuildConfig {
    let o: Override = read_json(&ctx.root.join("config.json")).await.unwrap_or_default();
    BuildConfig {
        ms_client_id: o.ms_client_id.unwrap_or_else(|| MS_CLIENT_ID.into()).trim().to_string(),
        discord_app_id: o.discord_app_id.unwrap_or_else(|| DISCORD_APP_ID.into()).trim().to_string(),
        update_repo: o.update_repo.unwrap_or_else(|| UPDATE_REPO.into()).trim().to_string(),
    }
}
