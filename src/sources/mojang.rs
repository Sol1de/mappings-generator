//! Mojang: release list, version descriptors, client jar and `client_mappings`.

use super::{Artifact, Cache, Expected, Result, SourceError};
use serde_json::Value;

pub const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const FIRST_RELEASE: &str = "1.7.10";
const LICENSE: &str = "Mojang (no redistribution)";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub sha1: String,
}

#[derive(Clone, Debug)]
pub struct Version {
    pub client: Download,
    /// `None` before 1.14.4 and for unobfuscated versions (26.1+).
    pub client_mappings: Option<Download>,
    pub descriptor: Artifact,
}

fn field<'a>(v: &'a Value, k: &str, ctx: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| SourceError::Format(format!("{ctx}: missing {k}")))
}

/// Releases from 1.7.10 to the latest, ordered by `releaseTime`; ids kept verbatim.
pub fn releases_from(manifest: &Value) -> Result<Vec<Release>> {
    let list = manifest["versions"]
        .as_array()
        .ok_or_else(|| SourceError::Format("manifest: no versions".into()))?;
    let mut out = Vec::new();
    for v in list.iter().filter(|v| v["type"] == "release") {
        out.push(Release {
            id: field(v, "id", "manifest")?.to_owned(),
            url: field(v, "url", "manifest")?.to_owned(),
            sha1: field(v, "sha1", "manifest")?.to_owned(),
            release_time: field(v, "releaseTime", "manifest")?.to_owned(),
        });
    }
    out.sort_by(|a, b| a.release_time.cmp(&b.release_time));
    let first = out
        .iter()
        .position(|r| r.id == FIRST_RELEASE)
        .ok_or_else(|| SourceError::Format("manifest lacks 1.7.10".into()))?;
    Ok(out.split_off(first))
}

pub fn releases(cache: &Cache) -> Result<(Vec<Release>, Artifact)> {
    let a = cache.fetch(
        "mojang.version_manifest",
        MANIFEST,
        Expected::Mutable,
        "Mojang",
    )?;
    Ok((releases_from(&a.json()?)?, a))
}

pub fn version(cache: &Cache, release: &Release) -> Result<Version> {
    let descriptor = cache.fetch(
        "mojang.version",
        &release.url,
        Expected::Sha1(release.sha1.clone()),
        "Mojang",
    )?;
    let json = descriptor.json()?;
    let download = |k: &str| -> Result<Option<Download>> {
        let d = &json["downloads"][k];
        if d.is_null() {
            return Ok(None);
        }
        Ok(Some(Download {
            url: field(d, "url", &release.id)?.to_owned(),
            sha1: field(d, "sha1", &release.id)?.to_owned(),
        }))
    };
    let client = download("client")?
        .ok_or_else(|| SourceError::Format(format!("{}: no client download", release.id)))?;
    Ok(Version {
        client,
        client_mappings: download("client_mappings")?,
        descriptor,
    })
}

pub fn client_jar(cache: &Cache, v: &Version) -> Result<Artifact> {
    cache.fetch(
        "mojang.client",
        &v.client.url,
        Expected::Sha1(v.client.sha1.clone()),
        LICENSE,
    )
}

pub fn client_mappings(cache: &Cache, v: &Version) -> Result<Option<Artifact>> {
    v.client_mappings
        .as_ref()
        .map(|d| {
            cache.fetch(
                "mojang.client_mappings",
                &d.url,
                Expected::Sha1(d.sha1.clone()),
                LICENSE,
            )
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_are_filtered_sorted_and_verbatim() {
        let m = serde_json::json!({"versions": [
            {"id": "26.1", "type": "release", "url": "u3", "sha1": "s", "releaseTime": "2026-03-01T00:00:00+00:00"},
            {"id": "26.2-snapshot-1", "type": "snapshot", "url": "u", "sha1": "s", "releaseTime": "2026-04-01T00:00:00+00:00"},
            {"id": "1.21.11", "type": "release", "url": "u2", "sha1": "s", "releaseTime": "2025-12-01T00:00:00+00:00"},
            {"id": "1.7.10", "type": "release", "url": "u1", "sha1": "s", "releaseTime": "2014-06-26T00:00:00+00:00"},
            {"id": "1.7.2", "type": "release", "url": "u0", "sha1": "s", "releaseTime": "2013-10-25T00:00:00+00:00"},
            {"id": "27.1", "type": "release", "url": "u4", "sha1": "s", "releaseTime": "2027-03-01T00:00:00+00:00"}
        ]});
        let ids: Vec<_> = releases_from(&m)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(
            ids,
            ["1.7.10", "1.21.11", "26.1", "27.1"],
            "a new release needs no code change"
        );
    }
}
