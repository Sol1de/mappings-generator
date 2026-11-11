# mc-mappings dataset

Inputs for the [mc-mappings](../../../mc_mappings) table format (schema version 2).

- [`catalog-v1.json`](catalog-v1.json): the Minecraft v1 symbols needed by the mc-bridge API v1
  (`mc_bridge/spec/api-v1.md`, appendix), by Mojang name and semantic types, with per-version
  names, types and scopes where 1.21 and 1.21.11 differ ([format](../../spec/formats.md#mc-mappings-dataset)).
  It is the catalog of mc-mappings `data_version` 2 (`mc_mappings/spec/catalog.md`): 77 classes,
  89 fields and 175 methods in 1.21, 79 / 89 / 177 in 1.21.11. Changing it means editing this file,
  regenerating, and a new `data_version`.
- [`build-dataset.sh`](build-dataset.sh): generates the ten target configurations (1.21 and
  1.21.11 × standard Vanilla/Fabric/Forge, Lunar, Lunar + Fabric) and converts them.

```sh
# once per Lunar configuration: launch it in the Lunar launcher, then
mc_mappings_generator scan
# then
consumers/mc_mappings/build-dataset.sh OUT 2
# reproduce later, byte for byte, without network:
consumers/mc_mappings/build-dataset.sh OUT2 2 --offline --locked OUT/mappings/mappings.lock.json
```

Outputs: `OUT/minecraft-v1.json` (the dataset: catalog, ten configurations, shared profiles),
`OUT/minecraft-v1.provenance.json` (per configuration: source file hash, upstream inputs with
hashes and licences, runtime-namespace proof) and `OUT/mappings/` (full generated mappings and
reports). The dataset contains runtime names derived from Mojang mappings: see the licence column
of the provenance before redistributing it.
