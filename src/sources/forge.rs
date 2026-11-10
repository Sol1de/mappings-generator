//! Forge, NeoForge and MCP: builds per version, installer `install_profile.json` (read, never
//! executed), MCP SRG/MCPConfig tables and MCP CSV names.

use super::{Artifact, Cache, Expected, Result, SourceError, maven_versions, version_cmp};
use serde_json::Value;
use std::collections::BTreeMap;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const NEO_MAVEN: &str = "https://maven.neoforged.net/releases";
const LGPL: &str = "LGPL-2.1-only";
const MCP_LICENSE: &str = "MCP (no redistribution)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Forge,
    NeoForge,
}

impl Flavor {
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Forge => "forge",
            Flavor::NeoForge => "neoforge",
        }
    }
}

/// One loader build: its Maven coordinates and whether it is a stable release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub version: String,
    pub group_path: &'static str,
    pub artifact: &'static str,
    pub stable: bool,
}

impl Build {
    pub fn installer_url(&self) -> String {
        let base = if self.group_path.starts_with("net/neoforged") {
            NEO_MAVEN
        } else {
            FORGE_MAVEN
        };
        format!(
            "{base}/{}/{}/{v}/{}-{v}-installer.jar",
            self.group_path,
            self.artifact,
            self.artifact,
            v = self.version
        )
    }
}

/// Minecraft version of a `net.neoforged:neoforge` build: `21.0.x` → `1.21`, `21.11.x` →
/// `1.21.11`, `26.1.0.x` → `26.1`, `26.1.2.x` → `26.1.2`.
pub fn neoforge_minecraft(version: &str) -> Option<String> {
    let parts: Vec<&str> = version.split(['-', '+']).next()?.split('.').collect();
    let num = |i: usize| parts.get(i).and_then(|p| p.parse::<u32>().ok());
    let (a, b) = (num(0)?, num(1)?);
    if a < 25 {
        num(2)?;
        Some(if b == 0 {
            format!("1.{a}")
        } else {
            format!("1.{a}.{b}")
        })
    } else {
        let c = num(2)?;
        num(3)?;
        Some(if c == 0 {
            format!("{a}.{b}")
        } else {
            format!("{a}.{b}.{c}")
        })
    }
}

/// Builds per Minecraft version, newest first.
pub fn builds(cache: &Cache, flavor: Flavor) -> Result<BTreeMap<String, Vec<Build>>> {
    let mut out: BTreeMap<String, Vec<Build>> = BTreeMap::new();
    let meta = |id: &str, url: String| -> Result<Vec<String>> {
        Ok(maven_versions(
            &cache.fetch(id, &url, Expected::Mutable, "")?.text()?,
        ))
    };
    match flavor {
        Flavor::Forge => {
            for v in meta(
                "forge.metadata",
                format!("{FORGE_MAVEN}/net/minecraftforge/forge/maven-metadata.xml"),
            )? {
                if let Some((mc, _)) = v.split_once('-') {
                    let b = Build {
                        version: v.clone(),
                        group_path: "net/minecraftforge",
                        artifact: "forge",
                        stable: true,
                    };
                    out.entry(mc.to_owned()).or_default().push(b);
                }
            }
        }
        Flavor::NeoForge => {
            for v in meta(
                "neoforge.metadata.forge",
                format!("{NEO_MAVEN}/net/neoforged/forge/maven-metadata.xml"),
            )? {
                if let Some((mc, _)) = v.split_once('-') {
                    let b = Build {
                        version: v.clone(),
                        group_path: "net/neoforged",
                        artifact: "forge",
                        stable: true,
                    };
                    out.entry(mc.to_owned()).or_default().push(b);
                }
            }
            for v in meta(
                "neoforge.metadata",
                format!("{NEO_MAVEN}/net/neoforged/neoforge/maven-metadata.xml"),
            )? {
                if v.contains("+snapshot") || v.contains("alpha") {
                    continue; // builds for snapshots and pre-releases
                }
                if let Some(mc) = neoforge_minecraft(&v) {
                    let stable = !v.contains('-');
                    out.entry(mc).or_default().push(Build {
                        version: v,
                        group_path: "net/neoforged",
                        artifact: "neoforge",
                        stable,
                    });
                }
            }
        }
    }
    // Forge-style versions are `<mc>-<build>`: order by build.
    let build = |b: &Build| {
        if b.artifact == "forge" {
            b.version
                .split_once('-')
                .map_or(b.version.clone(), |x| x.1.to_owned())
        } else {
            b.version.clone()
        }
    };
    for list in out.values_mut() {
        list.sort_by(|a, b| version_cmp(&build(b), &build(a)));
    }
    Ok(out)
}

