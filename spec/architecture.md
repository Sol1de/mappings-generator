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

Providers are modules of plain functions under `src/sources/` (availability, builds, artifacts);
the per-configuration rules are one `match` in `pipeline::generate` (`src/pipeline/mod.rs`), and
the matrix is assembled in `pipeline::matrix`. Era boundaries are not listed anywhere: they follow
from what each upstream artifact contains (client_mappings, launch profile libraries,
install_profile processors), see [namespaces.md](namespaces.md).

To add a loader:

1. Add its provider in `src/sources/` (builds per version, default build, launch metadata) and its
   name to `App::resolve` in `main.rs`.
2. Add its matrix entries in `pipeline::matrix` (only versions its metadata declares).
3. Add its arm in `pipeline::generate`: the evidence that decides the runtime namespaces, and the
   extra mapping sources to merge, if any.
4. Record one proof per era in `spec/evidence/runtime-namespaces.md`, add an integration test
   case in `tests/integration.rs`, update `spec/namespaces.md` and `spec/scope.md`.

To add an era of an existing loader (a new install-profile shape, a new mapping format): extend the
derivation rule (e.g. `sources::forge::derive_runtime`) or add a reader in `src/formats/` with
synthetic fixtures, then the same evidence and tests. An unknown shape fails explicitly
(`unknown install profile`), never falls back.

## Main dependencies

`clap` (CLI), `serde` + `serde_json`, `ureq` with rustls (blocking HTTP), `rayon` (parallelism),
`zip` (deflate only), `sha1` + `sha2`, `flate2` (gzip).
Maven metadata is read with a few lines of string parsing; progress goes to stderr with
`eprintln!`. Tests: `jsonschema` (schema validation), `tempfile`. No async runtime, no JVM.

## Errors

Modules expose typed errors (plain enums and structs implementing `Error`) carrying context (configuration, artifact,
line number, symbol). No panic on bad input. The CLI maps them to the exit codes in [cli.md](cli.md).

## Testing

| Level | Content | Network |
| --- | --- | --- |
| Unit | parsers, descriptors, model operations, classfile parser on generated class files | no |
| CLI | commands, exit codes and the mc-mappings export on synthetic fixtures (`tests/cli.rs`); outputs validated against the embedded schema | no |
| Integration | one representative configuration per era and loader (1.7.10, 1.12.2, 1.13.2, 1.16.5, 1.20.1, 1.21.1, 1.21.11 Forge; 1.20.1, 1.20.4 NeoForge; 1.14.4, 1.21.11, 26.1 Fabric; 1.20.1 Quilt; 1.8.9, 1.21.11, 26.1 Vanilla; Lunar sample) (`tests/integration.rs`) | yes, `#[ignore]`, `cargo test --release -- --ignored` |
| Reproducibility | two generations from the same lock are byte-identical | yes |

Quality gates (`scripts/check.sh`): `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`.

Test fixtures are synthetic files in the real formats. Real upstream data (game jars, Mojang
mappings, MCP/SRG names) is never committed; it is only used by the network tests above.
