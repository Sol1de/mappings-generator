# Evidence: runtime namespaces

Proofs behind the "Expected runtime namespace" table in [../namespaces.md](../namespaces.md).
Collected on 2026-11-08 from upstream artifacts; nothing was executed.
SHA-256 is of the downloaded file. Re-run the checks when a new era appears.

The install profile rules these proofs support are in
[../namespaces.md](../namespaces.md#derivation-for-official-launcher). Applied to all 25 Forge and
NeoForge profiles listed below, the rules give the expected namespaces and none is left unknown.

Since 1.17, MCPConfig's own class names are ids (`net/minecraft/src/C_3391_`), not Mojang names:
Mojang class names at runtime come only from the `--classes` merge.

## Per row

### official / vanilla / ≤ 1.21.11 → `official`

- `client.jar` 1.21.11, SHA-1 `ba2df812c2d12e0219c489c4cd9a5e1f0760f5bd`,
  SHA-256 `1473c9489ac50fda3c435049a76a70d61a10b8610db27f5ba9d8756b686cd3bd`.
  Only 33 of 10 291 classes are under `net/minecraft/`; contains `a.class`, `fud.class`.
- `client_mappings` 1.21.11 (SHA-1 `031a68bebf55d824f66d6573d8c752f0e1bf232a`):
  `net.minecraft.client.Minecraft -> gfj:`.
- Version descriptor 1.21.11 (SHA-256 `cc717c0d11f2447690a13069401368969671a90d0503d2848b9e4438ae82d866`)
  declares `client_mappings`.

### official / vanilla / ≥ 26.1 → `mojang`

- `client.jar` 26.1, SHA-1 `191771837687b766537a8c4607cb6fad79c533a1`,
  SHA-256 `bc6194c61566b587f196090d730698f2191170f7580adb143e4ff839939d3840`.
  10 208 of 10 682 classes under `net/minecraft/`; contains `net/minecraft/client/Minecraft.class`.
- Version descriptor 26.1 (SHA-256 `7680e85c9aee9cea1ec41370d9e7e93fbd1d527f88f8edbf7ec736f3184a9533`)
  has **no** `client_mappings`;
  1.21.11 is the last release with it (manifest of 2026-11-08: releases 26.1 → 26.3 after 1.21.11).

### official / fabric, quilt / ≤ 1.21.11 → `intermediary`

- Fabric profile `loader/1.21.11/0.20.1/profile/json`
  (SHA-256 `7655f564fef0c8a241fac03e4bec80ad8ae7a640ebe68a22c52889d0a2f8fe21`):
  libraries include `net.fabricmc:intermediary:1.21.11`.
- Quilt profile `loader/1.21.11/0.31.0-beta.6/profile/json`
  (SHA-256 `6b0dbf25c819c5a04a5b4a8761cc569dd683197b879131f6283b6cfe32c96016`):
  libraries include `net.fabricmc:intermediary:1.21.11` and `org.quiltmc:hashed:1.21.11`.
- `quilt-loader-0.31.0-beta.6.jar` (SHA-256 `f14bf8d5ec41be892f161526d2bc509ef3f7eea4184ce02d071b818fe56ab5ee`):
  `org/quiltmc/loader/impl/game/MappingConfigurationImpl.class` holds the target namespaces
  `named`, `intermediary` and the error `Unable to locate intermediary! Something is very wrong.`;
  `MinecraftGameProvider.class` finds the game jar by `net/fabricmc/intermediary/.../intermediary-*.jar`
  and uses `official` for unobfuscated versions. Neither class mentions `hashed`. A user can force
  another target with `loader.experimental.minecraft.targetNamespace`.

### official / fabric, quilt / ≥ 26.1 → `mojang`

- Fabric profile 26.1 (SHA-256 `51640010d18dd8737a9883508fdc692a086e368c0e5a01c66f854ff93bf5df21`)
  and Quilt profile 26.1 (SHA-256 `7b70e1cac1a7ed46951400cbd38287af896562b5653328419612b608f720e8ed`):
  no intermediary library, only the loader.
- `meta.fabricmc.net/v2/versions/intermediary/26.1` lists 26.1 but points to the placeholder
  `net.fabricmc:intermediary:0.0.0`; the Maven directory `intermediary/26.1/` does not exist (404).
  Detection rule: "listed" is not enough, the profile must reference a real intermediary artifact.

### official / forge / 1.7.10 → 1.16.5 → `srg` (MCP names)

| Installer | SHA-256 | Observed |
| --- | --- | --- |
| `forge-1.7.10-10.13.4.1614-1.7.10-installer.jar` | `a5d60757d8b305939233a942d5097fa78816d116ef9082c03066fe669e010c7f` | legacy profile (`install`/`versionInfo`), universal jar contains `deobfuscation_data-1.7.10.lzma` |
| `forge-1.12.2-14.23.5.2864-installer.jar` | `2c0065938de6df6f3deb4a08a3018940a1cc86111adb34a3be55d657857a11cc` | `spec: 0`, no processors; Forge jar contains `deobfuscation_data-1.12.2.lzma`, `FMLDeobfuscatingRemapper` |
| `forge-1.13.2-25.0.223-installer.jar` | `d46cb326e7dc32695084090c8169bb57f32f1c23b801ddb35a336f252e381191` | `MAPPINGS = mcp_config:1.13.2-20190213.203750`, `SpecialSource --in-jar {MC_SLIM} --out-jar {MC_SRG} --srg-in {MAPPINGS}` |
| `forge-1.16.5-36.2.42-installer.jar` | `dfa90c681f30be2406548b6429bccb68d95af4f64ab19b8c58f6128fd50ff857` | `MAPPINGS = mcp_config:1.16.5-20210115.111550`, `SpecialSource --in-jar {MC_SLIM} --out-jar {MC_SRG} --srg-in {MAPPINGS}` |

`mcp_config-1.16.5-20210115.111550.zip` (SHA-256 `80216a0f4ae79d81a5e11ceccaa97582a1f6a2cf079b7703b88a99a26547b342`),
`config/joined.tsrg`: `djz net/minecraft/client/Minecraft`, members `field_142025_a`, `field_211502_b`.

### official / forge / 1.17 → 1.20.4 → classes `mojang`, members `srg`

| Installer | SHA-256 | Observed |
| --- | --- | --- |
| `forge-1.17.1-37.1.1-installer.jar` | `31f2319b1b9ca491aaccc020887240f8524a4ff2a8e0f8b795e17e889cbc043b` | `MERGE_MAPPING ... --classes --reverse-right`, `vignette --mappings {MERGED_MAPPINGS}` |
| `forge-1.18.2-40.3.12-installer.jar` | `dcff493f6c2212bdf6ae087f98029cd317a79759ab1009dce23e63e7dba0e216` | `MERGE_MAPPING ... --classes --reverse-right`, `ForgeAutoRenamingTool:0.1.25 --names {MERGED_MAPPINGS}` |
| `forge-1.19.2-43.5.2-installer.jar` | `cee6837e368a579e10b6c9677a29e789b5a7e424ea6143996e1c8794d0b01cf1` | same as 1.18.2 (`ForgeAutoRenamingTool:0.1.22`) |
| `forge-1.20.1-47.4.29-installer.jar` | `a60f4e219abc6ae5ab5c9c0458741fbb8aba15deaf444810ff61bf3e94e907f4` | `MERGE_MAPPING ... --classes --reverse-right`, `ForgeAutoRenamingTool --names {MERGED_MAPPINGS}` |
| `forge-1.20.4-49.2.13-installer.jar` (last 1.20.4 build) | `d85b979476eabfd31aa7e2f5a059ee9fff7f49422f2f081279c0109ea46680ab` | same as 1.20.1 |

`mcp_config-1.20.1-20230612.114412.zip` (SHA-256 `f46d1050d8bed9046886b90a0d2fb80b4c6b5120fc35d32e5d37498dbe0a6d2a`):
`enn net/minecraft/src/C_3391_ 3391` / `N ()Lenn; m_91087_`; Mojang 1.20.1: `getInstance() -> N`
on `net.minecraft.client.Minecraft -> enn`. Classes come from Mojang, members stay `m_`.

The runtime class names equal Mojang's, so the model records this row as classes `mojang`,
members `srg`.

### official / forge / ≥ 1.20.6 → `mojang`

| Installer | SHA-256 | Observed |
| --- | --- | --- |
| `forge-1.20.6-50.0.0-installer.jar` (first 1.20.6 build) | `c3d07091f685e4b96de5853cd3dd2f20220d9829d28e46e9cc532181750c8af0` | rename `--names {MOJMAPS} --reverse`, output `{MC_OFF}` |
| `forge-1.20.6-50.2.15-installer.jar` | `69e5be0258757de8ac84217fdcb81ec55942171a30b1c2b0dcc4eb77dd7cf40e` | same |
| `forge-1.21-51.0.0-installer.jar` | `3dd76c0b5d827f8e0ca29de3f483f0068c0e1ae6a3c3881ac39eb3037fa1c1b7` | same (still declares unused `MAPPINGS`) |
| `forge-1.21.1-52.1.21-installer.jar` | `3a67cbffd8761df06256cf7bf31abf9e056edba686082270e9eef9a87cb84901` | same, no `MAPPINGS` |
| `forge-1.21.11-61.2.5-installer.jar` | `4b1a7d08cd598f7e2513c10c239acc581b3934a455ea9752a1ef43de02914221` | same |
| `forge-26.1-62.0.9-installer.jar` | `b18b0fd7a28d2b20a6f623c0bc904a811d7def110b3aa81676e533c3b66c05af` | only `binarypatcher` |
| `forge-26.3-66.1.5-installer.jar` | `4ba54a1bc216ad12dde032eecb999cb3c617b1d0c92ad67add4b6531f45e58b4` | only `binarypatcher` |

Forge Maven has no 1.20.5 build (`maven-metadata.xml`: 1.20.3 ×2, 1.20.4 ×79, then 1.20.6).

### official / neoforge / 1.20.1 (`net.neoforged:forge`) → classes `mojang`, members `srg`

- `forge-1.20.1-47.1.106-installer.jar` (SHA-256 `c1ea1c3be532c4444004efd7f9cc71e1590d010ff44845b93effba61e3bd1526`):
  `MERGE_MAPPING ... --classes --reverse-right`, `--names {MERGED_MAPPINGS}`.
- Universal jar (SHA-256 `af67e430f39dc742713c94975166fac29c1a49f5ba7c31095a77289931399dc1`)
  `accesstransformer.cfg`: `public net.minecraft.client.Minecraft f_90987_ # textureManager` (120 `m_`, 163 `f_`).

### official / neoforge / ≥ 1.20.2 (`net.neoforged:neoforge`) → `mojang`

| Installer | SHA-256 | Observed |
| --- | --- | --- |
| `neoforge-20.2.3-beta-installer.jar` (first 20.2 build) | `c09b102dfbb29f5ad2129e590fdedd4f3b11f2b2b7fb62ed35438feb67f8da55` | `MERGE_MAPPING ... --classes --fields --methods --reverse-right`, `--names {MERGED_MAPPINGS}` |
| `neoforge-20.4.253-installer.jar` | `8f1413379067059acc0151444b17536cfad97756d99fc70e45587bf229c986fa` | same |
| `neoforge-20.5.0-beta-installer.jar` | `733facf3a474cea43879e2d0ac22e5a58cdaecb15ad3152e7699e33200af907c` | same |
| `neoforge-20.6.1-beta-installer.jar` | `3797f7a6d91b7d644171e0095da48a7fb922b534fe9f618530c3386b0b342451` | same |
| `neoforge-21.1.265-installer.jar` | `a547ccc30a80a675d1c61d04b55d10c45c39a86971e09731482c50a24399d1a2` | same |
| `neoforge-21.11.48-installer.jar` | `1f14a4bac4aa885a497550fa5a8374451b7014cec27e1bb69ab16742fb689db0` | `PROCESS_MINECRAFT_JAR --input-mappings {MOJMAPS}` |
| `neoforge-26.1.2.126-installer.jar` | `1184ffee181bd7a940691539fea07f237096378da4c94795247ecdb9cb354a1b` | `PROCESS_MINECRAFT_JAR`, no mappings |
| `neoforge-26.3.0.101-installer.jar` | `6bac566026d3f18b1dd9a529e7a4041fbc3141841e72d869c54f06730a38f09d` | `PROCESS_MINECRAFT_JAR`, no mappings |

Universal jars: `accesstransformer.cfg` has 0 `m_`/`f_` entries and uses Mojang names
(`public net.minecraft.client.Minecraft textureManager`).

| Universal jar | SHA-256 |
| --- | --- |
| `neoforge-20.2.3-beta-universal.jar` | `4906f318b2ba191b7accbd5c84674b62071741a66f1437fe7eec037f595e91e9` |
| `neoforge-20.4.253-universal.jar` | `fb939de1e6aa86bba29255a8734380a993e66986bd76aa38a723c0275cb1b975` |
| `neoforge-20.5.0-beta-universal.jar` | `0d21f02605314082df3c9b60f6aa29d284cbd860bb572f3a33d6a5fbc76839a0` |
| `neoforge-20.6.1-beta-universal.jar` | `abcba970a13683fea383a07387715e0235fc313164deab2e2d86b01264c7e63e` |
| `neoforge-21.1.265-universal.jar` | `c12cf1a6cb0aa7792b13174b06088a4f54d8bcc92f6a32fcb03310b456936847` |
| `neoforge-21.11.48-universal.jar` | `95882e7dda195b8cdf8c7fcddd69c32b9163ca1c84cd33ea47a1afc9e51960a6` |
| `neoforge-26.3.0.101-universal.jar` | `bc8904db92ad31f721a5cb7a20cd2565deccb66809fd04e8169a7d5a43bbcbf8` |

NeoForm `config/joined.tsrg` for 1.20.4, 1.20.5 and 1.21.1 (`neoform-1.20.4-20240627.114801.zip`
SHA-256 `31798da3449d3e296a0761b0a05bbbca1a9d85eaf55302e9a258f6cce7e38fe6`) maps `obf → obf`
with ids (`a a 252363`): its `srg` column carries no names, everything comes from Mojang.

### lunar / any / any → detected

- Local install `%USERPROFILE%\.lunarclient\offline\multiver` (1.8 and 1.21.11 present).
  It ships no Minecraft class jar there; Lunar code is in `*-nomappings.jar` and mappings in
  `lunar-platform-mappings-v1_21_11.jar` (SHA-256 `43fcaf7012c78d32b7285611ca7cd6b0994a1417b1cb7308ed13395a571f5825`):
  `fabric/intermediary_1.21.11.kin`, `lunar/lunar2intermediary_1.21.11.kin`,
  `lunar/lunar_named_b5_1.21.11.kin`, `v1_21_11_inflight_vanilla.kin` (proprietary `.kin` format).
  `lunar-platform-mappings-v1_8.jar`: SHA-256 `d72270adbd6386cc6f18c96c4ea2fc1382f315139eddb80c74b295e5644946c8`.
- Observation: Lunar remaps the game at launch into a Lunar-specific namespace (bridged to
  intermediary for Fabric mods). Nothing static decides it; the row stays "detected".

#### Detected on 2026-11-10 (Lunar launcher 3.7.34)

Each configuration was launched once from the Lunar launcher, then captured with
`mc_mappings_generator scan`. The runtime jar is Lunar's class cache (`bake.zip`, written after
Lunar's transformations); its entries are class files named `a.b.C`. Classpath checksums are
the ones Lunar logs in `ichor-boot.log`: `genesis-0.1.0-SNAPSHOT-all.jar` `a425c132`,
`lunar.jar` `bc92bf50`, `lunar-platform-mappings-v1_21_0.jar` `bcd49680` (1.21),
`lunar-platform-mappings-v1_21_11.jar` `302edf01` (1.21.11). 1.8.9: `lunar-platform-mappings-v1_8.jar` `d70f9ed7`,
`legacy-0.1.0-SNAPSHOT-all-nomappings.jar` `6948127b`; its boot log already shows MCP names
(`TextureMap::loadTextureAtlas`). Lunar changed one static flag there
(`FaceBakery.getFaceBrightness`, reported as a warning) and names 124 synthetic members of
anonymous classes differently (not exported). Fabric module: loader 0.20.1.

| Version | Module | Class cache SHA-256 | Class scores | Member scores | Verdict |
| --- | --- | --- | --- | --- | --- |
| 1.21 | lunar | `7b4c7b1802610841549ac9d96abc1c101318aaa84ed2690b777459f4d57bce9b` | mojang 1.0, official 0.0031, yarn 0.0097, intermediary 0.0021 | mojang 1.0, yarn 0.3274 | `mojang` / `mojang` |
| 1.21 | fabric | `6c6faafc2e32c8ed9abe39da67e3bc3b40f9dcb1030db6ef4fda1b62ada17457` | intermediary 1.0, mojang 0.003 | intermediary 1.0, yarn 0.1679 | `intermediary` / `intermediary` |
| 1.21.11 | lunar | `1c5cf18835f4c7cf02e0a73bf637c143ffc9162a290427061535d3e9f7060964` | mojang 1.0, yarn 0.0143 | mojang 1.0, yarn 0.3378 | `mojang` / `mojang` |
| 1.21.11 | fabric | `c6b70dd3a52da4783bf3d4ce16e725327070e18893d6e8413242343e2e66de16` | intermediary 1.0, mojang 0.0062 | intermediary 1.0, yarn 0.1669 | `intermediary` / `intermediary` |
| 1.8.9 | lunar | `6ffcadd60129067579059c62adb97b9d95331c4cbe101085c9db81bb9928dd8d` | mcp 1.0, srg 1.0 (same class names), official 0.0132 | mcp 1.0, srg 0.1962, official 0.0542 | `mcp` / `mcp` |

Scores count only symbols present at runtime under some candidate name. The class caches lack a
few game classes (1.21: 175 with Fabric, 2 without; 1.21.11: 223 with Fabric, 1 without), and
the module without loader lacks members Lunar removed or changed (1.21: 208, 1.21.11: 372, e.g.
`OptionInstance.codec()`, `Util$2.<init>(ForkJoinPool)` which becomes `<init>(Function)`).
These are listed in each report and not exported. Unloaded, unbaked classes cannot be checked:
a future capture that bakes them will add them.

### NeoForge ≥ 1.21.6: installertools 3.x merge

`neoforge-21.6.20-beta-installer.jar` (SHA-256 `e85b8ed2614f2ab3f10d2d179064e3e81f5836519a074d22b8a8460f2e91b5d5`)
client processors: `MERGE_MAPPING --merge {MAPPINGS} --base {MOJMAPS} --output {MERGED_MAPPINGS} --reverse-base`,
then `AutoRenamingTool --names {MERGED_MAPPINGS}`. `MAPPINGS` is
`neoform-1.21.6-20250617.151856.zip` (SHA-256 `f537eca06638ed23796d094ab9315c6660ebb142b067c791d68bf4e6469df32a`):
`config.json` has `"official": true` and `config/joined.tsrg` is identity (`a a 252363`).
All names therefore come from the Mojang base: `mojang` / `mojang` (same for 21.7, 21.8, 21.9).

### Excluded: Forge 1.16.2

`forge-1.16.2-33.0.61-installer.jar` (SHA-256 `961fb78089c638a8a70f178bb6e51880bf5d4c027830c03da27031d1e0ad72b1`)
uses `mcp_config:1.16.2-20200812.004259`, whose `joined.tsrg` maps two distinct synthetic lambdas
of `ServerWorld` (official `a` and `b`, both `(Laag;Lfx;)V`, Mojang `lambda$makeObsidianPlatform$16`
and `$17`) to the same SRG name `func_241122_a_`. The runtime names of that class cannot be exact;
the combination is excluded in `matrix-overrides.json`.

## Era boundaries

| Boundary | Result | Evidence |
| --- | --- | --- |
| Last version with Mojang ProGuard mappings | 1.21.11 | 26.1 descriptor has no `client_mappings` |
| First unobfuscated version | 26.1 | 26.1 `client.jar` classes under `net/minecraft/` |
| Last Forge version with SRG members at runtime | 1.20.4 (49.2.13) | `MERGED_MAPPINGS` with `--classes` only |
| First Forge version with Mojang runtime | 1.20.6 (50.0.0) | `--names {MOJMAPS} --reverse` |
| NeoForge SRG runtime | 1.20.1 only (`net.neoforged:forge`) | first `net.neoforged:neoforge` build 20.2.3-beta merges all members from Mojang |
| End of MCPConfig as runtime source | Forge 1.20.4 / NeoForge 1.20.1 | install profiles above |
| `mcp_config` publication | **does not end**: published through 26.3 (`mcp_config/maven-metadata.xml`) | presence of `mcp_config` must not be used as an era signal |
