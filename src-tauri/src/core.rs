//! Pure game logic: no network, no filesystem, no Tauri. Everything here is unit-tested at the bottom.

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Platform + rules
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub release: String,
}

impl Platform {
    pub fn new(os: &str, arch: &str, release: &str) -> Self {
        Platform { os: os.into(), arch: arch.into(), release: release.into() }
    }
}

pub fn current_platform() -> Platform {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => other,
    };
    // Only Mojang's Windows "version" rules (^10\.) look at this, and every supported Windows is 10 or newer.
    let release = if cfg!(target_os = "windows") { "10.0.0" } else { "" };
    Platform::new(os, arch, release)
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

fn os_matches(o: Option<&OsRule>, p: &Platform) -> bool {
    let Some(o) = o else { return true };
    if o.name.as_ref().is_some_and(|n| n != &p.os) {
        return false;
    }
    if o.arch.as_ref().is_some_and(|a| a != &p.arch) {
        return false;
    }
    if let Some(v) = &o.version {
        match Regex::new(v) {
            Ok(re) if re.is_match(&p.release) => {}
            _ => return false,
        }
    }
    true
}

/// Mojang rule evaluation: the last matching rule decides; no rules means allowed.
pub fn rule_allows(rules: Option<&[Rule]>, features: &HashMap<String, bool>, p: &Platform) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        let features_match = r
            .features
            .as_ref()
            .map_or(true, |f| f.iter().all(|(k, v)| features.get(k).copied().unwrap_or(false) == *v));
        if os_matches(r.os.as_ref(), p) && features_match {
            allowed = r.action == "allow";
        }
    }
    allowed
}

// ---------------------------------------------------------------------------
// Version JSON model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LibDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Extract {
    pub exclude: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Library {
    pub name: String,
    pub url: Option<String>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
    pub downloads: Option<LibDownloads>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Value>,
    #[serde(default)]
    pub jvm: Vec<Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FileDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct VersionDownloads {
    pub client: Option<FileDownload>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LogFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LogClient {
    pub argument: String,
    pub file: LogFile,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Logging {
    pub client: Option<LogClient>,
}

/// One version JSON, or (after `merge_versions`) a version together with everything it inherits.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub main_class: String,
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<VersionDownloads>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    pub jar: Option<String>,
    /// Id of the version whose jar is the actual game jar. Filled in when versions are resolved.
    #[serde(skip)]
    pub jar_id: String,
}

// ---------------------------------------------------------------------------
// Libraries
// ---------------------------------------------------------------------------

/// group:artifact:version[:classifier][@ext] -> repository-relative path.
pub fn maven_path(name: &str) -> String {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let mut parts = coords.split(':');
    let group = parts.next().unwrap_or("");
    let artifact = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");
    let file = match parts.next() {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    format!("{}/{artifact}/{version}/{file}", group.replace('.', "/"))
}

/// group:artifact[:classifier], which identifies a library regardless of its version.
pub fn lib_key(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name);
    let p: Vec<&str> = base.split(':').collect();
    let mut key: Vec<&str> = p.iter().take(2).copied().collect();
    if let Some(classifier) = p.get(3) {
        key.push(classifier);
    }
    key.join(":")
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

pub fn artifact_of(lib: &Library) -> Option<FileRef> {
    if let Some(a) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
        return Some(FileRef {
            path: a.path.clone().unwrap_or_else(|| maven_path(&lib.name)),
            url: a.url.clone(),
            sha1: a.sha1.clone(),
            size: a.size,
        });
    }
    // Legacy entries that only carry per-OS native classifiers have no main artifact.
    if lib.downloads.as_ref().is_some_and(|d| d.classifiers.is_some()) && lib.url.is_none() {
        return None;
    }
    let rel = maven_path(&lib.name);
    let base = lib.url.clone().unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
    let base = if base.ends_with('/') { base } else { format!("{base}/") };
    Some(FileRef { url: format!("{base}{rel}"), path: rel, sha1: None, size: None })
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeRef {
    pub file: FileRef,
    pub exclude: Vec<String>,
}

pub fn native_artifacts(lib: &Library, p: &Platform) -> Vec<NativeRef> {
    let exclude = lib
        .extract
        .as_ref()
        .and_then(|e| e.exclude.clone())
        .unwrap_or_else(|| vec!["META-INF/".to_string()]);
    if let Some(key) = lib.natives.as_ref().and_then(|n| n.get(&p.os)) {
        let classifier = key.replace("${arch}", if p.arch == "x86" { "32" } else { "64" });
        let Some(a) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()).and_then(|c| c.get(&classifier)) else {
            return vec![];
        };
        return vec![NativeRef {
            file: FileRef {
                path: a.path.clone().unwrap_or_else(|| maven_path(&format!("{}:{}", lib.name, classifier))),
                url: a.url.clone(),
                sha1: a.sha1.clone(),
                size: a.size,
            },
            exclude,
        }];
    }
    if lib.name.contains(":natives-") {
        return artifact_of(lib).map(|file| vec![NativeRef { file, exclude }]).unwrap_or_default();
    }
    vec![]
}

/// Libraries allowed on this platform; the first occurrence of a library wins (a child overrides its parent).
pub fn effective_libraries<'a>(v: &'a VersionJson, p: &Platform) -> Vec<&'a Library> {
    let none = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for lib in &v.libraries {
        if !rule_allows(lib.rules.as_deref(), &none, p) {
            continue;
        }
        if seen.insert(lib_key(&lib.name)) {
            out.push(lib);
        }
    }
    out
}

