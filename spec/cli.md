# Command-line interface

Binary name: `mc_mappings_generator`. The CLI is the only interface (no GUI): generation is a
batch, reproducible, scriptable task, run by humans and by agents alike. Hence:

- every command is non-interactive (no prompt, no confirmation);
- `--json` gives machine-readable output; exit codes are stable (below);
- progress and logs go to stderr, results to stdout.

## Commands

| Command | Purpose |
| --- | --- |
| `mc_mappings_generator versions [--loader L] [--launcher X]` | List supported Minecraft releases |
| `mc_mappings_generator matrix [--version V]` | List existing configurations with their default loader build and expected runtime namespace |
| `mc_mappings_generator generate -v V [-l LOADER] [--launcher X] [--loader-version B] -o DIR` | Generate one configuration |
| `mc_mappings_generator generate --all [filters] -o DIR` | Generate every configuration of the matrix (filters: `--since`, `--until`, `--loader`, `--launcher`) |
| `mc_mappings_generator verify FILE` | Re-run verification on an exported file against its runtime jars |
| `mc_mappings_generator scan [--minecraft-dir P] [--lunar-dir P]` | Detect locally installed configurations (official launcher profiles, Lunar) and optionally generate them (`--generate`) |
| `mc_mappings_generator schema -o FILE` | Write the JSON Schema of the output format ([formats.md](formats.md)) |
| `mc_mappings_generator cache info | clean [--older-than D]` | Inspect or clean the cache |

## Global options

`--cache-dir`, `--offline`, `--locked [FILE]`, `--jobs N`, `--namespaces a,b`, `--pretty`,
`--gzip`, `-q`, `-v/-vv`, `--json` (machine-readable output for listing commands and reports).

## Output layout

```text
DIR/<minecraft_version>/<launcher>-<loader>[-<loader_version>].json
DIR/<minecraft_version>/<launcher>-<loader>[-<loader_version>].report.json
DIR/mappings.lock.json
```

Output files are written only at the path given by `-o` (no default location); every written
path is printed once done. The cache ([pipeline.md](pipeline.md#cache-and-integrity)) is the only
other place the tool writes to.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Generation or verification error |
| 2 | Invalid usage / unknown configuration |
| 3 | Network or cache error (including `--offline` miss) |
| 4 | Partial success with `--all` (some configurations failed; see reports) |

## Local installation scan

- Official launcher: `<minecraft_dir>/versions/*/*.json` (follow `inheritsFrom`), recognising
  Vanilla, Fabric, Quilt, Forge and NeoForge profiles and their loader builds.
- Lunar: `%USERPROFILE%\.lunarclient` offline directories (versions, modules, jars with hashes).
