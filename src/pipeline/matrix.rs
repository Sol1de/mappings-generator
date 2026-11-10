//! Configuration matrix (spec/scope.md#configuration-matrix): versions × launchers × loaders,
//! keeping only combinations declared by upstream metadata.

use super::{Error, Result};
use crate::formats::json::Configuration;
use crate::sources::{Cache, fabric, forge, lunar, mojang};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub configuration: Configuration,
    /// Expected runtime namespaces `(classes, members)`, when derivable without downloading
    /// a loader installer; `None` means "derived at generation".
    pub expected_runtime: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub version: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub loader: Option<String>,
    pub launcher: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    pub exclude: Vec<Exclusion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    pub minecraft_version: String,
    pub launcher: String,
    pub loader: String,
    pub reason: String,
}

/// Releases selected by `filter` (bounds by release order, ids verbatim).
pub fn releases(cache: &Cache, filter: &Filter) -> Result<Vec<mojang::Release>> {
    let (all, _) = mojang::releases(cache)?;
    let index = |v: &Option<String>| -> Result<Option<usize>> {
        v.as_ref()
            .map(|v| {
                all.iter()
                    .position(|r| &r.id == v)
                    .ok_or_else(|| Error::Usage(format!("unknown release {v}")))
            })
            .transpose()
    };
    let (since, until) = (index(&filter.since)?, index(&filter.until)?);
    if let Some(v) = &filter.version
        && !all.iter().any(|r| &r.id == v)
    {
        return Err(Error::Usage(format!(
            "{v} is not a supported release (releases from 1.7.10 only)"
        )));
    }
    Ok(all
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            since.is_none_or(|s| *i >= s)
                && until.is_none_or(|u| *i <= u)
                && filter.version.as_ref().is_none_or(|v| v == &r.id)
        })
        .map(|(_, r)| r.clone())
        .collect())
}

fn cfg(version: &str, launcher: &str, loader: &str, build: Option<String>) -> Configuration {
    Configuration {
        minecraft_version: version.into(),
        launcher: launcher.into(),
        loader: loader.into(),
        loader_version: build,
    }
}

pub fn matrix(cache: &Cache, filter: &Filter, overrides: &Overrides) -> Result<Vec<Entry>> {
    let releases = releases(cache, filter)?;
    let wants = |launcher: &str, loader: &str| {
        filter.launcher.as_deref().is_none_or(|l| l == launcher)
            && filter.loader.as_deref().is_none_or(|l| l == loader)
    };
    let mut out = Vec::new();
    // Obfuscation: versions with `client_mappings`, and every release before the first one that has them.
    let (all, _) = mojang::releases(cache)?;
    let mut obfuscated = BTreeMap::new();
    let mut first_mapped: Option<String> = None;
    for r in &releases {
        let v = mojang::version(cache, r)?;
        if v.client_mappings.is_some() && first_mapped.is_none() {
            first_mapped = Some(r.release_time.clone());
        }
        obfuscated.insert(r.id.clone(), v.client_mappings.is_some());
    }
    if first_mapped.is_none() {
        // Filtered window without mapped versions: look for the first mapped release overall.
        for r in &all {
            if mojang::version(cache, r)?.client_mappings.is_some() {
                first_mapped = Some(r.release_time.clone());
                break;
            }
        }
    }
    for r in &releases {
        if first_mapped.as_ref().is_some_and(|t| &r.release_time < t) {
            obfuscated.insert(r.id.clone(), true);
        }
    }
    let ids: BTreeSet<&str> = releases.iter().map(|r| r.id.as_str()).collect();
    if wants("official", "vanilla") {
        for r in &releases {
            let ns = if obfuscated[&r.id] {
                "official"
            } else {
                "mojang"
            };
            out.push(Entry {
                configuration: cfg(&r.id, "official", "vanilla", None),
                expected_runtime: Some((ns.into(), ns.into())),
                excluded: None,
            });
        }
    }
    for flavor in [fabric::Flavor::Fabric, fabric::Flavor::Quilt] {
        if !wants("official", flavor.name()) {
            continue;
        }
        let games: BTreeSet<String> = fabric::game_versions(cache, flavor)?.into_iter().collect();
        for r in releases.iter().filter(|r| games.contains(&r.id)) {
            let builds = fabric::loaders(cache, flavor, &r.id)?;
            let Some(build) = fabric::default_loader(&builds) else {
                continue;
            };
            let ns = if obfuscated[&r.id] {
                "intermediary"
            } else {
                "mojang"
            };
            out.push(Entry {
                configuration: cfg(&r.id, "official", flavor.name(), Some(build.into())),
                expected_runtime: Some((ns.into(), ns.into())),
                excluded: None,
            });
        }
    }
    for flavor in [forge::Flavor::Forge, forge::Flavor::NeoForge] {
        if !wants("official", flavor.name()) {
            continue;
        }
        let builds = forge::builds(cache, flavor)?;
        let promos = if flavor == forge::Flavor::Forge {
            forge::forge_promotions(cache)?
        } else {
            Default::default()
        };
        for r in &releases {
            let Some(list) = builds.get(&r.id) else {
                continue;
            };
            let Some(build) = forge::default_build(&r.id, list, &promos) else {
                continue;
            };
            let expected = cache
                .cached(&build.installer_url())
                .and_then(|a| forge::install_profile(&a).ok())
                .and_then(|p| forge::derive_runtime(&p).ok())
                .map(|(c, m)| (c.to_owned(), m.to_owned()));
            out.push(Entry {
                configuration: cfg(
                    &r.id,
                    "official",
                    flavor.name(),
                    Some(build.version.clone()),
                ),
                expected_runtime: expected,
                excluded: None,
            });
        }
    }
    for capture in lunar::captures(cache)? {
        if ids.contains(capture.minecraft_version.as_str()) && wants("lunar", &capture.module) {
            out.push(Entry {
                configuration: cfg(
                    &capture.minecraft_version,
                    "lunar",
                    &capture.module,
                    capture.loader_version.clone(),
                ),
                expected_runtime: Some(("detected".into(), "detected".into())),
                excluded: None,
            });
        }
    }
    for e in &mut out {
        let c = &e.configuration;
        if let Some(x) = overrides.exclude.iter().find(|x| {
            (&x.minecraft_version, &x.launcher, &x.loader)
                == (&c.minecraft_version, &c.launcher, &c.loader)
        }) {
            e.excluded = Some(x.reason.clone());
        }
    }
    let order: BTreeMap<&str, usize> = all
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.as_str(), i))
        .collect();
    out.sort_by(|a, b| {
        let key = |e: &Entry| {
            (
                order[e.configuration.minecraft_version.as_str()],
                e.configuration.launcher.clone(),
                e.configuration.loader.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    Ok(out)
}
