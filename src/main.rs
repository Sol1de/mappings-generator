//! Minecraft Java Edition mappings generator. See `spec/`.
//!
//! Module call rule (by convention): main → pipeline → sources / formats / classfile → model.

mod classfile;
mod formats;
mod model;
mod pipeline;
mod sources;

use clap::{Parser, Subcommand};
use formats::json::Configuration;
use pipeline::{Error, Output, matrix};
use serde::{Deserialize, Serialize};
use sources::{Artifact, Cache, lunar};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "mc_mappings_generator",
    version,
    about = "Minecraft Java Edition mappings generator (no JVM)"
)]
struct Cli {
    /// Cache directory (default: %LOCALAPPDATA%\mc_mappings_generator).
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    /// Use the cache only; fail on any missing entry.
    #[arg(long, global = true)]
    offline: bool,
    /// Use exactly the inputs recorded in this lock file.
    #[arg(long, global = true)]
    locked: Option<PathBuf>,
    /// Configurations generated in parallel.
    #[arg(long, global = true, default_value_t = 4)]
    jobs: usize,
    /// Namespaces to export besides `runtime` (comma-separated).
    #[arg(long, global = true, value_delimiter = ',')]
    namespaces: Option<Vec<String>>,
    /// Indent JSON output.
    #[arg(long, global = true)]
    pretty: bool,
    /// Gzip mapping files.
    #[arg(long, global = true)]
    gzip: bool,
    /// No progress on stderr.
    #[arg(short, long, global = true)]
    quiet: bool,
    /// More detail on stderr (repeatable).
    #[arg(long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,
    /// Machine-readable output for listings and reports.
    #[arg(long, global = true)]
    json: bool,
    /// Matrix overrides file (default: ./matrix-overrides.json when present).
    #[arg(long, global = true)]
    overrides: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List supported Minecraft releases.
    Versions {
        #[arg(long)]
        loader: Option<String>,
        #[arg(long)]
        launcher: Option<String>,
    },
    /// List existing configurations, their default loader build and expected runtime namespace.
    Matrix {
        #[arg(long)]
        version: Option<String>,
    },
    /// Generate one configuration (`-v`) or the whole matrix (`--all`).
    Generate {
        #[arg(short = 'v', long = "version", required_unless_present = "all")]
        minecraft_version: Option<String>,
        /// vanilla, fabric, quilt, forge, neoforge; with --launcher lunar: lunar or fabric.
        #[arg(short, long)]
        loader: Option<String>,
        /// official or lunar.
        #[arg(long)]
        launcher: Option<String>,
        #[arg(long)]
        loader_version: Option<String>,
        #[arg(long, conflicts_with_all = ["minecraft_version", "loader_version"])]
        all: bool,
        #[arg(long, requires = "all")]
        since: Option<String>,
        #[arg(long, requires = "all")]
        until: Option<String>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Re-run verification of an exported file against its runtime jars.
    Verify { file: PathBuf },
    /// Detect local installations (official launcher, Lunar) and optionally generate them.
    Scan {
        #[arg(long)]
        minecraft_dir: Option<PathBuf>,
        #[arg(long)]
        lunar_dir: Option<PathBuf>,
        #[arg(long, requires = "output")]
        generate: bool,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Write the JSON Schema of the output format.
    Schema {
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Convert a generated file to Tiny v2.
    Tiny {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Inspect or clean the cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Convert generated files into an mc-mappings dataset (table format schema 2).
    McMappings {
        /// Semantic catalog (format `mc_mappings_catalog`).
        #[arg(long)]
        catalog: PathBuf,
        /// Dataset revision written as `data_version`.
        #[arg(long)]
        data_version: u64,
        #[arg(short, long)]
        output: PathBuf,
        /// Generated `mappings` files, one per configuration.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    Info,
    Clean {
        /// Age in days.
        #[arg(long)]
        older_than: Option<u64>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Lock {
    format: String,
    format_version: u32,
    configurations: Vec<LockEntry>,
    /// Every fetched URL → SHA-256 (metadata included), used by `--locked`.
    fetched: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LockEntry {
    configuration: Configuration,
    inputs: Vec<Artifact>,
}

/// Written files, inputs and resolved configuration, or the error.
type Outcome = Result<(Vec<PathBuf>, Vec<Artifact>, Configuration), String>;

struct App {
    cli: Cli,
    cache: Cache,
}

fn usage(m: impl Into<String>) -> Error {
    Error::Usage(m.into())
}

fn io(e: impl std::fmt::Display) -> Error {
    Error::Generation(e.to_string())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(io)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(io)
}

impl App {
    fn progress(&self, msg: impl AsRef<str>) {
        if !self.cli.quiet {
            eprintln!("{}", msg.as_ref());
        }
    }

    fn output(&self) -> Output {
        Output {
            namespaces: self.cli.namespaces.clone(),
            pretty: self.cli.pretty,
            gzip: self.cli.gzip,
        }
    }

    fn overrides(&self) -> Result<matrix::Overrides, Error> {
        let path = match &self.cli.overrides {
            Some(p) => p.clone(),
            None if Path::new("matrix-overrides.json").exists() => {
                PathBuf::from("matrix-overrides.json")
            }
            None => return Ok(Default::default()),
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| usage(format!("{}: {e}", path.display())))?;
        serde_json::from_str(&text).map_err(|e| usage(format!("{}: {e}", path.display())))
    }

    fn print_lines(&self, lines: &[String], json: &impl Serialize) {
        if self.cli.json {
            println!("{}", serde_json::to_string_pretty(json).unwrap());
        } else {
            for l in lines {
                println!("{l}");
            }
        }
    }

    /// Generates configurations grouped by version (base computed once per version), in
    /// parallel; returns per-configuration outcomes.
    fn generate_many(
        &self,
        configs: Vec<Configuration>,
        dir: &Path,
    ) -> Vec<(Configuration, Outcome)> {
        use rayon::prelude::*;
        let mut by_version: BTreeMap<String, Vec<Configuration>> = BTreeMap::new();
        for c in configs {
            by_version
                .entry(c.minecraft_version.clone())
                .or_default()
                .push(c);
        }
        let total: usize = by_version.values().map(Vec::len).sum();
        let done = std::sync::atomic::AtomicUsize::new(0);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.cli.jobs.max(1))
            .build()
            .expect("thread pool");
        let releases = match matrix::releases(&self.cache, &matrix::Filter::default()) {
            Ok(r) => r,
            Err(e) => {
                return by_version
                    .into_values()
                    .flatten()
                    .map(|c| (c, Err(e.to_string())))
                    .collect();
            }
        };
        pool.install(|| {
            by_version
                .into_par_iter()
                .flat_map(|(version, cfgs)| {
                    let base = releases
                        .iter()
                        .find(|r| r.id == version)
                        .ok_or_else(|| format!("{version} is not a supported release"))
                        .and_then(|r| pipeline::base(&self.cache, r).map_err(|e| e.to_string()));
                    cfgs.into_iter()
                        .map(|cfg| {
                            let outcome = base.as_ref().map_err(Clone::clone).and_then(|b| {
                                let g = pipeline::generate(&self.cache, b, &cfg)
                                    .map_err(|e| e.to_string())?;
                                let paths = pipeline::write(&g, dir, &self.output())
                                    .map_err(|e| e.to_string())?;
                                Ok((paths, g.sources.clone(), g.configuration.clone()))
                            });
                            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            let status = match &outcome {
                                Ok(_) => "ok".to_owned(),
                                Err(e) => format!("FAILED: {}", e.lines().next().unwrap_or("")),
                            };
                            self.progress(format!(
                                "[{n}/{total}] {} {} {}: {status}",
                                cfg.minecraft_version, cfg.launcher, cfg.loader
                            ));
                            (cfg, outcome)
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        })
    }

    /// Records the inputs of generated configurations in `<dir>/mappings.lock.json`.
    fn update_lock(
        &self,
        dir: &Path,
        generated: &[(Configuration, Vec<Artifact>)],
    ) -> Result<PathBuf, Error> {
        let path = dir.join("mappings.lock.json");
        let mut lock = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Lock>(&t).ok())
            .unwrap_or(Lock {
                format: "mappings-lock".into(),
                format_version: 1,
                configurations: vec![],
                fetched: BTreeMap::new(),
            });
        for (cfg, inputs) in generated {
            let same = |e: &LockEntry| {
                (
                    &e.configuration.minecraft_version,
                    &e.configuration.launcher,
                    &e.configuration.loader,
                ) == (&cfg.minecraft_version, &cfg.launcher, &cfg.loader)
            };
            lock.configurations.retain(|e| !same(e));
            let mut inputs = inputs.clone();
            inputs.sort_by(|a, b| (&a.id, &a.url).cmp(&(&b.id, &b.url)));
            inputs.dedup_by(|a, b| a.url == b.url);
            lock.configurations.push(LockEntry {
                configuration: cfg.clone(),
                inputs,
            });
        }
        lock.configurations
            .sort_by(|a, b| a.configuration.cmp(&b.configuration));
        lock.fetched.extend(self.cache.log.lock().unwrap().clone());
        write_json(&path, &lock)?;
        Ok(path)
    }

    fn resolve(
        &self,
        version: &str,
        launcher: Option<&str>,
        loader: Option<&str>,
        build: Option<&str>,
    ) -> Result<Configuration, Error> {
        let launcher = launcher.unwrap_or("official");
        let loader = loader.unwrap_or(if launcher == "lunar" {
            "lunar"
        } else {
            "vanilla"
        });
        let known = match launcher {
            "official" => ["vanilla", "fabric", "quilt", "forge", "neoforge"].contains(&loader),
            "lunar" => ["lunar", "fabric"].contains(&loader),
            _ => false,
        };
        if !known {
            return Err(usage(format!("unknown configuration {launcher}/{loader}")));
        }
        let mut build = build.map(str::to_owned);
        if build.is_none()
            && let Some(lock) = &self.lock_file()?
        {
            build = lock
                .configurations
                .iter()
                .find(|e| {
                    (
                        e.configuration.minecraft_version.as_str(),
                        e.configuration.launcher.as_str(),
                        e.configuration.loader.as_str(),
                    ) == (version, launcher, loader)
                })
                .and_then(|e| e.configuration.loader_version.clone());
        }
        Ok(Configuration {
            minecraft_version: version.into(),
            launcher: launcher.into(),
            loader: loader.into(),
            loader_version: build,
        })
    }

    fn lock_file(&self) -> Result<Option<Lock>, Error> {
        let Some(path) = &self.cli.locked else {
            return Ok(None);
        };
        let text =
            std::fs::read_to_string(path).map_err(|e| usage(format!("{}: {e}", path.display())))?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| usage(format!("{}: {e}", path.display())))
    }

    fn run(&self) -> Result<ExitCode, Error> {
        match &self.cli.command {
            Command::Versions { loader, launcher } => {
                let filter = matrix::Filter {
                    loader: loader.clone(),
                    launcher: launcher.clone(),
                    ..Default::default()
                };
                let entries = if loader.is_none() && launcher.is_none() {
                    matrix::releases(&self.cache, &filter)?
                        .into_iter()
                        .map(|r| r.id)
                        .collect::<Vec<_>>()
                } else {
                    let mut v: Vec<String> =
                        matrix::matrix(&self.cache, &filter, &self.overrides()?)?
                            .into_iter()
                            .map(|e| e.configuration.minecraft_version)
                            .collect();
                    v.dedup();
                    v
                };
                self.print_lines(&entries, &entries);
            }
            Command::Matrix { version } => {
                let filter = matrix::Filter {
                    version: version.clone(),
                    ..Default::default()
                };
                let entries = matrix::matrix(&self.cache, &filter, &self.overrides()?)?;
                let lines: Vec<String> = entries
                    .iter()
                    .map(|e| {
                        let c = &e.configuration;
                        let rt = e.expected_runtime.as_ref().map_or(
                            "derived at generation".to_owned(),
                            |(a, b)| {
                                if a == b {
                                    a.clone()
                                } else {
                                    format!("classes {a}, members {b}")
                                }
                            },
                        );
                        let excluded = e
                            .excluded
                            .as_ref()
                            .map(|r| format!("  [excluded: {r}]"))
                            .unwrap_or_default();
                        format!(
                            "{:<10} {:<8} {:<9} {:<28} {rt}{excluded}",
                            c.minecraft_version,
                            c.launcher,
                            c.loader,
                            c.loader_version.as_deref().unwrap_or("-")
                        )
                    })
                    .collect();
                self.print_lines(&lines, &entries);
            }
            Command::Generate {
                minecraft_version,
                loader,
                launcher,
                loader_version,
                all,
                since,
                until,
                output,
            } => {
                let configs = if *all {
                    let filter = matrix::Filter {
                        since: since.clone(),
                        until: until.clone(),
                        loader: loader.clone(),
                        launcher: launcher.clone(),
                        version: None,
                    };
                    matrix::matrix(&self.cache, &filter, &self.overrides()?)?
                        .into_iter()
                        .filter(|e| e.excluded.is_none())
                        .map(|e| {
                            let c = e.configuration;
                            self.resolve(
                                &c.minecraft_version,
                                Some(&c.launcher),
                                Some(&c.loader),
                                c.loader_version.as_deref(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    let v = minecraft_version.as_deref().expect("required by clap");
                    vec![self.resolve(
                        v,
                        launcher.as_deref(),
                        loader.as_deref(),
                        loader_version.as_deref(),
                    )?]
                };
                if !*all {
                    // Unknown configurations are usage errors (exit 2), not generation failures.
                    let c = &configs[0];
                    let releases = matrix::releases(
                        &self.cache,
                        &matrix::Filter {
                            version: Some(c.minecraft_version.clone()),
                            ..Default::default()
                        },
                    )?;
                    let base = pipeline::base(&self.cache, &releases[0])?;
                    let g = pipeline::generate(&self.cache, &base, c)?;
                    let mut paths = pipeline::write(&g, output, &self.output())?;
                    paths.push(
                        self.update_lock(output, &[(g.configuration.clone(), g.sources.clone())])?,
                    );
                    if self.cli.verbose > 0 {
                        self.progress(format!(
                            "{} warnings, coverage {:?}",
                            g.report.warnings.len(),
                            g.report.coverage
                        ));
                    }
                    for p in paths {
                        println!("{}", p.display());
                    }
                    return Ok(ExitCode::SUCCESS);
                }
                let results = self.generate_many(configs, output);
                let ok: Vec<(Configuration, Vec<Artifact>)> = results
                    .iter()
                    .filter_map(|(_, r)| r.as_ref().ok().map(|(_, s, c)| (c.clone(), s.clone())))
                    .collect();
                let lock = self.update_lock(output, &ok)?;
                let summary: Vec<serde_json::Value> = results
                    .iter()
                    .map(|(c, r)| match r {
                        Ok((paths, _, cfg)) => serde_json::json!({ "configuration": cfg, "status": "ok", "files": paths.iter().map(|p| p.strip_prefix(output).unwrap_or(p).display().to_string().replace('\\', "/")).collect::<Vec<_>>() }),
                        Err(e) => serde_json::json!({ "configuration": c, "status": "failed", "error": e }),
                    })
                    .collect();
                let failed = results.iter().filter(|(_, r)| r.is_err()).count();
                let report = output.join("generate-all.report.json");
                write_json(
                    &report,
                    &serde_json::json!({ "total": results.len(), "failed": failed, "results": summary }),
                )?;
                for (_, r) in &results {
                    if let Ok((paths, _, _)) = r {
                        for p in paths {
                            println!("{}", p.display());
                        }
                    }
                }
                println!("{}\n{}", lock.display(), report.display());
                self.progress(format!("{} configurations, {failed} failed", results.len()));
                return Ok(if failed > 0 {
                    ExitCode::from(4)
                } else {
                    ExitCode::SUCCESS
                });
            }
            Command::Verify { file } => {
                let f = std::fs::File::open(file)
                    .map_err(|e| usage(format!("{}: {e}", file.display())))?;
                let parsed = formats::json::read(f)
                    .map_err(|e| usage(format!("{}: {e}", file.display())))?;
                let c = &parsed.configuration;
                let releases = matrix::releases(
                    &self.cache,
                    &matrix::Filter {
                        version: Some(c.minecraft_version.clone()),
                        ..Default::default()
                    },
                )?;
                let base = pipeline::base(&self.cache, &releases[0])?;
                let g = pipeline::generate(&self.cache, &base, c)?;
                let errors = pipeline::verify::check(
                    &pipeline::verify::Symbols::from_file(&parsed),
                    &g.runtime_symbols,
                );
                let report = serde_json::json!({ "file": file.display().to_string(), "configuration": c, "errors": errors.len(), "details": errors });
                if self.cli.json {
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                } else {
                    for e in &errors {
                        println!("{e}");
                    }
                    println!("{} errors", errors.len());
                }
                return Ok(if errors.is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                });
            }
            Command::Scan {
                minecraft_dir,
                lunar_dir,
                generate,
                output,
            } => {
                let mc_dir = minecraft_dir.clone().or_else(|| {
                    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(".minecraft"))
                });
                let mut found = match &mc_dir {
                    Some(d) if d.exists() => scan_official(d)?,
                    _ => Vec::new(),
                };
                let lunar_dir = lunar_dir.clone().or_else(lunar::default_dir);
                if let Some(d) = lunar_dir.filter(|d| d.exists()) {
                    for c in lunar::scan(&self.cache, &d)? {
                        found.push(Configuration {
                            minecraft_version: c.minecraft_version,
                            launcher: "lunar".into(),
                            loader: c.module,
                            loader_version: c.loader_version,
                        });
                    }
                }
                found.sort();
                found.dedup();
                let lines: Vec<String> = found
                    .iter()
                    .map(|c| {
                        format!(
                            "{} {} {} {}",
                            c.minecraft_version,
                            c.launcher,
                            c.loader,
                            c.loader_version.as_deref().unwrap_or("-")
                        )
                    })
                    .collect();
                self.print_lines(&lines, &found);
                if *generate {
                    let dir = output.as_ref().expect("required by clap");
                    let results = self.generate_many(found, dir);
                    let ok: Vec<_> = results
                        .iter()
                        .filter_map(|(_, r)| {
                            r.as_ref().ok().map(|(_, s, c)| (c.clone(), s.clone()))
                        })
                        .collect();
                    self.update_lock(dir, &ok)?;
                    if results.iter().any(|(_, r)| r.is_err()) {
                        return Ok(ExitCode::from(4));
                    }
                }
            }
            Command::Schema { output } => {
                std::fs::write(output, formats::json::SCHEMA).map_err(io)?;
                println!("{}", output.display());
            }
            Command::Tiny { file, output } => {
                let f = std::fs::File::open(file)
                    .map_err(|e| usage(format!("{}: {e}", file.display())))?;
                let parsed = formats::json::read(f)
                    .map_err(|e| usage(format!("{}: {e}", file.display())))?;
                let mut w = std::io::BufWriter::new(std::fs::File::create(output).map_err(io)?);
                formats::tiny::write(&parsed, &mut w).map_err(io)?;
                std::io::Write::flush(&mut w).map_err(io)?;
                println!("{}", output.display());
            }
            Command::Cache { action } => match action {
                CacheAction::Info => {
                    let (count, size) = self.cache.info()?;
                    let info = serde_json::json!({ "dir": self.cache.dir.display().to_string(), "objects": count, "bytes": size });
                    self.print_lines(
                        &[format!(
                            "{} objects, {} MiB in {}",
                            count,
                            size >> 20,
                            self.cache.dir.display()
                        )],
                        &info,
                    );
                }
                CacheAction::Clean { older_than } => {
                    let removed = self
                        .cache
                        .clean(older_than.map(|d| std::time::Duration::from_secs(d * 86_400)))?;
                    println!("{removed} files removed");
                }
            },
            Command::McMappings {
                catalog,
                data_version,
                output,
                inputs,
            } => {
                let text = std::fs::read_to_string(catalog)
                    .map_err(|e| usage(format!("{}: {e}", catalog.display())))?;
                let catalog: formats::mc_mappings::Catalog =
                    serde_json::from_str(&text).map_err(|e| usage(format!("catalog: {e}")))?;
                let mut files = Vec::new();
                let mut inputs_prov = Vec::new();
                for path in inputs {
                    let bytes = std::fs::read(path)
                        .map_err(|e| usage(format!("{}: {e}", path.display())))?;
                    let f = formats::json::read(&bytes[..])
                        .map_err(|e| usage(format!("{}: {e}", path.display())))?;
                    inputs_prov.push(serde_json::json!({
                        "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                        "sha256": sources::sha256(&bytes),
                        "configuration": f.configuration,
                        "provenance": f.provenance,
                    }));
                    files.push(f);
                }
                let dataset = formats::mc_mappings::export(&catalog, &files, *data_version)
                    .map_err(|errors| {
                        Error::Generation(format!(
                            "{} unresolved catalog entries:\n  {}",
                            errors.len(),
                            errors.join("\n  ")
                        ))
                    })?;
                write_json(output, &dataset)?;
                let prov_path = output.with_extension("provenance.json");
                write_json(
                    &prov_path,
                    &serde_json::json!({
                        "generator": format!("mc_mappings_generator {}", env!("CARGO_PKG_VERSION")),
                        "data_version": data_version,
                        "catalog_sha256": sources::sha256(text.as_bytes()),
                        "inputs": inputs_prov,
                    }),
                )?;
                println!("{}\n{}", output.display(), prov_path.display());
            }
        }
        Ok(ExitCode::SUCCESS)
    }
}

/// Configurations installed in an official launcher directory (`versions/*/*.json`).
fn scan_official(dir: &Path) -> Result<Vec<Configuration>, Error> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir.join("versions")) else {
        return Ok(out);
    };
    let profiles: BTreeMap<String, serde_json::Value> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(e.path().join(format!("{name}.json"))).ok()?;
            Some((name, serde_json::from_str(&text).ok()?))
        })
        .collect();
    for (id, p) in &profiles {
        // Follow `inheritsFrom` to the vanilla release.
        let mut base = p;
        let mut seen = 0;
        while let Some(parent) = base["inheritsFrom"].as_str() {
            let Some(next) = profiles.get(parent) else {
                break;
            };
            base = next;
            seen += 1;
            if seen > 8 {
                break;
            }
        }
        let version = base["id"]
            .as_str()
            .or(p["inheritsFrom"].as_str())
            .unwrap_or(id);
        let lib = |prefix: &str| {
            p["libraries"].as_array().and_then(|ls| {
                ls.iter().find_map(|l| {
                    l["name"]
                        .as_str()?
                        .strip_prefix(prefix)
                        .map(|v| v.split(':').next().unwrap_or(v).to_owned())
                })
            })
        };
        let (loader, build) = if let Some(b) = lib("net.fabricmc:fabric-loader:") {
            ("fabric", Some(b))
        } else if let Some(b) = lib("org.quiltmc:quilt-loader:") {
            ("quilt", Some(b))
        } else if let Some(b) =
            lib("net.neoforged:neoforge:").or_else(|| lib("net.neoforged:forge:"))
        {
            ("neoforge", Some(b))
        } else if let Some(b) = lib("net.minecraftforge:forge:")
            .or_else(|| lib("net.minecraftforge:fmlloader:").map(|v| v.replace("-fmlloader", "")))
        {
            ("forge", Some(b))
        } else if p["inheritsFrom"].is_null() && p["type"] == "release" {
            ("vanilla", None)
        } else {
            continue; // snapshots, unknown loaders
        };
        out.push(Configuration {
            minecraft_version: version.to_owned(),
            launcher: "official".into(),
            loader: loader.into(),
            loader_version: build,
        });
    }
    Ok(out)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cache = match Cache::new(cli.cache_dir.clone(), cli.offline) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(3);
        }
    };
    let mut app = App { cli, cache };
    match app.lock_file() {
        Ok(Some(lock)) => app.cache.lock = Some(lock.fetched),
        Ok(None) => {}
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    }
    match app.run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(match e {
                Error::Generation(_) => 1,
                Error::Usage(_) => 2,
                Error::Source(_) => 3,
            })
        }
    }
}