/// Forge promotions: `<mc>-recommended` / `<mc>-latest` → build number.
pub fn forge_promotions(cache: &Cache) -> Result<BTreeMap<String, String>> {
    let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let json = cache
        .fetch("forge.promotions", url, Expected::Mutable, "")?
        .json()?;
    Ok(json["promos"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default())
}

/// Default build: Forge recommended, then Forge latest, then the newest stable build.
pub fn default_build<'a>(
    mc: &str,
    builds: &'a [Build],
    promos: &BTreeMap<String, String>,
) -> Option<&'a Build> {
    for key in ["recommended", "latest"] {
        if let Some(n) = promos.get(&format!("{mc}-{key}"))
            && let Some(b) = builds
                .iter()
                .find(|b| b.artifact == "forge" && b.version.split('-').nth(1) == Some(n))
        {
            return Some(b);
        }
    }
    builds.iter().find(|b| b.stable).or(builds.first())
}

pub fn installer(cache: &Cache, build: &Build) -> Result<Artifact> {
    let id = format!(
        "{}.installer",
        if build.group_path == "net/minecraftforge" {
            "forge"
        } else {
            "neoforge"
        }
    );
    cache.maven(&id, &build.installer_url(), LGPL)
}

pub fn install_profile(installer: &Artifact) -> Result<Value> {
    serde_json::from_slice(&installer.zip_entry("install_profile.json")?)
        .map_err(|e| SourceError::Format(format!("install_profile.json: {e}")))
}

