# Formats

## Input formats

| Format | Used for | Notes |
| --- | --- | --- |
| ProGuard (`.txt`) | Mojang `client_mappings` | maps `mojang → official`; Java type syntax converted to descriptors; line numbers ignored |
| Tiny v1 | older Intermediary/Yarn artifacts | |
| Tiny v2 | Intermediary, Yarn, Lunar Mapping Project | escapes, properties, comments, params ignored |
| SRG | MCP 1.7.10 → 1.12.2 (`joined.srg`) | `PK`/`CL`/`FD`/`MD` lines |
| TSRG / TSRG2 | MCPConfig (`joined.tsrg`) | TSRG2: multiple namespaces, `static` markers, ids |
| MCP CSV | `fields.csv`, `methods.csv` | `searge → name` |

Parsers are streaming, report errors with line numbers and reject malformed input; they never skip
lines silently.

## JSON output (`mappings`, version 1)

One UTF-8 file per configuration. Keys sorted as below; arrays sorted by `runtime` name.

```json
{
  "format": "mappings",
  "format_version": 1,
  "configuration": {
    "minecraft_version": "1.21.11",
    "launcher": "official",
    "loader": "fabric",
    "loader_version": "0.17.3"
  },
  "namespaces": ["runtime", "official", "mojang", "yarn"],
  "aliases": { "runtime": "intermediary", "named": "mojang" },
  "classes": [
    {
      "names": { "runtime": "net/minecraft/class_310", "official": "fud", "mojang": "net/minecraft/client/Minecraft" },
      "access": 33,
      "super": "java/lang/Object",
      "interfaces": [],
      "fields": [
        {
          "names": { "runtime": "field_1724", "official": "s", "mojang": "player" },
          "descriptor": "Lnet/minecraft/class_746;",
          "access": 1,
          "static": false
        }
      ],
      "methods": [
        {
          "names": { "runtime": "method_1551", "official": "Q", "mojang": "getInstance" },
          "descriptor": "()Lnet/minecraft/class_310;",
          "access": 9,
          "static": true
        }
      ]
    }
  ],
  "provenance": {
    "generator": "mc_mappings_generator 0.1.0",
    "sources": [{ "id": "mojang.client_mappings", "url": "...", "sha256": "...", "license": "Mojang" }],
    "runtime_detection": { "method": "derived", "classes": "intermediary", "members": "intermediary" },
    "verification": { "errors": 0, "warnings": 0 },
    "coverage": { "classes": 1.0, "fields": 0.99, "methods": 0.98 }
  }
}
```

Rules:

- `runtime` is always the first namespace; `descriptor`, `super` and `interfaces` use runtime names.
  The physical namespace behind `runtime` (`aliases.runtime`) is not repeated as a separate key.
  If classes and members come from different namespaces, `aliases.runtime` is an object
  `{ "classes": "...", "members": "..." }`.
- A namespace without a name for a symbol is omitted from that symbol's `names` (no `null`).
- `access` is the raw JVM access-flags value; `static` is redundant but explicit for consumers.
- Options: `--namespaces` restricts exported namespaces (`runtime` always kept); `--pretty`
  indents; `--gzip` compresses. Compact, non-gzipped output is the default.
- Breaking changes increment `format_version`. The JSON Schema of the current version is embedded
  in the binary and written on request (`schema` command, [cli.md](cli.md)); tests validate
  outputs against it.

JSON is the generated format. Tiny v2 is available as a conversion of a JSON output (below).

## Tiny v2 output

`mc_mappings_generator tiny FILE -o OUT` converts one `mappings` file (gzipped or not) to Tiny v2:

- Header `tiny\t2\t0\t<namespaces>` (`\t` is a tab), with the file's `namespaces` in order
  (`runtime` first), then the property line `\tescaped-names`; names are escaped
  (`\\`, `\n`, `\r`, `\t`, `\0`).
- One `c` line per class, then `\tf\t<descriptor>` and `\tm\t<descriptor>` lines for its members,
  in the file's order. Descriptors are the file's runtime descriptors (Tiny's first namespace).
- A namespace without a name for a symbol is an empty column.

Names and descriptors round-trip without loss (tested: JSON → Tiny v2 → Tiny reader). Tiny v2 has
no place for access flags, `aliases` or provenance: those stay in the JSON file.

Provenance keys are sorted alphabetically; `sources` lists every input (`id`, `url`, `size`,
`sha256`, `license`). Lunar inputs use `lunar:<path inside .lunarclient>` URLs, never local paths.
For Lunar configurations, `ns0` is the runtime namespace and Lunar's own classes and members are
exported with their runtime names only.

## Report (`.report.json`)

One per configuration, written even when verification fails: `configuration`, `errors` (non-empty
blocks the export), `warnings`, `pruned_not_in_client_jar` (symbols named by mapping sources but
absent from the client jar, e.g. server-only), `coverage` and `coverage_gaps` (per kind, for the
`named` namespace), `runtime_detection` (method, namespaces, evidence or detection scores) and, for
Lunar, `not_exported_absent_at_runtime` / `members_not_exported_absent_at_runtime`.

## mc-mappings dataset

`mc-mappings` converts generated files into the mc-mappings table format (schema version 1,
`mc_mappings/spec/table-format.md`). It is a downstream conversion: the `mappings` output above is
unchanged.

Input catalog (`format: "mc_mappings_catalog"`, `format_version: 1`):

| Array | Entry |
| --- | --- |
| `classes` | `id`, `mojang` (internal name) |
| `fields` | `id`, `owner` (class id), `mojang` (name), `type` (mc-mappings type object) |
| `methods` | `id`, `owner`, `mojang`, `parameters`, `returns` (mc-mappings type objects) |

Any entry may add `mojang_by_version` (`{ "<minecraft_version>": "<name>" }`) for symbols Mojang
renamed (1.21.11: `ResourceLocation` → `Identifier`). Types reference classes by catalog id;
`external` types must not name a mapped game class.

For each input file: classes are found by Mojang name (the `mojang` names, or `runtime` when the
runtime namespace is `mojang`); each member is looked up by Mojang name and runtime descriptor in
its owner, then in its ancestors; the class that declares it must be a catalog class and becomes
the record `owner`. `static` comes from the file. Any missing symbol, undeclared owner or two ids
on one runtime member fails the export with every problem listed.

Launchers map to `standard`/`lunar`, loaders to `none` (vanilla, Lunar module without loader),
`fabric`, `forge`; other loaders are rejected. Configurations whose records are identical share one
profile, named after all of them (`mc1_21_lunar_fabric.mc1_21_standard_fabric`). Output: the
dataset (`--data-version` is its `data_version`) and `<name>.provenance.json` (catalog SHA-256 and,
per input, file name, SHA-256, configuration and provenance). Same inputs give the same bytes.
