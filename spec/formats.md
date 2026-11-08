# Formats

## Input formats

| Format | Used for | Notes |
| --- | --- | --- |
| ProGuard (`.txt`) | Mojang `client_mappings` | maps `mojang → official`; Java type syntax converted to descriptors; line numbers ignored |
| Tiny v1 | older Intermediary/Yarn artifacts | |
| Tiny v2 | Intermediary, Yarn, Lunar Mapping Project | escapes, properties, comments, params ignored |
| SRG | MCP 1.7.10 → 1.12.2 (`joined.srg`) | `PK`/`CL`/`FD`/`MD` lines |
| CSRG | where published | |
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

JSON is the only output format. Tiny v2 output is deferred to a later version (a conversion from
the JSON output).