/// Runtime namespaces `(classes, members)` declared by an install profile's client side
/// (spec/namespaces.md#derivation-for-official-launcher).
pub fn derive_runtime(
    profile: &Value,
) -> std::result::Result<(&'static str, &'static str), String> {
    if profile.get("install").is_some() && profile.get("versionInfo").is_some() {
        return Ok(("srg", "srg")); // legacy profile: FML deobfuscation data
    }
    let processors: Vec<Vec<&str>> = profile["processors"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .filter(|p| {
                    p["sides"]
                        .as_array()
                        .is_none_or(|s| s.iter().any(|x| x == "client"))
                })
                .map(|p| {
                    p["args"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let has = |args: &[&str], flag: &str| args.contains(&flag);
    if processors.is_empty() && profile["spec"] == 0 {
        return Ok(("srg", "srg"));
    }
    if let Some(merge) = processors.iter().find(|a| has(a, "MERGE_MAPPING")) {
        return match (
            has(merge, "--classes"),
            has(merge, "--fields") && has(merge, "--methods"),
        ) {
            (true, true) => Ok(("mojang", "mojang")),
            (true, false) => Ok(("mojang", "srg")),
            _ => Err("unknown install profile: MERGE_MAPPING without --classes".into()),
        };
    }
    let pair = |args: &[&str], flag: &str, value: &str| {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    };
    if processors.iter().any(|a| pair(a, "--srg-in", "{MAPPINGS}")) {
        return Ok(("srg", "srg"));
    }
    if processors
        .iter()
        .any(|a| pair(a, "--names", "{MOJMAPS}") || has(a, "PROCESS_MINECRAFT_JAR"))
    {
        return Ok(("mojang", "mojang"));
    }
    if !processors.is_empty()
        && processors
            .iter()
            .all(|a| has(a, "--apply") || has(a, "DOWNLOAD_MOJMAPS"))
    {
        return Ok(("mojang", "mojang")); // binary patches only: unobfuscated game
    }
    Err("unknown install profile".into())
}

/// MCPConfig version referenced by `data.MAPPINGS` (e.g. `1.16.5-20210115.111550`).
pub fn mcp_config_version(profile: &Value) -> Option<String> {
    let coord = profile["data"]["MAPPINGS"]["client"].as_str()?;
    let rest = coord
        .trim_matches(['[', ']'])
        .strip_prefix("de.oceanlabs.mcp:mcp_config:")?;
    Some(rest.split(':').next()?.to_owned())
}

pub fn mcp_config(cache: &Cache, version: &str) -> Result<Artifact> {
    let url =
        format!("{FORGE_MAVEN}/de/oceanlabs/mcp/mcp_config/{version}/mcp_config-{version}.zip");
    cache.maven("mcp.mcp_config", &url, MCP_LICENSE)
}

/// Legacy MCP SRG zip (`joined.srg`), 1.7.10 → 1.12.2.
pub fn mcp_srg(cache: &Cache, mc: &str) -> Result<Artifact> {
    cache.maven(
        "mcp.srg",
        &format!("{FORGE_MAVEN}/de/oceanlabs/mcp/mcp/{mc}/mcp-{mc}-srg.zip"),
        MCP_LICENSE,
    )
}

/// MCP name channel (`fields.csv`, `methods.csv`): newest `mcp_stable` for the exact version,
/// else for its `major.minor` line, then the same for `mcp_snapshot`. `None` if no channel exists.
pub fn mcp_names(cache: &Cache, mc: &str) -> Result<Option<Artifact>> {
    let line: String = mc.split('.').take(2).collect::<Vec<_>>().join(".");
    for channel in ["mcp_stable", "mcp_snapshot"] {
        let url = format!("{FORGE_MAVEN}/de/oceanlabs/mcp/{channel}/maven-metadata.xml");
        let versions = maven_versions(
            &cache
                .fetch(
                    &format!("mcp.{channel}.metadata"),
                    &url,
                    Expected::Mutable,
                    "",
                )?
                .text()?,
        );
        for suffix in [mc, line.as_str()] {
            let mut matching: Vec<&String> = versions
                .iter()
                .filter(|v| v.rsplit_once('-').is_some_and(|(_, m)| m == suffix))
                .collect();
            matching.sort_by(|a, b| version_cmp(b, a));
            if let Some(v) = matching.first() {
                let url = format!("{FORGE_MAVEN}/de/oceanlabs/mcp/{channel}/{v}/{channel}-{v}.zip");
                return cache
                    .maven(&format!("mcp.{channel}"), &url, MCP_LICENSE)
                    .map(Some);
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proc(args: &[&str]) -> Value {
        json!({"jar": "x", "args": args})
    }

    #[test]
    fn neoforge_versions() {
        assert_eq!(neoforge_minecraft("21.0.166").as_deref(), Some("1.21"));
        assert_eq!(
            neoforge_minecraft("21.11.12-beta").as_deref(),
            Some("1.21.11")
        );
        assert_eq!(
            neoforge_minecraft("20.2.53-beta").as_deref(),
            Some("1.20.2")
        );
        assert_eq!(neoforge_minecraft("26.1.0.8-beta").as_deref(), Some("26.1"));
        assert_eq!(neoforge_minecraft("26.1.2.96").as_deref(), Some("26.1.2"));
        assert_eq!(neoforge_minecraft("nope"), None);
    }

    #[test]
    fn derives_every_era() {
        assert_eq!(
            derive_runtime(&json!({"install": {}, "versionInfo": {}})).unwrap(),
            ("srg", "srg")
        );
        assert_eq!(
            derive_runtime(&json!({"spec": 0, "processors": []})).unwrap(),
            ("srg", "srg")
        );
        let p116 = json!({"spec": 0, "processors": [proc(&["--in-jar", "{MC_SLIM}", "--out-jar", "{MC_SRG}", "--srg-in", "{MAPPINGS}"])]});
        assert_eq!(derive_runtime(&p116).unwrap(), ("srg", "srg"));
        let p120 = json!({"spec": 1, "processors": [
            proc(&["--task", "MERGE_MAPPING", "--left", "{MAPPINGS}", "--right", "{MOJMAPS}", "--output", "{MERGED_MAPPINGS}", "--classes", "--reverse-right"]),
            proc(&["--input", "{MC_SLIM}", "--names", "{MERGED_MAPPINGS}"])]});
        assert_eq!(derive_runtime(&p120).unwrap(), ("mojang", "srg"));
        let merged_all = json!({"spec": 1, "processors": [proc(&["--task", "MERGE_MAPPING", "--classes", "--fields", "--methods"])]});
        assert_eq!(derive_runtime(&merged_all).unwrap(), ("mojang", "mojang"));
        let p121 = json!({"spec": 1, "processors": [
            {"sides": ["server"], "args": ["--srg-in", "{MAPPINGS}"]},
            proc(&["--task", "DOWNLOAD_MOJMAPS", "--output", "{MOJMAPS}"]),
            proc(&["--input", "{MINECRAFT_JAR}", "--output", "{MC_OFF}", "--names", "{MOJMAPS}"]),
            proc(&["--clean", "{MC_OFF}", "--apply", "{BINPATCH}"])]});
        assert_eq!(derive_runtime(&p121).unwrap(), ("mojang", "mojang"));
        let p26 = json!({"spec": 1, "processors": [proc(&["--clean", "{MINECRAFT_JAR}", "--apply", "{BINPATCH}"])]});
        assert_eq!(derive_runtime(&p26).unwrap(), ("mojang", "mojang"));
        assert!(derive_runtime(&json!({"spec": 1, "processors": [proc(&["--weird"])]})).is_err());
    }

    #[test]
    fn mcp_config_coordinates() {
        let p = json!({"data": {"MAPPINGS": {"client": "[de.oceanlabs.mcp:mcp_config:1.16.5-20210115.111550:mappings@txt]"}}});
        assert_eq!(
            mcp_config_version(&p).as_deref(),
            Some("1.16.5-20210115.111550")
        );
        assert_eq!(mcp_config_version(&json!({})), None);
    }

    #[test]
    fn default_build_uses_promotions() {
        let b = |v: &str| Build {
            version: v.into(),
            group_path: "net/minecraftforge",
            artifact: "forge",
            stable: true,
        };
        let builds = vec![b("1.21.11-61.2.5"), b("1.21.11-61.2.4")];
        let promos = BTreeMap::from([("1.21.11-recommended".to_string(), "61.2.4".to_string())]);
        assert_eq!(
            default_build("1.21.11", &builds, &promos).unwrap().version,
            "1.21.11-61.2.4"
        );
        assert_eq!(
            default_build("1.21.11", &builds, &BTreeMap::new())
                .unwrap()
                .version,
            "1.21.11-61.2.5"
        );
        assert_eq!(
            b("1.7.10-10.13.4.1614-1.7.10").installer_url(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar"
        );
    }
}
