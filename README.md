# Minecraft mappings generator

Generates class, method and field mappings for Minecraft Java Edition releases (1.7.10 → latest)
across Vanilla, Fabric, Quilt, Forge and NeoForge, for the official launcher and Lunar Client.
Exports JSON (`mappings`). Command-line only, for humans and agents. Windows x64. Pure Rust, no JVM.

**Status:** skeleton — not functional yet.

- Specifications: [`spec/`](spec/README.md)
- Quality gates: `sh scripts/check.sh` (fmt, clippy `-D warnings`, tests)
