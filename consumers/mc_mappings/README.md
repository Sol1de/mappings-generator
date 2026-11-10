# mc-mappings dataset

Inputs for the [mc-mappings](../../../mc_mappings) table format (schema version 1).

- [`catalog-v1.json`](catalog-v1.json): the symbols of the first mc-bridge API v1 catalog (session,
  player, world, entities, inventory, game thread), by Mojang name and semantic types. It is the
  catalog of mc-mappings `data_version` 1 (`mc_mappings/spec/catalog.md`); changing it means
  editing this file, regenerating, and a new `data_version`.
- [`catalog-v2-common.json`](catalog-v2-common.json): `catalog-v1.json` plus the symbols added by
  the revised mc-bridge API v1 (`mc_bridge/spec/api-v1.md`, appendix) that have the same types on
  every configuration (77 classes, 88 fields, 163 methods; exported and audited on the ten
  configurations). It is not published: the revised API also needs symbols whose type differs
  between 1.21 and 1.21.11 (not expressible in this catalog format yet) and symbols that exist in
  one version only (needs mc-mappings `schema_version` 2).
- [`build-dataset.sh`](build-dataset.sh): generates the ten target configurations (1.21 and
  1.21.11 × standard Vanilla/Fabric/Forge, Lunar, Lunar + Fabric) and converts them.

```sh
# once per Lunar configuration: launch it in the Lunar launcher, then
mc_mappings_generator scan
# then
consumers/mc_mappings/build-dataset.sh OUT 1
# reproduce later, byte for byte, without network:
consumers/mc_mappings/build-dataset.sh OUT2 1 --offline --locked OUT/mappings/mappings.lock.json
```

Outputs: `OUT/minecraft-v1.json` (the dataset: catalog, ten configurations, shared profiles),
`OUT/minecraft-v1.provenance.json` (per configuration: source file hash, upstream inputs with
hashes and licences, runtime-namespace proof) and `OUT/mappings/` (full generated mappings and
reports). The dataset contains runtime names derived from Mojang mappings: see the licence column
of the provenance before redistributing it.
