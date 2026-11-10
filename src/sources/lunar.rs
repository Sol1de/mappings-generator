//! Lunar Client, read from a local installation only (spec/sources.md#lunar-metadata).
//!
//! Lunar remaps and transforms the game at launch and stores the resulting classes in a
//! "bake" cache (`offline/multiver/cache/<a>/<b>/bake.zip`). That file is the runtime jar of a
//! Lunar configuration. Each launch is identified from `profiles/*/logs/ichor-boot.log`
//! (classpath, bake path, Fabric loader) cross-checked with the launcher log (version, module).
//! `scan` copies each identified bake into the generator cache with a record, so configurations
//! launched one after the other stay available. No network request, no account or hardware data.

use super::{Artifact, Cache, Result, SourceError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LICENSE: &str = "Lunar Client (no redistribution)";

/// A Lunar configuration captured from a launch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub minecraft_version: String,
    /// Lunar module: `lunar` (no loader) or `fabric`.
    pub module: String,
    pub loader_version: Option<String>,
    /// `lunar-platform-mappings-v*.jar` and other Lunar classpath jars with their checksums.
    pub classpath: Vec<String>,
    pub launched_at: String,
    pub bake: Artifact,
}

pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".lunarclient"))
}

/// One "Spawning JVM" block of the launcher log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// `YYYY-MM-DD HH:MM:SS`
    pub time: String,
    pub version: String,
    pub module: String,
    pub loader_version: Option<String>,
}

pub fn parse_launcher_log(text: &str) -> Vec<Launch> {
    let mut out = Vec::new();
    let mut current: Option<Launch> = None;
    for line in text.lines() {
        if line.contains("[Launch] Spawning JVM process") {
            if let Some(l) = current.take() {
                out.push(l);
            }
            let time = line.get(1..20).unwrap_or("").to_owned();
            current = Some(Launch {
                time,
                ..Launch::default()
            });
            continue;
        }
        let Some(l) = current.as_mut() else { continue };
        let value = |key: &str| {
            line.split_once(&format!("* {key}: "))
                .map(|(_, v)| v.trim().to_owned())
        };
        if let Some(v) = value("Subversion") {
            l.version = v;
        } else if let Some(v) = value("Module") {
            l.module = v;
        } else if let Some(v) = value("Loader Version") {
            l.loader_version = (v != "N/A").then_some(v);
        } else if line.contains("[Launch] System properties")
            || line.contains("JVM process started")
        {
            out.push(current.take().unwrap());
        }
    }
    out.extend(current);
    out
}

/// What `ichor-boot.log` tells about one launch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Boot {
    /// `HH:MM:SS` of the first line.
    pub time: String,
    pub classpath: Vec<String>,
    pub bake: Option<PathBuf>,
    pub fabric_loader: bool,
}

pub fn parse_boot_log(text: &str) -> Boot {
    let mut boot = Boot::default();
    for line in text.lines() {
        if boot.time.is_empty() && line.starts_with('[') {
            boot.time = line.get(1..9).unwrap_or("").to_owned();
        }
        if let Some((_, rest)) = line.split_once("Found classpath URL: ") {
            let file = rest.rsplit(['\\', '/']).next().unwrap_or(rest);
            boot.classpath.push(file.trim().to_owned());
        }
        if let Some(i) = line.find(":\\").or_else(|| line.find(":/"))
            && line.contains("bake.zip")
        {
            let start = line[..i].rfind(' ').map_or(0, |s| s + 1);
            let end = line.find("bake.zip").unwrap() + "bake.zip".len();
            boot.bake = Some(PathBuf::from(&line[start..end]));
        }
        boot.fabric_loader |= line.contains("pre-launch library") && line.contains("fabric_loader");
    }
    boot
}

/// Version encoded in `lunar-platform-mappings-v1_21_11.jar ...` → `1.21.11`.
fn classpath_version(classpath: &[String]) -> Option<String> {
    classpath.iter().find_map(|c| {
        let v = c
            .strip_prefix("lunar-platform-mappings-v")?
            .split(".jar")
            .next()?;
        // `v1_21_0` names 1.21 (Mojang ids drop a zero patch).
        let v = v.replace('_', ".");
        Some(v.strip_suffix(".0").map_or(v.clone(), str::to_owned))
    })
}

