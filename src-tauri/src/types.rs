//! Data shared with the UI. Field names match src/renderer/types.ts exactly.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

impl Loader {
    pub fn as_str(&self) -> &'static str {
        match self {
            Loader::Vanilla => "vanilla",
            Loader::Fabric => "fabric",
            Loader::Quilt => "quilt",
            Loader::Forge => "forge",
            Loader::Neoforge => "neoforge",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: String,
    #[serde(default)]
    pub version_id: String,
    pub created: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
    /// Total time the game has been open, in milliseconds.
    #[serde(default)]
    pub played_ms: u64,

    // Organisation
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub notes: String,

    // Launch settings. Empty / null means "use the launcher default".
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub min_memory_mb: Option<u32>,
    #[serde(default)]
    pub java_path: String,
    #[serde(default)]
    pub jvm_args: String,
    #[serde(default)]
    pub extra_game_args: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fullscreen: Option<bool>,
    #[serde(default)]
    pub wrapper: String,
    #[serde(default)]
    pub pre_launch: String,
    #[serde(default)]
    pub post_exit: String,
    #[serde(default)]
    pub env_vars: String,
    /// "" = off, "mc" = the standard .minecraft folder, otherwise another instance id.
    /// options.txt (video settings, keybinds, ...) is copied from there before every launch.
    #[serde(default)]
    pub sync_options_from: String,
    /// A server ("host" or "host:port") to join automatically when the game starts. Empty = off.
    #[serde(default)]
    pub join_server: String,
    /// A world (folder name in saves) to open automatically instead. Used when no server is set.
    #[serde(default)]
    pub join_world: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_next: Option<bool>,
}

/// Keys the UI may change through `update_instance`. Identity and game-version fields are not editable.
pub const EDITABLE_INSTANCE_KEYS: &[&str] = &[
    "name", "versionId", "lastPlayed", "playedMs", "group", "icon", "notes", "memoryMb", "minMemoryMb", "javaPath", "jvmArgs",
    "extraGameArgs", "width", "height", "fullscreen", "wrapper", "preLaunch", "postExit", "envVars", "syncOptionsFrom", "joinServer", "joinWorld", "verifyNext",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // Java & memory
    pub default_memory_mb: u32,
    pub default_min_memory_mb: u32,
    pub java_path: String,
    pub auto_download_java: bool,
    pub jvm_args: String,
    // Game window
    pub default_width: u32,
    pub default_height: u32,
    pub default_fullscreen: bool,
    // Commands
    pub wrapper: String,
    pub pre_launch: String,
    pub post_exit: String,
    pub env_vars: String,
    // Launcher behaviour
    pub launcher_action: String,
    pub show_console_on_launch: bool,
    pub check_launcher_updates: bool,
    /// "owner/name" of the GitHub repository that holds releases; empty uses the one built into the launcher.
    pub update_repo: String,
    pub notifications: bool,
    pub console_max_lines: u32,
    pub show_snapshots: bool,
    pub check_mod_updates: bool,
    // Network
    pub download_threads: u32,
    pub proxy_url: String,
    // Discord
    pub discord_enabled: bool,
    pub discord_show_instance: bool,
    pub discord_show_time: bool,
    pub discord_show_server: bool,
    // Appearance
    pub theme: String,
    /// "cover" | "dense" | "glass" | "classic" | "compact" | "right" | "bar" | "zen"
    pub layout: String,
    pub accent: String,
    pub ui_scale: u32,
    pub compact: bool,
    pub reduce_motion: bool,
    /// Warn about duplicate / mismatched / missing mods before launching.
    pub warn_mod_conflicts: bool,
    /// Colours and shape of the "custom" theme. Keys: bg side panel text muted accent border radius.
    pub custom_theme: std::collections::BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_memory_mb: 4096,
            default_min_memory_mb: 512,
            java_path: String::new(),
            auto_download_java: true,
            jvm_args: String::new(),
            default_width: 0,
            default_height: 0,
            default_fullscreen: false,
            wrapper: String::new(),
            pre_launch: String::new(),
            post_exit: String::new(),
            env_vars: String::new(),
            launcher_action: "keep".into(),
            show_console_on_launch: true,
            check_launcher_updates: true,
            update_repo: String::new(),
            notifications: true,
            console_max_lines: 2000,
            show_snapshots: false,
            check_mod_updates: true,
            download_threads: 16,
            proxy_url: String::new(),
            discord_enabled: true,
            discord_show_instance: true,
            discord_show_time: true,
            discord_show_server: false,
            theme: "voxel".into(),
            layout: "cover".into(),
            accent: String::new(),
            ui_scale: 100,
            compact: false,
            reduce_motion: false,
            warn_mod_conflicts: true,
            custom_theme: std::collections::BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Brings every value into a sane range.
    pub fn sanitized(mut self) -> Settings {
        self.default_memory_mb = self.default_memory_mb.clamp(512, 262144);
        self.default_min_memory_mb = self.default_min_memory_mb.clamp(128, self.default_memory_mb);
        self.download_threads = self.download_threads.clamp(1, 64);
        self.console_max_lines = self.console_max_lines.clamp(100, 100_000);
        self.ui_scale = self.ui_scale.clamp(75, 150);
        self.custom_theme = std::mem::take(&mut self.custom_theme)
            .into_iter()
            .filter(|(k, v)| match k.as_str() {
                "bg" | "side" | "panel" | "text" | "muted" | "accent" | "border" => v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()),
                "radius" => v.parse::<u8>().map(|n| n <= 28).unwrap_or(false),
                _ => false,
            })
            .collect();
        for s in [&mut self.java_path, &mut self.proxy_url, &mut self.accent] {
            *s = s.trim().to_string();
        }
        if !["keep", "minimize"].contains(&self.launcher_action.as_str()) {
            self.launcher_action = "keep".into();
        }
        {
            let r = self.update_repo.trim().trim_end_matches('/');
            let r = r.strip_suffix(".git").unwrap_or(r);
            let r = r.strip_prefix("https://github.com/").or_else(|| r.strip_prefix("github.com/")).unwrap_or(r);
            let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
            let mut it = r.split('/');
            self.update_repo = match (it.next(), it.next(), it.next()) {
                (Some(a), Some(b), None) if ok(a) && ok(b) => format!("{a}/{b}"),
                _ => String::new(),
            };
        }
        if !["cover", "dense", "glass", "classic", "compact", "right", "bar", "zen"].contains(&self.layout.as_str()) {
            self.layout = "cover".into();
        }
        if !["voxel", "sleek", "bold", "minimal", "custom"].contains(&self.theme.as_str()) {
            self.theme = "voxel".into();
        }
        let hex = self.accent.strip_prefix('#').unwrap_or("");
        if !(self.accent.is_empty() || (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))) {
            self.accent.clear();
        }
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub offline: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}
