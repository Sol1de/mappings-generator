# Generation pipeline

## Steps for one configuration

1. **Resolve** the configuration against the matrix ([scope.md](scope.md)); unknown → error.
2. **Fetch** the required artifacts (client jar, mapping files, loader metadata, Lunar jars)
   through the cache.
3. **Build** a `MappingSet` with `official` as `ns0`:
   - load each available source (ProGuard, Tiny, SRG family, MCP CSV);
   - compose/merge them onto `official` ([model.md](model.md));
   - for unobfuscated versions, `official` and `mojang` are the jar names.
4. **Reconstruct** from the client jar: `attach_jar`, `propagate`, `complete`.
5. **Determine the runtime namespace**: derived for the official launcher, detected for Lunar
   ([namespaces.md](namespaces.md)). Set the `runtime` and `named` aliases.
6. **Verify** (below). Errors abort; warnings go to the report.
7. **Export** as JSON ([formats.md](formats.md)) with the provenance record.

Configurations are independent and run in parallel. Work shared by several configurations of the
same version (jar inventory, base `MappingSet`) is computed once and reused.

## Verification

| Check | Severity |
| --- | --- |
| Every class/member of the runtime namespace exists in the runtime jars with the same descriptor and static flag | error |
| No duplicate name per namespace (model invariants) | error |
| Detection verdict unambiguous | error |
| Composition losses (entries dropped between namespaces) | warning, counted |
| Coverage: share of jar symbols with a `named` name, per kind | report |

For the official launcher, the runtime jars are the client jar remapped into the runtime
namespace (Forge/NeoForge patches are not applied): the remapper keeps names its mappings do not
cover, so a runtime name missing from the sources is the `ns0` name, after inherited method names
were propagated. For Lunar, the runtime jar is Lunar's class cache; game symbols it lacks are not
exported and are listed in the report ([namespaces.md](namespaces.md#detection-lunar-and-verification)).

Symbols named by mapping sources but absent from the client jar (server-only classes of merged
tables) are dropped before verification and counted in the report.

`verify FILE` regenerates the file's configuration from the cache and checks every symbol of the
file against its runtime jars, reporting wrong names, descriptors and static flags.

## Cache and integrity

- Location: the OS cache directory (`%LOCALAPPDATA%\mc_mappings_generator` on Windows), overridable by `--cache-dir`.
- Immutable artifacts are content-addressed by SHA-256 and checked against the published SHA-1
  or hash when upstream provides one; mismatch → error, file discarded.
- Mutable metadata (manifests, maven-metadata) has a TTL (default 1 h); `--offline` uses the cache only
  and fails if an entry is missing.
- Downloads are parallel, with retries and exponential backoff on transient errors.

## Determinism and provenance

- Outputs are sorted and serialised without timestamps or machine-specific paths.
- `mappings.lock.json` records, per configuration, every input (URL, size, SHA-256) and the loader build.
  With `--locked`, generation uses exactly these inputs or fails.
- Same lock + same `mc_mappings_generator` version ⇒ byte-identical outputs (tested).
- Each output carries a provenance block: `mc_mappings_generator` version, inputs, licences, runtime-namespace
  verdict and proof, verification summary.

## Performance targets

- One modern configuration (≈ 10k classes) generated in under 10 s from a warm cache.
- Full matrix from a warm cache bounded by CPU cores (no global lock); memory below 2 GB.