/// Identifies the latest launch of each Lunar profile and copies its bake into the cache.
pub fn scan(cache: &Cache, dir: &Path) -> Result<Vec<Capture>> {
    let launcher_log = ["main.old.log", "main.log"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(dir.join("logs").join("launcher").join(f)).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let launches = parse_launcher_log(&launcher_log);
    let profiles = dir.join("profiles");
    let entries = std::fs::read_dir(&profiles)
        .map_err(|e| SourceError::Io(format!("{}: {e}", profiles.display())))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path().join("logs").join("ichor-boot.log"))
        else {
            continue;
        };
        let boot = parse_boot_log(&text);
        let Some(bake) = boot.bake.clone().filter(|b| b.exists()) else {
            continue;
        };
        // The launch whose time of day matches the boot log, latest first.
        let launch = launches.iter().rev().find(|l| {
            l.time.get(11..19).is_some_and(|t| t <= boot.time.as_str())
                && within(&l.time[11..19], &boot.time, 120)
        });
        let Some(launch) = launch else {
            // Older launch whose launcher log entry has rotated away: cannot be identified.
            eprintln!(
                "warning: {}: no launcher log entry matches the last launch; skipped",
                entry.path().display()
            );
            continue;
        };
        let cp_version = classpath_version(&boot.classpath);
        let module_ok = (launch.module == "fabric") == boot.fabric_loader;
        // Lunar names some platform jars by version line (`v1_8` for 1.8.9).
        let cp_ok = cp_version
            .as_deref()
            .is_none_or(|v| v == launch.version || launch.version.starts_with(&format!("{v}.")));
        if !cp_ok || !module_ok {
            return Err(SourceError::Format(format!(
                "{}: launcher log ({} {}) disagrees with the boot log ({cp_version:?}, fabric loader: {})",
                entry.path().display(),
                launch.version,
                launch.module,
                boot.fabric_loader
            )));
        }
        let bytes = std::fs::read(&bake)
            .map_err(|e| SourceError::Io(format!("{}: {e}", bake.display())))?;
        let (sha, path) = cache.store_bytes(&bytes)?;
        let artifact = Artifact {
            id: "lunar.bake".into(),
            url: portable_url(&bake.display().to_string()),
            size: bytes.len() as u64,
            sha256: sha,
            license: LICENSE.into(),
            path,
        };
        let capture = Capture {
            minecraft_version: launch.version.clone(),
            module: if launch.module == "fabric" {
                "fabric".into()
            } else {
                "lunar".into()
            },
            loader_version: launch
                .loader_version
                .clone()
                .filter(|_| launch.module == "fabric"),
            classpath: boot.classpath.clone(),
            launched_at: launch.time.clone(),
            bake: artifact,
        };
        save(cache, &capture)?;
        out.push(capture);
    }
    Ok(out)
}

/// `lunar:<path inside the Lunar directory>`: provenance must not carry machine paths.
pub fn portable_url(path: &str) -> String {
    let path = path.replace('\\', "/");
    match path.find(".lunarclient/") {
        Some(i) => format!("lunar:{}", &path[i + ".lunarclient/".len()..]),
        None => path,
    }
}

fn within(a: &str, b: &str, seconds: i64) -> bool {
    let secs = |t: &str| -> Option<i64> {
        let mut p = t.split(':').map(|x| x.parse::<i64>().ok());
        Some(p.next()?? * 3600 + p.next()?? * 60 + p.next()??)
    };
    matches!((secs(a), secs(b)), (Some(x), Some(y)) if (y - x).abs() <= seconds)
}

fn record_path(cache: &Cache, version: &str, module: &str) -> PathBuf {
    cache
        .dir
        .join("lunar")
        .join(format!("{version}-{module}.json"))
}

