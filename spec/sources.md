# Sources

Every provider returns typed artifacts with their origin (URL), expected hash when published,
and observed SHA-256. See [pipeline.md](pipeline.md#cache-and-integrity).

## Providers

| Provider | Endpoint(s) | Provides |
| --- | --- | --- |
| Mojang | `piston-meta.mojang.com/mc/game/version_manifest_v2.json`, per-version descriptor | release list, client jar (SHA-1), `client_mappings` ProGuard file (1.14.4 → 1.21.11) |
| Fabric | `meta.fabricmc.net/v2/versions/{game,loader,intermediary}`, `maven.fabricmc.net` | supported versions, loader builds, Intermediary (Tiny v2), Yarn (Tiny v2) |
| Quilt | `meta.quiltmc.org/v3/versions/...`, `maven.quiltmc.org` | supported versions, loader builds, runtime mapping artifact declared by Quilt |
| Forge | `maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml`, `promotions_slim.json`, installer `install_profile.json` | builds per version, MCP/MCPConfig coordinates, declared runtime mappings |
| MCP | `maven.minecraftforge.net/de/oceanlabs/mcp/` (`mcp:<v>:srg`, `mcp_config`, `mcp_stable`, `mcp_snapshot`) | SRG/TSRG/TSRG2 tables, MCP CSV names (≤ 1.16.5) |
| NeoForge | `maven.neoforged.net/releases/net/neoforged/{forge,neoforge}` metadata and installer `install_profile.json` | builds per version, declared runtime mappings |
| Lunar | local installation (`%USERPROFILE%\.lunarclient`), Lunar launch metadata API | versions/modules offered, classpath jars with hashes |
| Lunar Mapping Project | GitHub releases (Tiny v2, MIT) | optional names for Lunar's own classes, matched by jar hash |

## Named namespace priority

For each version, the `named` namespace is filled by the first available source:

1. `mojang` — official Mojang mappings (1.14.4 → 1.21.11), or the unobfuscated jar itself (26.1+).
2. `mcp` — MCP names over SRG (1.7.10 → 1.16.5, where a channel exists).
3. `yarn` — Yarn (1.14+, used only when Mojang names are unavailable).

All available sources are still exported as their own namespaces; priority only decides which
one is aliased as `named`. Missing names are reported as coverage gaps.

## Naming eras (sources per era)

| Era | Versions | Obf → runtime-relevant sources |
| --- | --- | --- |
| Legacy MCP | 1.7.10 → 1.12.2 | MCP SRG zip (`notch → srg`), MCP CSV |
| MCPConfig TSRG | 1.13 → 1.16.5 | `mcp_config` TSRG (`obf → srg`), MCP CSV |
| MCPConfig TSRG2 | 1.17 → 1.20.4 (Forge), 1.20.1 (NeoForge) | TSRG2 (`obf → srg` with ids; class names are ids `C_n_`), Mojang ProGuard |
| Mojang-only | 1.20.2 (NeoForge) / 1.20.6 (Forge) → 1.21.11 | Mojang ProGuard, Intermediary |
| Unobfuscated | 26.1 → latest | client jar (names = Mojang) |

Era boundaries are resolved from upstream evidence, not from a hard-coded version list. The table
above documents the expected result; proofs are in
[evidence/runtime-namespaces.md](evidence/runtime-namespaces.md).

- **Mojang ProGuard / unobfuscated:** `client_mappings` present in the version descriptor
  (last: 1.21.11); absent and classes under `net/minecraft/` → unobfuscated (first: 26.1).
- **End of MCPConfig:** decided per loader build by its `install_profile.json`
  (see [namespaces.md](namespaces.md#derivation-for-official-launcher)). Publication of
  `mcp_config` is **not** a signal: it is still published for 26.x.

## Lunar metadata

Lunar's launch API is undocumented and may change. Rules:

- Prefer reading a local Lunar installation when present (`mc_mappings_generator scan`, see [cli.md](cli.md)).
- The API client is isolated in its own provider; any response-shape change fails explicitly.
- Requests send no account data, no hardware identifier and a generic user agent.
- Downloaded jars stay in the local cache; nothing from Lunar is redistributed.

## Licensing and redistribution

| Data | Rule |
| --- | --- |
| Game jars (Mojang, Lunar) | Never redistributed; local cache only |
| Mojang mappings | Used locally; `mc_mappings_generator` does not publish them. Users are responsible for redistribution of outputs |
| MCP names/SRG | Same rule as Mojang mappings |
| Intermediary, Yarn (CC0), Lunar Mapping Project (MIT) | Redistributable with their notices |

Every output embeds the list of sources used and their licence identifiers
(see [formats.md](formats.md)) so that users can decide whether an output can be shared.
