//! Minecraft Java Edition mappings generator. See `spec/`.
//!
//! Module call rule (by convention): main → pipeline → sources / formats / classfile → model.

fn main() {
    println!("mc_mappings_generator {}", env!("CARGO_PKG_VERSION"));
}