fn save(cache: &Cache, capture: &Capture) -> Result<()> {
    let path = record_path(cache, &capture.minecraft_version, &capture.module);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| SourceError::Io(e.to_string()))?;
    let mut value = serde_json::to_value(capture).map_err(|e| SourceError::Io(e.to_string()))?;
    value["bake"]["path"] = serde_json::Value::Null;
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap())
        .map_err(|e| SourceError::Io(format!("{}: {e}", path.display())))
}

/// Captured Lunar configurations available in the cache.
pub fn captures(cache: &Cache) -> Result<Vec<Capture>> {
    let dir = cache.dir.join("lunar");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let text = std::fs::read(e.path()).map_err(|err| SourceError::Io(err.to_string()))?;
        let mut value: serde_json::Value = serde_json::from_slice(&text)
            .map_err(|err| SourceError::Format(format!("{}: {err}", e.path().display())))?;
        if let Some(b) = value.get_mut("bake").and_then(|b| b.as_object_mut()) {
            b.remove("path");
        }
        let mut c: Capture = serde_json::from_value(value)
            .map_err(|err| SourceError::Format(format!("{}: {err}", e.path().display())))?;
        c.bake.path = cache.object_path(&c.bake.sha256);
        c.bake.url = portable_url(&c.bake.url);
        out.push(c);
    }
    out.sort_by(|a, b| (&a.minecraft_version, &a.module).cmp(&(&b.minecraft_version, &b.module)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCHER: &str = "[2026-11-05 17:06:45.615] [info]  [Launch] Spawning JVM process with following configuration...
[2026-11-05 17:06:45.617] [info]           * Version: 1.21
[2026-11-05 17:06:45.617] [info]           * Subversion: 1.21.11
[2026-11-05 17:06:45.617] [info]           * Module: fabric
[2026-11-05 17:06:45.618] [info]           * Loader Version: 0.18.1
[2026-11-05 17:06:45.620] [info]  [Launch] System properties: [...]
";
    const BOOT: &str = r"[17:06:45] [Genesis/main/INFO] Found classpath URL: C:\Users\u\.lunarclient\offline\multiver\lunar-platform-mappings-v1_21_11.jar with checksum 302edf01
[17:06:47] [Genesis/main/INFO] Adding pre-launch library to game classpath: .\.ichor\ichormodule-libs\net_fabricmc_fabric_loader_0_20_1-lunar-0.1.5.jar
[17:06:49] [Genesis/Baker/main/INFO] Missing class cache: C:\Users\u\.lunarclient\offline\multiver\cache\914c23a3\73bb9564\bake.zip
";

    #[test]
    fn parses_logs() {
        let l = parse_launcher_log(LAUNCHER);
        assert_eq!(
            l,
            [Launch {
                time: "2026-11-05 17:06:45".into(),
                version: "1.21.11".into(),
                module: "fabric".into(),
                loader_version: Some("0.18.1".into())
            }]
        );
        let b = parse_boot_log(BOOT);
        assert_eq!(b.time, "17:06:45");
        assert!(b.fabric_loader);
        assert_eq!(
            b.bake.unwrap(),
            PathBuf::from(
                r"C:\Users\u\.lunarclient\offline\multiver\cache\914c23a3\73bb9564\bake.zip"
            )
        );
        assert_eq!(classpath_version(&b.classpath).as_deref(), Some("1.21.11"));
        assert_eq!(
            classpath_version(&["lunar-platform-mappings-v1_21_0.jar with checksum 1".into()])
                .as_deref(),
            Some("1.21")
        );
        assert!(within("17:06:45", "17:07:30", 120));
        assert_eq!(
            portable_url(r"file:///C:/Users/u/.lunarclient/offline/multiver/cache/a/b/bake.zip"),
            "lunar:offline/multiver/cache/a/b/bake.zip"
        );
        assert_eq!(
            portable_url(r"C:\Users\u\.lunarclient\x\bake.zip"),
            "lunar:x/bake.zip"
        );
        assert!(!within("17:06:45", "18:07:30", 120));
    }
}
