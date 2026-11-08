# Namespaces

## Standard namespaces

| Namespace | Example class | Example member | Origin |
| --- | --- | --- | --- |
| `official` | `fud` | `a` | obfuscated client jar |
| `intermediary` | `net/minecraft/class_310` | `method_1551` | Fabric Intermediary |
| `srg` | `net/minecraft/client/Minecraft` (MCP class names ≤ 1.16.5, Mojang class names ≥ 1.17) | `func_71410_x` (≤ 1.16.5), `m_91087_` (≥ 1.17) | MCP / MCPConfig |
| `mcp` | `net/minecraft/client/Minecraft` | `getMinecraft` | MCP CSV over `srg` |
| `mojang` | `net/minecraft/client/Minecraft` | `getInstance` | Mojang ProGuard / unobfuscated jar |
| `yarn` | `net/minecraft/client/MinecraftClient` | `getInstance` | Yarn |
| `lunar` | Lunar-specific classes | | Lunar Mapping Project |

`runtime` and `named` are aliases set per configuration; they never hold their own data.

## Expected runtime namespace

This table is the **hypothesis** to be confirmed by the validation ticket and the detector.
Any disagreement found by detection updates this table; the detector's verdict wins.

| Launcher | Loader | Versions | Classes | Members |
| --- | --- | --- | --- | --- |
| official | vanilla | ≤ 1.21.11 | `official` | `official` |
| official | vanilla | ≥ 26.1 | `mojang` | `mojang` |
| official | fabric, quilt | ≤ 1.21.11 | `intermediary` | `intermediary` |
| official | fabric, quilt | ≥ 26.1 | `mojang` | `mojang` |
| official | forge | 1.7.10 → 1.16.5 | `srg` (MCP class names) | `srg` (`func_`/`field_`) |
| official | forge | 1.17 → per install profile | `srg` (= Mojang class names) | `srg` (`m_`/`f_`) |
| official | forge, neoforge | when install profile declares Mojang runtime | `mojang` | `mojang` |
| official | neoforge | 1.20.1 → 1.20.4 | `srg` | `srg` |
| lunar | any | any | detected | detected |

A configuration may mix namespaces (classes from one, members from another); the model handles
class and member namespaces independently.

## Derivation for official launcher

For Forge and NeoForge, the runtime namespace is read from the installer's
`install_profile.json` (data entries and processor arguments referencing MCP/MCPConfig or
Mojang mappings). No processor is executed.

For Fabric and Quilt, the runtime namespace is the one declared by the loader metadata
(Intermediary artifact present → `intermediary`; absent for an unobfuscated version → `mojang`).

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

Lunar-specific (non-Minecraft) classes are exported in their runtime names; when the Lunar
Mapping Project publishes mappings for the exact jar hash, they are added as the `lunar` namespace.