pub fn merge_versions(parent: VersionJson, child: VersionJson) -> VersionJson {
    let arguments = if parent.arguments.is_some() || child.arguments.is_some() {
        let p = parent.arguments.clone().unwrap_or_default();
        let c = child.arguments.clone().unwrap_or_default();
        Some(Arguments { game: [p.game, c.game].concat(), jvm: [p.jvm, c.jvm].concat() })
    } else {
        None
    };
    let mut libraries = child.libraries.clone();
    libraries.extend(parent.libraries.iter().cloned());
    VersionJson {
        id: child.id,
        inherits_from: child.inherits_from,
        kind: child.kind.or(parent.kind),
        main_class: if child.main_class.is_empty() { parent.main_class } else { child.main_class },
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        arguments,
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        logging: child.logging.or(parent.logging),
        jar: child.jar.or(parent.jar),
        jar_id: parent.jar_id,
    }
}

// ---------------------------------------------------------------------------
// Launch arguments
// ---------------------------------------------------------------------------

pub type Vars = HashMap<String, String>;

fn var_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").unwrap())
}

fn substitute(s: &str, vars: &Vars) -> String {
    var_re()
        .replace_all(s, |c: &regex::Captures| vars.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string()))
        .into_owned()
}

fn expand_args(list: &[Value], vars: &Vars, features: &HashMap<String, bool>, p: &Platform) -> Vec<String> {
    let mut out = Vec::new();
    for entry in list {
        if let Some(s) = entry.as_str() {
            out.push(substitute(s, vars));
            continue;
        }
        let rules: Option<Vec<Rule>> = entry.get("rules").and_then(|r| serde_json::from_value(r.clone()).ok());
        if !rule_allows(rules.as_deref(), features, p) {
            continue;
        }
        match entry.get("value") {
            Some(Value::String(s)) => out.push(substitute(s, vars)),
            Some(Value::Array(items)) => out.extend(items.iter().filter_map(|x| x.as_str()).map(|x| substitute(x, vars))),
            _ => {}
        }
    }
    out
}

/// Splits a user-entered argument string, honouring simple "double quotes".
pub fn split_args(input: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#""([^"]*)"|(\S+)"#).unwrap());
    re.captures_iter(input)
        .map(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default())
        .collect()
}

/// Returns (jvm arguments, game arguments) for a resolved version.
pub fn build_args(v: &VersionJson, vars: &Vars, features: &HashMap<String, bool>, p: &Platform) -> (Vec<String>, Vec<String>) {
    let jvm = match v.arguments.as_ref().filter(|a| !a.jvm.is_empty()) {
        Some(a) => expand_args(&a.jvm, vars, features, p),
        None => vec![
            format!("-Djava.library.path={}", vars.get("natives_directory").cloned().unwrap_or_default()),
            "-cp".to_string(),
            vars.get("classpath").cloned().unwrap_or_default(),
        ],
    };
    let game = match v.arguments.as_ref().filter(|a| !a.game.is_empty()) {
        Some(a) => expand_args(&a.game, vars, features, p),
        None => split_args(v.minecraft_arguments.as_deref().unwrap_or("")).iter().map(|a| substitute(a, vars)).collect(),
    };
    (jvm, game)
}

// ---------------------------------------------------------------------------
// Offline accounts
// ---------------------------------------------------------------------------

