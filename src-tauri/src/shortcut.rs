//! Desktop / Start menu shortcuts that launch one instance directly (`Vox Launcher.exe --launch <id>`).

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

/// The instance id from `--launch <id>` (or `--launch=<id>`), when present and safe.
pub fn launch_arg(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let v = if a == "--launch" { it.next().cloned() } else { a.strip_prefix("--launch=").map(String::from) };
        if let Some(v) = v {
            let v = v.trim().to_string();
            if !v.is_empty() && v.len() <= 80 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Some(v);
            }
        }
    }
    None
}

/// A file name that is valid on Windows, macOS and Linux.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches('.').trim().to_string();
    let cleaned: String = cleaned.chars().take(60).collect();
    if cleaned.is_empty() { "Minecraft".into() } else { cleaned }
}

#[cfg(windows)]
/// Writes a Windows .lnk file directly (no PowerShell or other helper program is run).
pub fn write_lnk(dir: &std::path::Path, name: &str, exe: &std::path::Path, id: &str, title: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.lnk"));
    let mut lnk = mslnk::ShellLink::new(exe).map_err(|e| anyhow!("Could not prepare the shortcut: {e}"))?;
    lnk.set_arguments(Some(format!("--launch {id}")));
    lnk.set_name(Some(format!("Play {title} with Vox Launcher")));
    if let Some(d) = exe.parent() {
        lnk.set_working_dir(Some(d.to_string_lossy().to_string()));
    }
    lnk.set_icon_location(Some(exe.to_string_lossy().to_string()));
    lnk.create_lnk(&path).map_err(|e| anyhow!("Could not save the shortcut: {e}"))?;
    Ok(path)
}

/// Creates the shortcut and returns where it was saved. `place` is "desktop" or "startmenu".
pub fn create(id: &str, title: &str, place: &str) -> Result<PathBuf> {
    if launch_arg(&["--launch".into(), id.into()]).is_none() {
        bail!("That instance id cannot be used in a shortcut.");
    }
    let place = if place == "startmenu" { "startmenu" } else { "desktop" };
    let exe = std::env::current_exe().map_err(|e| anyhow!("Could not find the launcher program: {e}"))?;
    let name = format!("{} - Vox Launcher", safe_name(title));
    platform_create(id, title, place, &name, &exe)
}

#[cfg(windows)]
fn platform_create(id: &str, title: &str, place: &str, name: &str, exe: &std::path::Path) -> Result<PathBuf> {
    let dir = if place == "startmenu" {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).ok_or_else(|| anyhow!("Windows did not say where the Start menu is."))?;
        appdata.join("Microsoft").join("Windows").join("Start Menu").join("Programs")
    } else {
        // A desktop that OneDrive backs up lives in the OneDrive folder.
        let profile = std::env::var_os("USERPROFILE").map(PathBuf::from).ok_or_else(|| anyhow!("Windows did not say where the desktop is."))?;
        let one = std::env::var_os("OneDrive").map(|p| PathBuf::from(p).join("Desktop")).filter(|p| p.is_dir());
        one.unwrap_or_else(|| profile.join("Desktop"))
    };
    write_lnk(&dir, name, exe, id, title)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_create(id: &str, title: &str, place: &str, name: &str, exe: &std::path::Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| anyhow!("No home folder."))?;
    let dir = if place == "startmenu" { home.join(".local/share/applications") } else { home.join("Desktop") };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.desktop"));
    let exec = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let text = format!("[Desktop Entry]\nType=Application\nName={}\nComment=Play {} with Vox Launcher\nExec=\"{exec}\" --launch {id}\nTerminal=false\nCategories=Game;\n", name.replace('\n', " "), title.replace('\n', " "));
    std::fs::write(&path, text)?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    Ok(path)
}

#[cfg(target_os = "macos")]
fn platform_create(_id: &str, _title: &str, _place: &str, _name: &str, _exe: &std::path::Path) -> Result<PathBuf> {
    bail!("Shortcuts are not supported on macOS yet.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launch_argument_is_read_safely() {
        assert_eq!(launch_arg(&v(&["app.exe", "--launch", "my-pack_1"])), Some("my-pack_1".into()));
        assert_eq!(launch_arg(&v(&["app.exe", "--launch=abc"])), Some("abc".into()));
        assert_eq!(launch_arg(&v(&["app.exe"])), None);
        assert_eq!(launch_arg(&v(&["app.exe", "--launch"])), None);
        for bad in ["../x", "a b", "a;b", "a\"b", ""] {
            assert_eq!(launch_arg(&v(&["app.exe", "--launch", bad])), None, "{bad}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn writes_a_real_lnk_file() {
        let d = std::env::temp_dir().join(format!("vox-lnk-{}-{}", std::process::id(), crate::util::now_ms()));
        let exe = std::env::current_exe().unwrap();
        let p = write_lnk(&d, "My Pack", &exe, "my-pack", "My Pack").unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..4], &[0x4c, 0, 0, 0], "shell link header");
        assert!(bytes.windows(b"--launch my-pack".len() * 2).any(|w| w.iter().step_by(2).copied().eq(b"--launch my-pack".iter().copied())), "arguments are stored");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn file_names_are_cleaned() {
        assert_eq!(safe_name("My: Pack/1?"), "My Pack 1");
        assert_eq!(safe_name("..."), "Minecraft");
        assert_eq!(safe_name("   "), "Minecraft");
        assert!(safe_name(&"x".repeat(200)).len() <= 60);
    }
}
