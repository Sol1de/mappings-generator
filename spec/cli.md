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
| `mc_mappings_generator tiny FILE -o OUT` | Convert a generated file to Tiny v2 ([formats.md](formats.md#tiny-v2-output)) |
| `mc_mappings_generator cache info | clean [--older-than D]` | Inspect or clean the cache (`D` in days) |
| `mc_mappings_generator mc-mappings --catalog C --data-version N -o FILE INPUT...` | Convert generated files into an mc-mappings dataset ([formats.md](formats.md#mc-mappings-dataset)) |

## Global options

`--cache-dir`, `--offline`, `--locked FILE`, `--jobs N`, `--namespaces a,b`, `--pretty`,
`--gzip`, `-q`, `--verbose` (repeatable), `--json` (machine-readable output for listing commands
and reports), `--overrides FILE` (default `./matrix-overrides.json` when present). Verbosity has no
short flag: `-v` is the Minecraft version of `generate`. `--jobs` defaults to 4.

## Output layout

```text
DIR/<minecraft_version>/<launcher>-<loader>[-<loader_version>].json
DIR/<minecraft_version>/<launcher>-<loader>[-<loader_version>].report.json
DIR/mappings.lock.json
DIR/generate-all.report.json          (with --all: outcome of every configuration)
```

`--locked FILE` replays the inputs pinned by a lock file: every URL fetched (metadata included)
must be listed there with its SHA-256, and loader builds default to the locked ones. A lock
entry is `{ configuration, inputs: [{ id, url, size, sha256, license }] }`; `fetched` maps every
URL to its SHA-256.

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
- Lunar: for each profile in `%USERPROFILE%\.lunarclient\profiles`, the last launch is read from
  `logs/ichor-boot.log` (classpath with checksums, class cache path, Fabric loader) and matched by
  time with the launcher log `logs/launcher/main*.log` (version, module, loader build); both must
  agree. Its class cache is copied into the generator cache with a record
  (`<cache>/lunar/<version>-<module>.json`), so configurations launched one after another stay
  available. A launch the logs no longer describe is skipped with a warning. To capture a Lunar
  configuration: launch it from the Lunar launcher, then run `scan`.
