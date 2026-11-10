#!/usr/bin/env sh
# Builds the mc-mappings dataset (table format schema 1) for its ten target configurations:
# Minecraft 1.21 and 1.21.11 x {standard vanilla, fabric, forge; Lunar, Lunar + Fabric}.
#
# Usage: consumers/mc_mappings/build-dataset.sh OUT_DIR DATA_VERSION [--locked LOCK]
# Lunar configurations need a local capture first: launch each one once in the Lunar
# launcher, then run `mc_mappings_generator scan` right after (see README.md).
set -eu
out=$1
data_version=$2
shift 2
here=$(dirname "$0")
bin=${MC_MAPPINGS_GENERATOR:-mc_mappings_generator}

for v in 1.21 1.21.11; do
  for l in vanilla fabric forge; do
    "$bin" -q "$@" generate -v "$v" -l "$l" -o "$out/mappings" >/dev/null
  done
  for m in lunar fabric; do
    "$bin" -q "$@" generate -v "$v" --launcher lunar -l "$m" -o "$out/mappings" >/dev/null
  done
done
# shellcheck disable=SC2046 # one argument per generated file
"$bin" mc-mappings --catalog "$here/catalog-v1.json" --data-version "$data_version" \
  -o "$out/minecraft-v1.json" $(ls "$out"/mappings/1.21/*.json "$out"/mappings/1.21.11/*.json | grep -v '\.report\.json$')
