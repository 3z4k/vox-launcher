//! Where everything lives under the data root.

use crate::util::Ctx;
use std::path::PathBuf;

impl Ctx {
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id)
    }
    pub fn version_json_path(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn version_jar_path(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn runtimes_dir(&self) -> PathBuf {
        self.root.join("runtimes")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }
    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("minecraft")
    }
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("natives")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }
}

/// Portable mode: a file called `portable.txt` next to the program keeps all data in a `data`
/// folder beside it, so the whole launcher can live on a USB stick.
pub fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("portable.txt").is_file().then(|| dir.join("data"))
}

pub fn is_portable() -> bool {
    portable_root().is_some()
}

/// Data folder: reuse the Electron build's folder when it exists so instances carry over.
pub fn default_root() -> PathBuf {
    if let Some(p) = portable_root() {
        return p;
    }
    let base = dirs_base();
    let legacy = base.join("vox-launcher").join("data");
    if legacy.exists() {
        return legacy;
    }
    base.join("Vox Launcher").join("data")
}

fn dirs_base() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(std::env::temp_dir)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(std::env::temp_dir)
    }
}
