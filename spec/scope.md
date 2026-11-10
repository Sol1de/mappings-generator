# Scope

## Minecraft versions

- Every entry of the [Mojang version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json)
  with `type == "release"`, from **1.7.10** (inclusive) to the latest release, ordered by `releaseTime`.
- Snapshots, pre-releases, release candidates, experimental and April-fools builds are excluded
  (they are not `release` entries).
- Version identifiers are kept verbatim (`1.7.10`, `1.21.11`, `26.1`, ...). No alias, trimming
  or semantic-version normalisation.
- Both version schemes are supported: `1.x[.y]` and the year-based scheme (`26.1`, ...) used from 2026.
- Only the **client** distribution is targeted. Server jars are out of scope.

## Loaders

| Loader | Availability source | Lower bound |
| --- | --- | --- |
| `vanilla` | Mojang manifest | 1.7.10 |
| `fabric` | Fabric Meta, stable game versions | 1.14 (official Fabric only) |
| `quilt` | Quilt Meta | as published by Quilt Meta |
| `forge` | Forge Maven metadata | 1.7.10 |
| `neoforge` | NeoForge Maven metadata (`net.neoforged:forge` for 1.20.1, `net.neoforged:neoforge` after) | 1.20.1 |

Legacy Fabric, Ornithe, Babric, Rift, LiteLoader and other loaders are out of scope.
The provider abstraction must allow adding them later without changing the pipeline.

## Launchers

| Launcher | Meaning |
| --- | --- |
| `official` | Official Minecraft Launcher, or any launcher running the unmodified game + loader |
| `lunar` | Lunar Client, which ships its own patched game and loader modules |

For `official`, the runtime is fully determined by the version and the loader.
For `lunar`, available versions and modules (`lunar` = no loader, `fabric`, others if offered)
are the ones captured from the local Lunar installation (launched, then `scan`; see
[cli.md](cli.md#local-installation-scan)). A Lunar configuration is never assumed identical to the
official one; its runtime namespace is detected (see [namespaces.md](namespaces.md)).

## Configuration matrix

The matrix is computed at run time: `versions × launchers × loaders`, keeping only combinations
that the corresponding upstream metadata declares as existing. Loader versions:

- default: the latest stable loader build for that Minecraft version;
- selectable explicitly (`--loader-version`); when the runtime naming does not depend on the
  loader build (Fabric, Quilt), the result is identical across builds and recorded once.

A local `matrix-overrides.json` (optional) can exclude a combination with a reason. It cannot
add a combination that upstream metadata does not declare.

## Platforms

Windows x64 only for now; Linux and macOS come later. Platform matters only for local paths and
Lunar metadata requests; the generated mappings are platform-independent.

## Out of scope

- Running the game, a JVM or a loader installer.
- Mod mappings, server mappings, Bedrock.
- Parameter names, local variables and Javadoc (possible later extension via Parchment/Yarn).
- Resolvers and consumer logic. The one consumer conversion shipped, `mc-mappings`, is a
  downstream transformation of the JSON output ([formats.md](formats.md#mc-mappings-dataset)).
- Tiny v2 output (possible later version, converted from the JSON output).
- Graphical interface: the CLI is the only interface.
