//! Fabric and Quilt: supported game versions, loader builds, launch profiles, Intermediary and Yarn.

use super::{Artifact, Cache, Expected, Result, SourceError, version_cmp};
use serde_json::Value;

const CC0: &str = "CC0-1.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Fabric,
    Quilt,
}

impl Flavor {
    fn meta(self) -> &'static str {
        match self {
            Flavor::Fabric => "https://meta.fabricmc.net/v2/versions",
            Flavor::Quilt => "https://meta.quiltmc.org/v3/versions",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Fabric => "fabric",
            Flavor::Quilt => "quilt",
        }
    }
}

fn list(a: &Artifact) -> Result<Vec<Value>> {
    a.json()?
        .as_array()
        .cloned()
        .ok_or_else(|| SourceError::Format(format!("{}: expected an array", a.url)))
}

/// Stable game versions declared by the loader metadata.
pub fn game_versions(cache: &Cache, flavor: Flavor) -> Result<Vec<String>> {
    let a = cache.fetch(
        &format!("{}.game", flavor.name()),
        &format!("{}/game", flavor.meta()),
        Expected::Mutable,
        "",
    )?;
    Ok(list(&a)?
        .iter()
        .filter(|v| v["stable"] == true)
        .filter_map(|v| v["version"].as_str().map(str::to_owned))
        .collect())
}

/// Loader builds for a game version, newest first; the flag tells whether the build is stable.
pub fn loaders(cache: &Cache, flavor: Flavor, game: &str) -> Result<Vec<(String, bool)>> {
    let url = format!("{}/loader/{game}", flavor.meta());
    let a = match cache.fetch(
        &format!("{}.loaders", flavor.name()),
        &url,
        Expected::Mutable,
        "",
    ) {
        Err(SourceError::NotFound(_)) => return Ok(Vec::new()),
        other => other?,
    };
    let mut out: Vec<(String, bool)> = list(&a)?
        .iter()
        .filter_map(|e| {
            let v = e["loader"]["version"].as_str()?;
            // Quilt Meta has no `stable` flag: pre-releases carry a `-beta`/`-pre` suffix.
            let stable = e["loader"]["stable"].as_bool().unwrap_or(!v.contains('-'));
            Some((v.to_owned(), stable))
        })
        .collect();
    out.sort_by(|a, b| version_cmp(&b.0, &a.0));
    Ok(out)
}

/// Default loader build: latest stable, else latest.
pub fn default_loader(builds: &[(String, bool)]) -> Option<&str> {
    builds
        .iter()
        .find(|b| b.1)
        .or(builds.first())
        .map(|b| b.0.as_str())
}

/// Launch profile of a loader build.
pub fn profile(cache: &Cache, flavor: Flavor, game: &str, loader: &str) -> Result<Artifact> {
    let url = format!("{}/loader/{game}/{loader}/profile/json", flavor.meta());
    cache.fetch(
        &format!("{}.profile", flavor.name()),
        &url,
        Expected::Unpublished,
        "",
    )
}

/// Intermediary version referenced by a launch profile's libraries; the `0.0.0` placeholder
/// published for unobfuscated versions counts as absent.
pub fn profile_intermediary(profile: &Value) -> Option<String> {
    profile["libraries"].as_array()?.iter().find_map(|l| {
        let v = l["name"]
            .as_str()?
            .strip_prefix("net.fabricmc:intermediary:")?;
        (v != "0.0.0").then(|| v.to_owned())
    })
}

pub fn intermediary(cache: &Cache, version: &str) -> Result<Artifact> {
    let url = format!(
        "https://maven.fabricmc.net/net/fabricmc/intermediary/{version}/intermediary-{version}-v2.jar"
    );
    cache.maven("fabric.intermediary", &url, CC0)
}

/// Latest stable Yarn build (merged `official`/`intermediary`/`named` Tiny v2), if any.
pub fn yarn(cache: &Cache, game: &str) -> Result<Option<Artifact>> {
    let url = format!("{}/yarn/{game}", Flavor::Fabric.meta());
    let builds = list(&cache.fetch("fabric.yarn.builds", &url, Expected::Mutable, "")?)?;
    let pick = builds
        .iter()
        .find(|b| b["stable"] == true)
        .or(builds.first());
    let Some(version) = pick.and_then(|b| b["version"].as_str()) else {
        return Ok(None);
    };
    let url = format!(
        "https://maven.fabricmc.net/net/fabricmc/yarn/{version}/yarn-{version}-mergedv2.jar"
    );
    match cache.maven("fabric.yarn", &url, CC0) {
        Ok(a) => Ok(Some(a)),
        Err(SourceError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_intermediary_is_absent() {
        let p = serde_json::json!({"libraries": [{"name": "net.fabricmc:intermediary:0.0.0"}, {"name": "net.fabricmc:fabric-loader:0.18.0"}]});
        assert_eq!(profile_intermediary(&p), None);
        let p = serde_json::json!({"libraries": [{"name": "net.fabricmc:intermediary:1.21.11"}]});
        assert_eq!(profile_intermediary(&p).as_deref(), Some("1.21.11"));
    }

    #[test]
    fn default_loader_prefers_stable() {
        let b = vec![
            ("0.30.0-beta.1".to_string(), false),
            ("0.29.2".to_string(), true),
        ];
        assert_eq!(default_loader(&b), Some("0.29.2"));
        assert_eq!(default_loader(&[]), None);
    }
}
