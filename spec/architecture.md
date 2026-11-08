# Architecture

## Project layout

A single Cargo package, `mc_mappings_generator` (one binary of the same name).

```text
mc_mappings_generator/
├── Cargo.toml          package, edition 2024, rust-version 1.99, lints
├── Cargo.lock, README.md, .gitignore
├── .github/workflows/ci.yml   runs scripts/check.sh
├── scripts/check.sh           fmt + clippy -D warnings + tests
├── spec/
└── src/
    ├── main.rs         argument parsing, progress, output layout (cli.md)
    ├── model/          MappingSet, descriptors, operations, invariants (model.md)
    ├── formats/        readers and writers (formats.md)
    ├── classfile/      minimal .class parser and jar inventory (names, descriptors, access, hierarchy, fingerprints)
    ├── sources/        HTTP client, cache, providers: Mojang, Fabric, Quilt, Forge, MCP, NeoForge, Lunar (sources.md)
    └── pipeline/       matrix, eras, runtime namespace derivation/detection, generation, verification (pipeline.md)
```

Each module under `src/` is created by the ticket that implements it.

Call rule, kept by convention: `main.rs → pipeline → sources / formats / classfile → model`.
A module never calls a module above it. `model`, `formats` and `classfile` do no I/O besides
readers/writers passed in.

## Extension points

- `Provider` trait: availability (`versions()`, `builds(version)`) and artifacts for a
  configuration. A new loader = a new provider + an era entry.
- `Era` table: per (loader, version range resolved from availability) → sources to load and
  runtime-derivation rule. New naming eras are data, not pipeline changes.
- Format `Reader`/`Writer` traits.

## Main dependencies

`clap` (CLI), `serde` + `serde_json`, `ureq` with rustls (blocking HTTP) + `rayon` (parallelism),
`zip`, `sha1` + `sha2`, `quick-xml` (Maven metadata), `directories` (cache path),
`thiserror` (module errors), `anyhow` (`main.rs`), `tracing` + `tracing-subscriber`, `indicatif`
(progress), `flate2` (gzip). Tests: `insta`, `tempfile`. No async runtime, no JVM.

## Errors

Modules expose typed errors (`thiserror`) carrying context (configuration, artifact,
line number, symbol). No panic on bad input. The CLI maps them to the exit codes in [cli.md](cli.md).

## Testing

| Level | Content | Network |
| --- | --- | --- |
| Unit | parsers, descriptors, model operations, classfile parser on generated class files | no |
| Snapshot | small synthetic configurations end-to-end (`insta`) | no |
| Integration | one representative configuration per era and loader (e.g. 1.7.10 Forge, 1.12.2 Forge, 1.16.5 Forge, 1.20.1 NeoForge, 1.21.11 Fabric, 26.x Vanilla, Lunar sample) | yes, `#[ignore]`, run on demand/CI schedule |
| Reproducibility | two generations from the same lock are byte-identical | yes |

Quality gates (`scripts/check.sh`): `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`.

Test fixtures are synthetic files in the real formats. Real upstream data (game jars, Mojang
mappings, MCP/SRG names) is never committed; it is only used by the network tests above.
