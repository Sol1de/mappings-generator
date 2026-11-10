//! Generation pipeline (spec/pipeline.md): resolve, fetch, build, reconstruct, determine the
//! runtime namespace, verify, export.

pub mod detect;
pub mod matrix;
pub mod verify;

use crate::classfile::{self, Inventory};
use crate::formats::json::{self, Configuration, Runtime};
use crate::formats::{proguard, srg, tiny};
use crate::model::{Complete, Kind, MappingSet};
use crate::sources::{self, Artifact, Cache, SourceError, fabric, forge, lunar, mojang};
use serde::Serialize;
use serde_json::json;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Pipeline failure, mapped to the CLI exit codes (spec/cli.md#exit-codes).
#[derive(Debug)]
pub enum Error {
    /// Exit 1: generation or verification error.
    Generation(String),
    /// Exit 2: unknown configuration or invalid usage.
    Usage(String),
    /// Exit 3: network or cache error.
    Source(SourceError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Generation(m) | Error::Usage(m) => f.write_str(m),
            Error::Source(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<SourceError> for Error {
    fn from(e: SourceError) -> Self {
        Error::Source(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn gen_err(e: impl fmt::Display) -> Error {
    Error::Generation(e.to_string())
}

fn joined(errors: Vec<String>) -> Error {
    let n = errors.len();
    let mut shown: Vec<String> = errors.into_iter().take(20).collect();
    if n > 20 {
        shown.push(format!("... and {} more", n - 20));
    }
    Error::Generation(shown.join("\n  "))
}

fn tiny_from_jar(a: &Artifact) -> Result<MappingSet> {
    let text = String::from_utf8(a.zip_entry("mappings/mappings.tiny")?).map_err(gen_err)?;
    tiny::read(&text).map_err(gen_err)
}

/// Everything shared by the configurations of one Minecraft version: the client jar and every
/// mapping source keyed by `official`, reconstructed from the jar.
pub struct Base {
    pub version: String,
    pub obfuscated: bool,
    pub set: MappingSet,
    pub jar: Inventory,
    pub sources: Vec<Artifact>,
    pub warnings: Vec<String>,
    /// Symbols named by mapping sources but absent from the client jar (server-only, removed).
    pub pruned: usize,
}

/// Loads an MCP/MCPConfig SRG table keyed by `official`, field descriptors resolved by the jar.
fn load_srg(set: MappingSet, jar: &Inventory) -> Result<MappingSet> {
    let mut set = set;
    if set.namespaces[0] == "obf" {
        set.namespaces[0] = "official".into();
    }
    set.drop_ns("id");
    let problems = set.attach_jar(&jar.classes);
    if !problems.is_empty() {
        return Err(joined(problems));
    }
    Ok(set)
}

fn merge(base: &mut MappingSet, other: &MappingSet) -> Result<()> {
    base.merge(other).map_err(joined)
}

pub fn base(cache: &Cache, release: &mojang::Release) -> Result<Base> {
    let version = mojang::version(cache, release)?;
    let client = mojang::client_jar(cache, &version)?;
    let jar = classfile::inventory(std::fs::File::open(&client.path).map_err(gen_err)?)
        .map_err(gen_err)?;
    let mut sources = vec![version.descriptor.clone(), client];
    let mappings = mojang::client_mappings(cache, &version)?;
    let under_mc = jar
        .classes
        .keys()
        .filter(|k| k.starts_with("net/minecraft/"))
        .count();
    let obfuscated = mappings.is_some() || under_mc * 2 < jar.classes.len();
    let mut set = MappingSet::new(&["official"]);
    let mut warnings = Vec::new();
    if let Some(m) = mappings {
        let p = proguard::read(&m.text()?).map_err(gen_err)?;
        merge(&mut set, &p)?;
        sources.push(m);
    } else if !obfuscated {
        set.add_namespace("mojang");
    }
    if obfuscated {
        match fabric::intermediary(cache, &release.id) {
            Ok(a) => {
                merge(&mut set, &tiny_from_jar(&a)?)?;
                sources.push(a);
            }
            Err(SourceError::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(a) = fabric::yarn(cache, &release.id)? {
            let mut y = tiny_from_jar(&a)?;
            y.rename_ns("named", "yarn");
            merge(&mut set, &y)?;
            sources.push(a);
        }
        if set.ns("mojang").is_none() {
            // No Mojang names: MCP provides the named namespace where a channel exists.
            let table = match forge::mcp_srg(cache, &release.id) {
                Ok(a) => Some((
                    srg::read_srg(&String::from_utf8(a.zip_entry("joined.srg")?).map_err(gen_err)?)
                        .map_err(gen_err)?,
                    a,
                )),
                Err(SourceError::NotFound(_)) => mcp_config_latest(cache, &release.id)?,
                Err(e) => return Err(e.into()),
            };
            if let Some((table, a)) = table {
                let mut table = load_srg(table, &jar)?;
                sources.push(a);
                if let Some(names) = forge::mcp_names(cache, &release.id)? {
                    apply_mcp_names(&mut table, &names)?;
                    sources.push(names);
                }
                merge(&mut set, &table)?;
            }
        }
    }
    warnings.extend(set.attach_jar(&jar.classes));
    let pruned = prune(&mut set);
    warnings.extend(
        set.propagate()
            .into_iter()
            .map(|c| format!("propagation conflict: {c}")),
    );
    if !obfuscated {
        let m = set.ns("mojang").expect("added above");
        set.complete(m, Complete::Remap); // unobfuscated: Mojang names are the jar names
    }
    for i in 1..set.namespaces.len() {
        set.complete(i, Complete::Proven(&[]));
    }
    Ok(Base {
        version: release.id.clone(),
        obfuscated,
        set,
        jar,
        sources,
        warnings,
        pruned,
    })
}

fn mcp_config_latest(cache: &Cache, mc: &str) -> Result<Option<(MappingSet, Artifact)>> {
    let url = "https://maven.minecraftforge.net/de/oceanlabs/mcp/mcp_config/maven-metadata.xml";
    let versions = sources::maven_versions(
        &cache
            .fetch(
                "mcp.mcp_config.metadata",
                url,
                sources::Expected::Mutable,
                "",
            )?
            .text()?,
    );
    let Some(v) = versions
        .iter()
        .filter(|v| v.strip_prefix(mc).is_some_and(|r| r.starts_with('-')))
        .max_by(|a, b| sources::version_cmp(a, b))
    else {
        return Ok(None);
    };
    let a = forge::mcp_config(cache, v)?;
    let text = String::from_utf8(a.zip_entry("config/joined.tsrg")?).map_err(gen_err)?;
    Ok(Some((srg::read_tsrg(&text).map_err(gen_err)?, a)))
}

fn apply_mcp_names(table: &mut MappingSet, names: &Artifact) -> Result<()> {
    let csv = |n: &str| -> Result<_> {
        let text = String::from_utf8(names.zip_entry(n)?).map_err(gen_err)?;
        srg::read_mcp_csv(&text).map_err(gen_err)
    };
    srg::apply_mcp(table, &csv("fields.csv")?, &csv("methods.csv")?).map_err(gen_err)
}

/// Removes symbols that mapping sources name but the client jar does not contain.
fn prune(set: &mut MappingSet) -> usize {
    let before: usize = set
        .classes
        .values()
        .map(|c| 1 + c.fields.len() + c.methods.len())
        .sum();
    set.classes.retain(|_, c| c.access.is_some());
    for c in set.classes.values_mut() {
        c.fields.retain(|_, m| m.access.is_some());
        c.methods.retain(|_, m| m.access.is_some());
    }
    let after: usize = set
        .classes
        .values()
        .map(|c| 1 + c.fields.len() + c.methods.len())
        .sum();
    before - after
}

#[derive(Debug, Default, Serialize)]
pub struct Coverage {
    pub classes: f64,
    pub fields: f64,
    pub methods: f64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub configuration: Configuration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub pruned_not_in_client_jar: usize,
    pub coverage: Coverage,
    pub coverage_gaps: serde_json::Value,
    pub runtime_detection: serde_json::Value,
    /// Game classes missing from a runtime cache (Lunar): not exported, since unverifiable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_exported_absent_at_runtime: Vec<String>,
    /// Game members a runtime cache (Lunar) lacks under their mapped name and descriptor:
    /// removed or changed by the launcher's transformations, so not exported.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub members_not_exported_absent_at_runtime: Vec<String>,
}

pub struct Generated {
    pub configuration: Configuration,
    pub set: MappingSet,
    pub runtime: Runtime,
    pub named: Option<usize>,
    pub sources: Vec<Artifact>,
    pub detection: serde_json::Value,
    pub report: Report,
    /// Symbols of the runtime jars, for `verify`.
    pub runtime_symbols: verify::Symbols,
}

fn coverage(set: &MappingSet, named: Option<usize>) -> (Coverage, serde_json::Value) {
    let Some(n) = named else {
        return (Coverage::default(), json!({}));
    };
    let ratio = |hit: usize, total: usize| {
        if total == 0 {
            1.0
        } else {
            ((hit as f64 / total as f64) * 10_000.0).floor() / 10_000.0
        }
    };
    let (mut c, mut f, mut m) = ((0, 0), (0, 0), (0, 0));
    for class in set.classes.values().filter(|c| c.access.is_some()) {
        c.1 += 1;
        c.0 += usize::from(class.names[n].is_some());
        for (kind, acc) in [(Kind::Field, &mut f), (Kind::Method, &mut m)] {
            for member in class.members(kind).values().filter(|m| m.access.is_some()) {
                acc.1 += 1;
                acc.0 += usize::from(member.names[n].is_some());
            }
        }
    }
    let gaps = json!({ "classes": c.1 - c.0, "fields": f.1 - f.0, "methods": m.1 - m.0 });
    (
        Coverage {
            classes: ratio(c.0, c.1),
            fields: ratio(f.0, f.1),
            methods: ratio(m.0, m.1),
        },
        gaps,
    )
}

/// Generates one configuration from its version's base.
pub fn generate(cache: &Cache, base: &Base, cfg: &Configuration) -> Result<Generated> {
    let mut set = base.set.clone();
    let mut sources = base.sources.clone();
    let mut warnings = base.warnings.clone();
    let mut configuration = cfg.clone();
    let mut runtime_jar: Option<(Inventory, Vec<String>)> = None;
    let (classes_ns, members_ns, mut detection): (String, String, serde_json::Value) = match (
        cfg.launcher.as_str(),
        cfg.loader.as_str(),
    ) {
        ("official", "vanilla") => {
            let ns = if base.obfuscated {
                "official"
            } else {
                "mojang"
            };
            (
                ns.into(),
                ns.into(),
                json!({ "method": "derived", "evidence": if base.obfuscated { "obfuscated client jar" } else { "unobfuscated client jar" } }),
            )
        }
        ("official", loader @ ("fabric" | "quilt")) => {
            let flavor = if loader == "fabric" {
                fabric::Flavor::Fabric
            } else {
                fabric::Flavor::Quilt
            };
            let builds = fabric::loaders(cache, flavor, &base.version)?;
            let build = match &cfg.loader_version {
                Some(b) if builds.iter().any(|x| &x.0 == b) => b.clone(),
                Some(b) => {
                    return Err(Error::Usage(format!(
                        "{loader} {b} is not published for {}",
                        base.version
                    )));
                }
                None => fabric::default_loader(&builds)
                    .ok_or_else(|| Error::Usage(format!("no {loader} build for {}", base.version)))?
                    .to_owned(),
            };
            configuration.loader_version = Some(build.clone());
            let profile = fabric::profile(cache, flavor, &base.version, &build)?;
            let declared = fabric::profile_intermediary(&profile.json()?);
            sources.push(profile);
            let ns = match (&declared, base.obfuscated) {
                (Some(v), true) if set.ns("intermediary").is_some() => {
                    if v != &base.version {
                        return Err(gen_err(format!(
                            "{loader} profile declares intermediary {v} for {}",
                            base.version
                        )));
                    }
                    "intermediary"
                }
                (Some(v), true) => {
                    return Err(gen_err(format!(
                        "intermediary {v} is declared but not published"
                    )));
                }
                (None, false) => "mojang",
                (declared, _) => {
                    return Err(gen_err(format!(
                        "unexpected {loader} profile: intermediary {declared:?}, obfuscated {}",
                        base.obfuscated
                    )));
                }
            };
            (
                ns.into(),
                ns.into(),
                json!({ "method": "derived", "evidence": format!("{loader} launch profile libraries: intermediary {declared:?}") }),
            )
        }
        ("official", loader @ ("forge" | "neoforge")) => {
            let flavor = if loader == "forge" {
                forge::Flavor::Forge
            } else {
                forge::Flavor::NeoForge
            };
            let all = forge::builds(cache, flavor)?;
            let builds = all
                .get(&base.version)
                .ok_or_else(|| Error::Usage(format!("no {loader} build for {}", base.version)))?;
            let build = match &cfg.loader_version {
                Some(b) => builds.iter().find(|x| {
                    &x.version == b || x.version.split_once('-').is_some_and(|(_, n)| n == b)
                }),
                None => {
                    let promos = if flavor == forge::Flavor::Forge {
                        forge::forge_promotions(cache)?
                    } else {
                        Default::default()
                    };
                    forge::default_build(&base.version, builds, &promos)
                }
            }
            .ok_or_else(|| {
                Error::Usage(format!(
                    "{loader} build {:?} is not published for {}",
                    cfg.loader_version, base.version
                ))
            })?
            .clone();
            configuration.loader_version = Some(build.version.clone());
            let installer = forge::installer(cache, &build)?;
            let profile = forge::install_profile(&installer)?;
            sources.push(installer);
            let declared_mc = profile["minecraft"]
                .as_str()
                .or(profile["install"]["minecraft"].as_str());
            if declared_mc.is_some_and(|m| m != base.version) {
                return Err(gen_err(format!(
                    "{loader} {} targets Minecraft {declared_mc:?}",
                    build.version
                )));
            }
            let (c, m) = forge::derive_runtime(&profile).map_err(gen_err)?;
            if (c == "srg" || m == "srg") && set.ns("srg").is_none() {
                let version = forge::mcp_config_version(&profile)
                    .ok_or_else(|| gen_err("install profile names no MCPConfig"))?;
                let a = forge::mcp_config(cache, &version)?;
                let text =
                    String::from_utf8(a.zip_entry("config/joined.tsrg")?).map_err(gen_err)?;
                let mut table = load_srg(srg::read_tsrg(&text).map_err(gen_err)?, &base.jar)?;
                sources.push(a);
                if c == "srg"
                    && let Some(names) = forge::mcp_names(cache, &base.version)?
                {
                    apply_mcp_names(&mut table, &names)?;
                    sources.push(names);
                }
                merge(&mut set, &table)?;
                prune(&mut set);
                for i in 1..set.namespaces.len() {
                    set.complete(i, Complete::Proven(&[]));
                }
            }
            (
                c.into(),
                m.into(),
                json!({ "method": "derived", "evidence": format!("{loader} {} install_profile.json client processors", build.version) }),
            )
        }
        ("lunar", module @ ("lunar" | "fabric")) => {
            let capture = lunar::captures(cache)?
                .into_iter()
                .find(|c| c.minecraft_version == base.version && c.module == module)
                .ok_or_else(|| {
                    Error::Usage(format!(
                        "no captured Lunar {module} launch for {} (launch it, then run `scan`)",
                        base.version
                    ))
                })?;
            configuration.loader_version = capture.loader_version.clone();
            let inv =
                classfile::inventory(std::fs::File::open(&capture.bake.path).map_err(gen_err)?)
                    .map_err(gen_err)?;
            let verdict = detect::detect(
                &set,
                &inv.classes,
                &base.jar.fingerprints,
                &inv.fingerprints,
            )
            .map_err(gen_err)?;
            sources.push(capture.bake.clone());
            let classpath = capture.classpath.clone();
            let (c, m) = (verdict.classes.clone(), verdict.members.clone());
            runtime_jar = Some((inv, classpath.clone()));
            (
                c,
                m,
                json!({ "method": "detected", "verdict": verdict, "lunar_classpath": classpath }),
            )
        }
        (launcher, loader) => {
            return Err(Error::Usage(format!(
                "unknown configuration {launcher}/{loader}"
            )));
        }
    };
    let ci = set.ns(&classes_ns).ok_or_else(|| {
        gen_err(format!(
            "namespace {classes_ns} unavailable for {}",
            base.version
        ))
    })?;
    let mi = set.ns(&members_ns).ok_or_else(|| {
        gen_err(format!(
            "namespace {members_ns} unavailable for {}",
            base.version
        ))
    })?;
    // MCP names are applied over SRG: a symbol without an MCP name keeps its SRG name.
    if let (Some(mcp), Some(srg)) = (set.ns("mcp"), set.ns("srg"))
        && (ci == mcp || mi == mcp)
    {
        set.fill_from(mcp, srg);
    }
    // The runtime remapper keeps names its mappings do not cover.
    set.complete(ci, Complete::Remap);
    set.complete(mi, Complete::Remap);
    let mut runtime = Runtime {
        classes: ci,
        members: mi,
    };
    let mut errors = Vec::new();
    let named_of = |set: &MappingSet| {
        ["mojang", "mcp", "yarn"].iter().find_map(|n| {
            set.ns(n)
                .filter(|&i| set.classes.values().any(|c| c.names[i].is_some()))
        })
    };
    // Coverage of the game's own symbols (before Lunar's classes are added).
    let (cov, gaps) = coverage(&set, named_of(&set));
    let mut absent_at_runtime = Vec::new();
    let mut absent_members = Vec::new();
    let mut static_changed = Vec::new();
    let runtime_symbols = if let Some((inv, _)) = &runtime_jar {
        if ci != mi {
            return Err(gen_err(
                "mixed class/member namespaces are not supported for Lunar",
            ));
        }
        // Classes the runtime cache does not contain cannot be verified: they are not exported,
        // but stay in the set so descriptors that reference them are still remapped.
        absent_at_runtime = set
            .classes
            .values()
            .filter_map(|c| c.names[ci].clone())
            .filter(|n| !inv.classes.contains_key(n))
            .collect();
        // Members Lunar removed or changed while transforming the game do not exist at runtime:
        // they are not exported either (the runtime variants are attached below).
        let map: std::collections::HashMap<String, String> = set
            .class_map(ci)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        for c in set.classes.values_mut() {
            let Some((rt_name, rt)) = c.names[ci]
                .clone()
                .and_then(|n| inv.classes.get(&n).map(|r| (n, r)))
            else {
                continue;
            };
            for kind in [Kind::Field, Kind::Method] {
                let list = if kind == Kind::Field {
                    &rt.fields
                } else {
                    &rt.methods
                };
                c.members_mut(kind).retain(|k, m| {
                    let name = m.names[mi].as_deref().unwrap_or(&k.name);
                    let desc =
                        crate::model::remap_descriptor(&k.desc, |n| map.get(n).map(String::as_str));
                    let found = list.iter().find(|(n, d, _)| n == name && *d == desc);
                    match found {
                        None => absent_members.push(format!("{rt_name}.{name}{desc}")),
                        Some((_, _, a))
                            if m.access
                                .is_some_and(|x| (x ^ a) & crate::model::ACC_STATIC != 0) =>
                        {
                            static_changed.push(format!(
                                "static flag of {rt_name}.{name}{desc} changed by the launcher"
                            ));
                        }
                        Some(_) => {}
                    }
                    found.is_some()
                });
            }
        }
        let symbols = verify::Symbols::from_jar(&inv.classes);
        warnings.append(&mut static_changed);
        let (reordered, losses) = set.reorder(&classes_ns).map_err(gen_err)?;
        if !losses.is_empty() {
            return Err(joined(losses));
        }
        set = reordered;
        // Lunar's own classes and members, with their runtime access flags.
        warnings.extend(set.attach_jar(&inv.classes));
        // Verified with runtime access flags: the class cache is the runtime.
        let mut expected = verify::Symbols::from_set(&set, 0, 0, true);
        expected.0.retain(|k, _| inv.classes.contains_key(k));
        errors.extend(verify::check(&expected, &symbols));
        runtime = Runtime {
            classes: 0,
            members: 0,
        };
        symbols
    } else {
        // Official launcher: the runtime jar is the client jar remapped into the runtime namespace.
        let symbols = verify::Symbols::remap_jar(&base.jar.classes, &set, ci, mi);
        let expected = verify::Symbols::from_set(&set, ci, mi, true);
        errors.extend(verify::check(&expected, &symbols));
        symbols
    };
    errors.extend(set.validate());
    let named = named_of(&set);
    if let Some(obj) = detection.as_object_mut() {
        obj.insert("classes".into(), json!(classes_ns));
        obj.insert("members".into(), json!(members_ns));
    }
    let report = Report {
        configuration: configuration.clone(),
        errors,
        warnings,
        pruned_not_in_client_jar: base.pruned,
        coverage: cov,
        coverage_gaps: gaps,
        runtime_detection: detection.clone(),
        not_exported_absent_at_runtime: absent_at_runtime,
        members_not_exported_absent_at_runtime: absent_members,
    };
    Ok(Generated {
        configuration,
        set,
        runtime,
        named,
        sources,
        detection,
        report,
        runtime_symbols,
    })
}

/// Output options (spec/formats.md).
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub namespaces: Option<Vec<String>>,
    pub pretty: bool,
    pub gzip: bool,
}

pub fn file_stem(cfg: &Configuration) -> String {
    match &cfg.loader_version {
        Some(v) => format!("{}-{}-{v}", cfg.launcher, cfg.loader),
        None => format!("{}-{}", cfg.launcher, cfg.loader),
    }
}

/// Writes `<dir>/<version>/<launcher>-<loader>[-<build>].json` and its `.report.json`.
/// Verification errors block the export (only the report is written).
pub fn write(g: &Generated, dir: &Path, out: &Output) -> Result<Vec<PathBuf>> {
    let folder = dir.join(&g.configuration.minecraft_version);
    std::fs::create_dir_all(&folder).map_err(gen_err)?;
    let stem = file_stem(&g.configuration);
    let report_path = folder.join(format!("{stem}.report.json"));
    let mut report = serde_json::to_vec_pretty(&g.report).map_err(gen_err)?;
    report.push(b'\n');
    std::fs::write(&report_path, report).map_err(gen_err)?;
    if !g.report.errors.is_empty() {
        return Err(Error::Generation(format!(
            "{} verification errors for {} (see {}):\n  {}",
            g.report.errors.len(),
            stem,
            report_path.display(),
            g.report
                .errors
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        )));
    }
    let runtime_physical: Vec<usize> = if g.runtime.classes == g.runtime.members {
        vec![g.runtime.classes]
    } else {
        vec![]
    };
    let exported: Vec<usize> = (0..g.set.namespaces.len())
        .filter(|i| !runtime_physical.contains(i))
        .filter(|&i| {
            out.namespaces
                .as_ref()
                .is_none_or(|keep| keep.contains(&g.set.namespaces[i]))
        })
        .collect();
    let sources: Vec<serde_json::Value> = g
        .sources
        .iter()
        .map(|a| json!({ "id": a.id, "url": a.url, "size": a.size, "sha256": a.sha256, "license": a.license }))
        .collect();
    let provenance = json!({
        "generator": format!("mc_mappings_generator {}", env!("CARGO_PKG_VERSION")),
        "sources": sources,
        "runtime_detection": g.detection,
        "verification": { "errors": 0, "warnings": g.report.warnings.len() },
        "coverage": g.report.coverage,
    });
    let export = json::Export {
        configuration: &g.configuration,
        set: &g.set,
        runtime: g.runtime.clone(),
        named: g.named,
        exported,
        provenance: &provenance,
        omit: &g.report.not_exported_absent_at_runtime,
    };
    let path = folder.join(format!("{stem}.json{}", if out.gzip { ".gz" } else { "" }));
    let file = std::fs::File::create(&path).map_err(gen_err)?;
    let buffered = std::io::BufWriter::new(file);
    if out.gzip {
        // mtime 0 and no file name keep gzip output byte-identical.
        let mut gz = flate2::GzBuilder::new()
            .mtime(0)
            .write(buffered, flate2::Compression::default());
        json::write(&export, out.pretty, &mut gz).map_err(gen_err)?;
        gz.finish().map_err(gen_err)?.flush().map_err(gen_err)?;
    } else {
        let mut w = buffered;
        json::write(&export, out.pretty, &mut w).map_err(gen_err)?;
        w.flush().map_err(gen_err)?;
    }
    Ok(vec![path, report_path])
}