/// Valid Minecraft usernames: 3 to 16 letters, digits or underscores.
pub fn is_valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The UUID Minecraft derives for offline players (UUID v3 of "OfflinePlayer:<name>"), without dashes.
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut b: [u8; 16] = h.finalize().into();
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    hex::encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn win() -> Platform {
        Platform::new("windows", "x64", "10.0.22631")
    }
    fn mac() -> Platform {
        Platform::new("osx", "arm64", "23.0.0")
    }
    fn linux() -> Platform {
        Platform::new("linux", "x64", "6.1.0")
    }
    fn no_features() -> HashMap<String, bool> {
        HashMap::new()
    }
    fn rules(v: Value) -> Vec<Rule> {
        serde_json::from_value(v).unwrap()
    }
    fn version(v: Value) -> VersionJson {
        let mut j: VersionJson = serde_json::from_value(v).unwrap();
        j.jar_id = j.id.clone();
        j
    }

    #[test]
    fn maven_path_builds_repository_paths() {
        assert_eq!(maven_path("net.fabricmc:fabric-loader:0.16.9"), "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar");
        assert_eq!(maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows"), "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar");
        assert_eq!(maven_path("de.oceanlabs.mcp:mcp_config:1.20.1@zip"), "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip");
    }

    #[test]
    fn lib_key_ignores_version_but_keeps_classifier() {
        assert_eq!(lib_key("org.ow2.asm:asm:9.6"), "org.ow2.asm:asm");
        assert_eq!(lib_key("org.lwjgl:lwjgl:3.3.3:natives-linux"), "org.lwjgl:lwjgl:natives-linux");
    }

    #[test]
    fn rules_no_rules_means_allowed() {
        assert!(rule_allows(None, &no_features(), &win()));
        assert!(rule_allows(Some(&[]), &no_features(), &win()));
    }

    #[test]
    fn rules_os() {
        let r = rules(json!([{ "action": "allow" }, { "action": "disallow", "os": { "name": "osx" } }]));
        assert!(rule_allows(Some(&r), &no_features(), &win()));
        assert!(!rule_allows(Some(&r), &no_features(), &mac()));
        let only = rules(json!([{ "action": "allow", "os": { "name": "osx", "arch": "arm64" } }]));
        assert!(rule_allows(Some(&only), &no_features(), &mac()));
        assert!(!rule_allows(Some(&only), &no_features(), &linux()));
    }

    #[test]
    fn rules_features_and_os_version_regex() {
        let demo = rules(json!([{ "action": "allow", "features": { "is_demo_user": true } }]));
        assert!(!rule_allows(Some(&demo), &no_features(), &win()));
        let mut f = HashMap::new();
        f.insert("is_demo_user".to_string(), true);
        assert!(rule_allows(Some(&demo), &f, &win()));
        let win10 = rules(json!([{ "action": "allow", "os": { "name": "windows", "version": "^10\\." } }]));
        assert!(rule_allows(Some(&win10), &no_features(), &win()));
        assert!(!rule_allows(Some(&win10), &no_features(), &Platform::new("windows", "x64", "6.1.7601")));
    }

    fn vanilla() -> VersionJson {
        version(json!({
            "id": "1.21.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": { "id": "17", "sha1": "x", "size": 1, "url": "u" },
            "javaVersion": { "majorVersion": 21 },
            "libraries": [
                { "name": "org.ow2.asm:asm:9.3", "downloads": { "artifact": { "path": "org/ow2/asm/asm/9.3/asm-9.3.jar", "url": "https://x/asm93.jar", "sha1": "a", "size": 1 } } },
                { "name": "org.lwjgl:lwjgl:3.3.3:natives-windows", "rules": [{ "action": "allow", "os": { "name": "windows" } }],
                  "downloads": { "artifact": { "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar", "url": "https://x/lw.jar", "sha1": "b", "size": 2 } } }
            ],
            "arguments": {
                "game": [
                    "--username", "${auth_player_name}", "--version", "${version_name}",
                    { "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }], "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"] },
                    { "rules": [{ "action": "allow", "features": { "is_demo_user": true } }], "value": "--demo" }
                ],
                "jvm": [
                    { "rules": [{ "action": "allow", "os": { "name": "osx" } }], "value": ["-XstartOnFirstThread"] },
                    "-Djava.library.path=${natives_directory}", "-cp", "${classpath}"
                ]
            }
        }))
    }

    fn fabric() -> VersionJson {
        version(json!({
            "id": "fabric-loader-0.16.9-1.21.1", "inheritsFrom": "1.21.1", "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [
                { "name": "org.ow2.asm:asm:9.7", "url": "https://maven.fabricmc.net/" },
                { "name": "net.fabricmc:fabric-loader:0.16.9", "url": "https://maven.fabricmc.net/" }
            ],
            "arguments": { "game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] }
        }))
    }

    #[test]
    fn merge_child_overrides_main_class_parent_supplies_the_rest() {
        let merged = merge_versions(vanilla(), fabric());
        assert_eq!(merged.id, "fabric-loader-0.16.9-1.21.1");
        assert_eq!(merged.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(merged.jar_id, "1.21.1");
        assert_eq!(merged.asset_index.as_ref().unwrap().id, "17");
        assert_eq!(merged.java_version.as_ref().unwrap().major_version, 21);
        assert_eq!(merged.libraries.len(), 4);
        assert_eq!(merged.arguments.as_ref().unwrap().jvm.len(), vanilla().arguments.unwrap().jvm.len() + 1);
    }

    #[test]
    fn effective_libraries_child_wins_and_os_rules_apply() {
        let merged = merge_versions(vanilla(), fabric());
        let on_win = effective_libraries(&merged, &win());
        let asm: Vec<_> = on_win.iter().filter(|l| lib_key(&l.name) == "org.ow2.asm:asm").collect();
        assert_eq!(asm.len(), 1);
        assert_eq!(asm[0].name, "org.ow2.asm:asm:9.7");
        assert!(on_win.iter().any(|l| l.name.contains("natives-windows")));
        assert!(!effective_libraries(&merged, &linux()).iter().any(|l| l.name.contains("natives-windows")));
    }

    #[test]
    fn maven_style_libraries_resolve_against_their_repository() {
        let a = artifact_of(&fabric().libraries[1]).unwrap();
        assert_eq!(a.url, "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar");
        assert_eq!(a.path, "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar");
    }

    #[test]
    fn natives_legacy_classifiers_and_natives_artifacts() {
        let legacy: Library = serde_json::from_value(json!({
            "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
            "natives": { "windows": "natives-windows", "linux": "natives-linux", "osx": "natives-osx" },
            "extract": { "exclude": ["META-INF/"] },
            "downloads": { "classifiers": { "natives-linux": { "path": "p/linux.jar", "url": "https://x/l.jar", "sha1": "c", "size": 3 } } }
        }))
        .unwrap();
        let lin = native_artifacts(&legacy, &linux());
        assert_eq!(lin.len(), 1);
        assert_eq!(lin[0].file.path, "p/linux.jar");
        assert_eq!(native_artifacts(&legacy, &win()).len(), 0);
        let v = vanilla();
        assert_eq!(native_artifacts(&v.libraries[1], &win()).len(), 1);
        assert_eq!(native_artifacts(&v.libraries[0], &win()).len(), 0);
    }

    fn vars() -> Vars {
        [
            ("auth_player_name", "Steve"),
            ("version_name", "fabric-loader-0.16.9-1.21.1"),
            ("resolution_width", "1280"),
            ("resolution_height", "720"),
            ("natives_directory", "/n"),
            ("classpath", "/a.jar:/b.jar"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn modern_arguments_with_conditional_features() {
        let (jvm, game) = build_args(&vanilla(), &vars(), &no_features(), &linux());
        assert_eq!(game, ["--username", "Steve", "--version", "fabric-loader-0.16.9-1.21.1"]);
        assert_eq!(jvm, ["-Djava.library.path=/n", "-cp", "/a.jar:/b.jar"]);

        let mut f = HashMap::new();
        f.insert("has_custom_resolution".to_string(), true);
        let (jvm, game) = build_args(&vanilla(), &vars(), &f, &mac());
        assert_eq!(&game[game.len() - 4..], ["--width", "1280", "--height", "720"]);
        assert_eq!(jvm[0], "-XstartOnFirstThread");
    }

    #[test]
    fn legacy_minecraft_arguments_string() {
        let mut v = vanilla();
        v.arguments = None;
        v.minecraft_arguments = Some("--username ${auth_player_name} --version ${version_name} --unknown ${missing_var}".into());
        let (jvm, game) = build_args(&v, &vars(), &no_features(), &linux());
        assert_eq!(game, ["--username", "Steve", "--version", "fabric-loader-0.16.9-1.21.1", "--unknown", "${missing_var}"]);
        assert_eq!(jvm, ["-Djava.library.path=/n", "-cp", "/a.jar:/b.jar"]);
    }

    #[test]
    fn split_args_honours_double_quotes() {
        assert_eq!(split_args("-Xss1M \"-Dname=a b\" -XX:+UseG1GC"), ["-Xss1M", "-Dname=a b", "-XX:+UseG1GC"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn offline_uuid_matches_what_minecraft_derives() {
        let dashed = |s: String| format!("{}-{}-{}-{}-{}", &s[0..8], &s[8..12], &s[12..16], &s[16..20], &s[20..32]);
        assert_eq!(dashed(offline_uuid("Notch")), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(dashed(offline_uuid("Steve")), "5627dd98-e6be-3c21-b8a8-e92344183641");
        assert_eq!(offline_uuid("Steve").len(), 32);
    }

    #[test]
    fn username_validation() {
        assert!(is_valid_username("Good_Name1"));
        assert!(!is_valid_username("ab"));
        assert!(!is_valid_username("way_too_long_username"));
        assert!(!is_valid_username("bad name"));
        assert!(!is_valid_username(""));
    }
}
