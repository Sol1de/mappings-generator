#!/usr/bin/env sh
# Quality gates (spec/architecture.md). Used locally and by CI.
set -eu
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
