# mc_mappings_generator specifications

`mc_mappings_generator` is a general-purpose generator of Minecraft Java Edition mappings, written in Rust.
It produces, for each supported configuration, the most complete and exact mapping
possible between the names **loaded at runtime** and every other known namespace
(obfuscated, intermediary, SRG, Mojang, MCP, Yarn...), for classes, fields and methods.

The tool is independent of any consumer project. Adapting its output to a specific
consumer (for example a semantic symbol catalog) is a separate, downstream transformation; the
`mc-mappings` command and `consumers/mc_mappings/` provide the one for mc-mappings.

## Documents

| Document | Responsibility |
| --- | --- |
| [scope.md](scope.md) | Versions, loaders, launchers, and how the configuration matrix is discovered |
| [sources.md](sources.md) | Upstream data providers, their coverage, integrity and licensing rules |
| [namespaces.md](namespaces.md) | Runtime namespace of each configuration and how it is detected |
| [evidence/runtime-namespaces.md](evidence/runtime-namespaces.md) | Upstream proofs (artifacts, hashes, observed excerpts) behind the runtime namespace table |
| [model.md](model.md) | Internal mapping model and its operations |
| [pipeline.md](pipeline.md) | Generation steps, reconstruction, verification, cache, determinism |
| [formats.md](formats.md) | Input formats read and JSON output format written |
| [cli.md](cli.md) | Command-line interface |
| [architecture.md](architecture.md) | Project layout, module call rule, errors, testing |

## Principles

- **Runtime first.** The reference namespace of a configuration is the one actually loaded by
  the JVM for that configuration. It is detected or derived from evidence, never assumed.
- **Discovered, not hard-coded.** Versions and loader availability come from upstream metadata.
  Adding a Minecraft release requires no code change unless a new naming era appears.
- **Exact or explicit.** A missing name, an ambiguous match or a failed verification is reported,
  never silently replaced by a guess or by a neighbouring version.
- **Reproducible.** Same inputs (pinned by hash) produce byte-identical outputs.
- **No JVM.** Pure Rust. No Java process, no loader installer execution.
- **No redistribution.** Game jars and upstream mapping files are downloaded into a local cache
  only. Outputs are generated locally by the user.

## Glossary

| Term | Meaning |
| --- | --- |
| Configuration | Tuple `(minecraft_version, launcher, loader[, loader_version])` |
| Namespace | A naming scheme: `official`, `intermediary`, `srg`, `mcp`, `mojang`, `yarn`, `runtime` |
| Runtime namespace | Alias of the namespace whose names the JVM sees for a configuration |
| Named namespace | Human-readable names (Mojang, MCP or Yarn depending on availability) |
| Era | Range of versions sharing the same sources and runtime-naming rules |
