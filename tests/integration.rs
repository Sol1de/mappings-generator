//! Network integration tests (spec/architecture.md#testing): one representative configuration
//! per era and loader, plus reproducibility. Run with `cargo test -- --ignored`.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_mc_mappings_generator");

fn generate(out: &Path, extra: &[&str], args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(extra)
        .args(["-q", "generate"])
        .args(args)
        .arg("-o")
        .arg(out)
        .output()
        .unwrap()
}

#[test]
#[ignore = "network"]
fn every_era_and_loader_generates_without_verification_errors() {
    let out = tempfile::tempdir().unwrap();
    let configs = [
        ["1.7.10", "forge"],
        ["1.8.9", "vanilla"],
        ["1.12.2", "forge"],
        ["1.13.2", "forge"],
        ["1.14.4", "fabric"],
        ["1.16.5", "forge"],
        ["1.20.1", "forge"],
        ["1.20.1", "neoforge"],
        ["1.20.1", "quilt"],
        ["1.20.4", "neoforge"],
        ["1.21.1", "forge"],
        ["1.21.11", "vanilla"],
        ["1.21.11", "fabric"],
        ["1.21.11", "forge"],
        ["26.1", "vanilla"],
        ["26.1", "fabric"],
    ];
    let mut failures = Vec::new();
    for [v, l] in configs {
        let o = generate(out.path(), &[], &["-v", v, "-l", l]);
        if !o.status.success() {
            failures.push(format!("{v} {l}: {}", String::from_utf8_lossy(&o.stderr)));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "network and a local Lunar capture (`scan` after launching Lunar)"]
fn lunar_sample() {
    let out = tempfile::tempdir().unwrap();
    // Modern version with each module, plus the legacy one (1.8.9 offers no Fabric module).
    for (version, module) in [
        ("1.21.11", "fabric"),
        ("1.21.11", "lunar"),
        ("1.8.9", "lunar"),
    ] {
        let o = generate(
            out.path(),
            &[],
            &["-v", version, "--launcher", "lunar", "-l", module],
        );
        assert!(
            o.status.success(),
            "{version} {module}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
}

#[test]
#[ignore = "network"]
fn same_lock_gives_byte_identical_outputs() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let o = generate(a.path(), &[], &["-v", "1.21.11", "-l", "fabric"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let lock = a.path().join("mappings.lock.json");
    let o = generate(
        b.path(),
        &["--offline", "--locked", lock.to_str().unwrap()],
        &["-v", "1.21.11", "-l", "fabric"],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    for f in [
        "official-fabric-0.20.1.json",
        "official-fabric-0.20.1.report.json",
    ] {
        let (x, y) = (
            a.path().join("1.21.11").join(f),
            b.path().join("1.21.11").join(f),
        );
        assert_eq!(std::fs::read(x).unwrap(), std::fs::read(y).unwrap(), "{f}");
    }
}
