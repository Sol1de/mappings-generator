# mc-mappings dataset

Inputs for the [mc-mappings](../../../mc_mappings) table format (schema version 1).

- [`catalog-v1.json`](catalog-v1.json): Minecraft v1 symbols needed by mc-bridge (session, player,
  world, entities, inventory, game thread), by Mojang name and semantic types. **Draft** until
  mc-bridge ticket B1 freezes the list; adding or removing a symbol only edits this file.
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
