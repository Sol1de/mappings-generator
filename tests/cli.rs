//! CLI tests on offline fixtures (spec/cli.md): commands, exit codes, output files.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_mc_mappings_generator");

fn run(cache: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("--cache-dir")
        .arg(cache)
        .args(args)
        .output()
        .expect("binary runs")
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn schema_is_written_and_validates_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("schema.json");
    let o = run(dir.path(), &["schema", "-o", out.to_str().unwrap()]);
    assert!(o.status.success(), "{o:?}");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for f in ["vanilla-1.21.json", "lunar-fabric-1.21.json"] {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture(f)).unwrap()).unwrap();
        let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{f}: {errors:?}");
    }
    let mut bad: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("vanilla-1.21.json")).unwrap())
            .unwrap();
    bad["classes"][0]["names"]["runtime"] = serde_json::Value::Null;
    assert!(!validator.is_valid(&bad));
}

#[test]
fn exports_an_mc_mappings_dataset_with_shared_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("data").join("tables.json");
    let o = run(
        dir.path(),
        &[
            "mc-mappings",
            "--catalog",
            &fixture("catalog.json"),
            "--data-version",
            "7",
            "-o",
            out.to_str().unwrap(),
            &fixture("vanilla-1.21.json"),
            &fixture("lunar-fabric-1.21.json"),
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let d: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(d["schema_version"], 1);
    assert_eq!(d["data_version"], 7);
    assert_eq!(d["configurations"].as_array().unwrap().len(), 2);
    let lunar = &d["profiles"][0];
    assert_eq!(lunar["profile_id"], "mc1_21_lunar_fabric");
    assert_eq!(
        lunar["classes"][0]["runtime_name"],
        "net/minecraft/class_310"
    );
    assert_eq!(lunar["methods"][0]["runtime_name"], "method_1551");
    assert_eq!(lunar["methods"][0]["static"], true);
    assert_eq!(d["profiles"][1]["methods"][1]["runtime_name"], "e");
    assert!(
        dir.path()
            .join("data")
            .join("tables.provenance.json")
            .exists()
    );
    // Same inputs, same bytes.
    let again = dir.path().join("again.json");
    let o2 = run(
        dir.path(),
        &[
            "mc-mappings",
            "--catalog",
            &fixture("catalog.json"),
            "--data-version",
            "7",
            "-o",
            again.to_str().unwrap(),
            &fixture("vanilla-1.21.json"),
            &fixture("lunar-fabric-1.21.json"),
        ],
    );
    assert!(o2.status.success());
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&again).unwrap());
}

#[test]
fn missing_catalog_symbol_fails_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = dir.path().join("catalog.json");
    let text = std::fs::read_to_string(fixture("catalog.json"))
        .unwrap()
        .replace("getHealth", "getMaxHealth");
    std::fs::write(&catalog, text).unwrap();
    let o = run(
        dir.path(),
        &[
            "mc-mappings",
            "--catalog",
            catalog.to_str().unwrap(),
            "--data-version",
            "1",
            "-o",
            dir.path().join("x.json").to_str().unwrap(),
            &fixture("vanilla-1.21.json"),
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("minecraft.local_player.get_health") && err.contains("not found"),
        "{err}"
    );
}

#[test]
fn exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let o = |args: &[&str]| run(dir.path(), args).status.code();
    // Invalid usage and unknown configurations: 2.
    assert_eq!(o(&["generate"]), Some(2));
    assert_eq!(
        o(&[
            "--offline",
            "generate",
            "-v",
            "1.21",
            "-l",
            "bogus",
            "-o",
            out.to_str().unwrap()
        ]),
        Some(2)
    );
    assert_eq!(o(&["verify", "does-not-exist.json"]), Some(2));
    // Offline cache miss: 3.
    assert_eq!(o(&["--offline", "versions"]), Some(3));
    assert_eq!(
        o(&[
            "--offline",
            "generate",
            "-v",
            "1.21",
            "-o",
            out.to_str().unwrap()
        ]),
        Some(3)
    );
    // Nothing is written outside -o and the cache.
    assert!(!out.exists());
    assert_eq!(o(&["cache", "info"]), Some(0));
}

#[test]
fn scans_an_official_launcher_tree() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    let profile = |id: &str, json: serde_json::Value| {
        let d = mc.join("versions").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{id}.json")), json.to_string()).unwrap();
    };
    profile(
        "1.21.11",
        serde_json::json!({"id": "1.21.11", "type": "release"}),
    );
    profile(
        "26.4-snapshot-8",
        serde_json::json!({"id": "26.4-snapshot-8", "type": "snapshot"}),
    );
    profile(
        "fabric-loader-0.20.1-1.21.11",
        serde_json::json!({"id": "fabric-loader-0.20.1-1.21.11", "inheritsFrom": "1.21.11",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.20.1"}, {"name": "net.fabricmc:intermediary:1.21.11"}]}),
    );
    profile(
        "1.21.11-forge-61.2.4",
        serde_json::json!({"id": "1.21.11-forge-61.2.4", "inheritsFrom": "1.21.11",
            "libraries": [{"name": "net.minecraftforge:forge:1.21.11-61.2.4"}]}),
    );
    let o = run(
        dir.path(),
        &[
            "--json",
            "scan",
            "--minecraft-dir",
            mc.to_str().unwrap(),
            "--lunar-dir",
            dir.path().join("none").to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let found: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let rows: Vec<String> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            format!(
                "{} {} {}",
                c["minecraft_version"], c["loader"], c["loader_version"]
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            r#""1.21.11" "fabric" "0.20.1""#,
            r#""1.21.11" "forge" "1.21.11-61.2.4""#,
            r#""1.21.11" "vanilla" null"#
        ]
    );
}

#[test]
fn converts_a_generated_file_to_tiny_v2() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("vanilla.tiny");
    let o = run(
        dir.path(),
        &[
            "tiny",
            &fixture("vanilla-1.21.json"),
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("tiny\t2\t0\truntime\tmojang\tintermediary\n\tescaped-names\nc\ta\tnet/minecraft/client/Minecraft\tnet/minecraft/class_310\n"), "{text}");
    assert!(
        text.contains("\tm\t()La;\td\tgetInstance\tmethod_1551\n"),
        "{text}"
    );
    assert_eq!(
        run(dir.path(), &["tiny", "missing.json", "-o", "x.tiny"])
            .status
            .code(),
        Some(2)
    );
}
