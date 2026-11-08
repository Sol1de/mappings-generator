# Namespaces

## Standard namespaces

| Namespace | Example class | Example member | Origin |
| --- | --- | --- | --- |
| `official` | `gfj` (1.21.11) | `a` | obfuscated client jar |
| `intermediary` | `net/minecraft/class_310` | `method_1551` | Fabric Intermediary |
| `srg` | `net/minecraft/client/Minecraft` (MCP class names ≤ 1.16.5), `net/minecraft/src/C_3391_` (id names ≥ 1.17) | `func_71410_x` (≤ 1.16.5), `m_91087_` (≥ 1.17) | MCP / MCPConfig |
| `mcp` | `net/minecraft/client/Minecraft` | `getMinecraft` | MCP CSV over `srg` |
| `mojang` | `net/minecraft/client/Minecraft` | `getInstance` | Mojang ProGuard / unobfuscated jar |
| `yarn` | `net/minecraft/client/MinecraftClient` | `getInstance` | Yarn |
| `lunar` | Lunar-specific classes | | Lunar Mapping Project |

`runtime` and `named` are aliases set per configuration; they never hold their own data.

## Expected runtime namespace

Validated against upstream artifacts (2026-11-08); one proof per row, with hashes, in
[evidence/runtime-namespaces.md](evidence/runtime-namespaces.md). Versions are the expected result;
the code derives each row from the install profile or loader metadata, not from these bounds.
Any disagreement found by detection updates this table; the detector's verdict wins.

| Launcher | Loader | Versions | Classes | Members |
| --- | --- | --- | --- | --- |
| official | vanilla | ≤ 1.21.11 | `official` | `official` |
| official | vanilla | ≥ 26.1 | `mojang` | `mojang` |
| official | fabric, quilt | ≤ 1.21.11 | `intermediary` | `intermediary` |
| official | fabric, quilt | ≥ 26.1 | `mojang` | `mojang` |
| official | forge | 1.7.10 → 1.16.5 | `srg` (MCP class names) | `srg` (`func_`/`field_`) |
| official | forge | 1.17 → 1.20.4 | `mojang` | `srg` (`m_`/`f_`) |
| official | forge | ≥ 1.20.6 (no 1.20.5 build) | `mojang` | `mojang` |
| official | neoforge | 1.20.1 (`net.neoforged:forge`) | `mojang` | `srg` (`m_`/`f_`) |
| official | neoforge | ≥ 1.20.2 (`net.neoforged:neoforge`) | `mojang` | `mojang` |
| lunar | any | any | detected | detected |

A configuration may mix namespaces (classes from one, members from another); the model handles
class and member namespaces independently.

## Derivation for official launcher

For Forge and NeoForge, the runtime namespace is read from the installer's
`install_profile.json`, client-side processors only. No processor is executed.

| Install profile shows | Classes | Members |
| --- | --- | --- |
| Legacy profile or `spec: 0` without processors (FML `deobfuscation_data-*.lzma`) | `srg` | `srg` |
| `SpecialSource --srg-in {MAPPINGS}` with `MAPPINGS` from `mcp_config` | `srg` | `srg` |
| `MERGE_MAPPING ... --classes` (no `--fields`/`--methods`) | `mojang` | `srg` |
| `MERGE_MAPPING ... --classes --fields --methods` | `mojang` | `mojang` |
| Rename `--names {MOJMAPS}`, or `PROCESS_MINECRAFT_JAR` (with or without `--input-mappings {MOJMAPS}`) | `mojang` | `mojang` |
| Only `binarypatcher` (no merge or rename), unobfuscated version (Forge 26.x) | `mojang` | `mojang` |
| Anything else | error `unknown install profile` | |

The `MERGE_MAPPING` output is then applied by the rename tool, whose flag varies by build
(`vignette --mappings` in 1.17.x, `ForgeAutoRenamingTool`/`AutoRenamingTool --names` later); only
the merge flags decide the namespaces. The presence of a `MAPPINGS`/`mcp_config` data entry alone is
not a signal: Forge 1.20.6 and 1.21 still declare it without using it for the rename.

For Fabric and Quilt, the runtime namespace is the one declared by the loader metadata:
the launch profile lists a real Intermediary artifact → `intermediary`; no Intermediary library
for an unobfuscated version → `mojang`. Fabric Meta lists 26.x Intermediary entries pointing to the
placeholder `net.fabricmc:intermediary:0.0.0`; these count as absent. Quilt Loader targets
`intermediary` too; its `org.quiltmc:hashed` library is on the classpath but not used as the runtime
namespace. Quilt accepts a user override (`loader.experimental.minecraft.targetNamespace`); the
detector catches such setups.

## Detection (Lunar and verification)

Input: the set of classes and members present in the runtime classpath jars of a configuration.
Candidates: every namespace available for that Minecraft version.

1. **Classes.** For each candidate, score = share of the candidate's `net/minecraft` classes found
   by exact name in the runtime jars. Select the candidate with score ≥ 0.98 that beats the
   second best by ≥ 0.10. Otherwise: `ambiguous` error.
2. **Members.** Same scoring on members of the classes matched at step 1, comparing name and
   descriptor remapped into the candidate namespace.
3. **Structural fallback.** If no candidate reaches the threshold (repackaged or renamed classes),
   match classes by structural fingerprint (super class, interface count, field/method
   descriptor multiset, string constants hash) against the `official` jar. Accepted only for
   unique matches; the rest is reported as unmatched.
4. **Proof.** The verdict, scores and unmatched counts are written to the provenance record.

Lunar remaps the game at launch with its own mappings (`lunar-platform-mappings-v<mc>.jar`,
proprietary `.kin` files: `intermediary`, `lunar2intermediary`, `lunar_named`), so the
Lunar runtime namespace is never derived statically.

Lunar-specific (non-Minecraft) classes are exported in their runtime names; when the Lunar
Mapping Project publishes mappings for the exact jar hash, they are added as the `lunar` namespace.
