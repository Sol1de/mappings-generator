# Minecraft mappings generator

Generates class, method and field mappings for Minecraft Java Edition releases (1.7.10 → latest)
across Vanilla, Fabric, Quilt, Forge and NeoForge, for the official launcher and Lunar Client.
Exports JSON (`mappings`), and converts it to the [mc-mappings](../mc_mappings) table format.
Command-line only, for humans and agents. Windows x64. Pure Rust, no JVM.

**Status:** v0.1 — every configuration of the matrix generates and verifies (251 configurations
on 2026-11-10; one excluded with its reason in [`matrix-overrides.json`](matrix-overrides.json)).

- Specifications: [`spec/`](spec/README.md)
- Quality gates: `sh scripts/check.sh` (fmt, clippy `-D warnings`, tests);
  network tests: `cargo test --release -- --ignored`

## Install

```sh
cargo build --release   # target/release/mc_mappings_generator.exe
```

Downloads go to `%LOCALAPPDATA%\mc_mappings_generator` (`--cache-dir` to change it).

## Use

```sh
mc_mappings_generator versions                       # releases from 1.7.10
mc_mappings_generator matrix --version 1.21.11       # configurations, default builds, runtime namespace
mc_mappings_generator generate -v 1.21.11 -l fabric -o out
mc_mappings_generator generate --all --jobs 6 -o out # whole matrix; exit 4 if some fail
mc_mappings_generator verify out/1.21.11/official-fabric-0.20.1.json
mc_mappings_generator schema -o mappings.schema.json
mc_mappings_generator tiny out/1.21.11/official-fabric-0.20.1.json -o fabric.tiny   # Tiny v2
```

Each configuration gives `out/<version>/<launcher>-<loader>[-<build>].json` (the mappings) and
`.report.json` (warnings, coverage, runtime-namespace proof, symbols not exported).
`out/mappings.lock.json` pins every input; `--locked out/mappings.lock.json --offline`
regenerates byte-identical files. Exit codes: 0 ok, 1 generation/verification error,
2 usage/unknown configuration, 3 network/cache, 4 partial `--all`.

### Lunar Client

Lunar remaps the game at launch, so its runtime names are detected from the classes Lunar
actually loads. For each Lunar configuration: launch it once from the Lunar launcher (the game
can be closed as soon as the menu appears), then run `mc_mappings_generator scan` right away.
`scan` identifies the launch from Lunar's logs, copies its class cache into the generator cache,
and the configuration appears in `matrix`. Nothing is sent anywhere.

Observed on 2026-11-10 (Lunar launcher 3.7.34): Lunar without loader runs on Mojang names,
Lunar + Fabric on Intermediary names, for 1.21 and 1.21.11.

### mc-mappings dataset

```sh
consumers/mc_mappings/build-dataset.sh OUT 2     # 10 configurations → OUT/minecraft-v1.json
```

The [catalog](consumers/mc_mappings/catalog-v1.json) names each semantic symbol by its Mojang
name and semantic types (with per-version overrides for symbols Mojang renamed, retyped, added or
removed). The export resolves the runtime name, declaring class, static flag and descriptor of
every symbol in every configuration of the versions it exists in, shares identical tables between
configurations, and fails on any missing symbol.
`OUT/minecraft-v1.provenance.json` records each input file's hash and provenance.

## Output and licences

The JSON format is documented in [spec/formats.md](spec/formats.md) and its schema is embedded
(`schema` command). Game jars and mapping files stay in the local cache. Outputs embed their
sources and licences: Mojang and MCP names are not redistributable by this tool; Intermediary
and Yarn are CC0. Users decide whether an output can be shared.

## Adding a loader or an era

See [spec/architecture.md](spec/architecture.md#extension-points).
