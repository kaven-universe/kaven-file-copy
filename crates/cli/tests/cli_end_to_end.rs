use std::{fs, path::Path, process::Command};

use serde_json::json;
use uuid::Uuid;

fn write_rule(root: &Path) -> std::path::PathBuf {
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("hello.txt"), b"hello from CLI").unwrap();
    fs::write(source.join("ignored.tmp"), b"ignore me").unwrap();
    let rule = json!({
        "schemaVersion": 1,
        "id": Uuid::new_v4(),
        "name": "CLI end-to-end",
        "enabled": true,
        "source": { "type": "local", "path": source },
        "destination": { "type": "local", "path": destination },
        "filters": {
            "exclude": [{ "kind": "glob", "value": "**/*.tmp" }],
            "includeHidden": true
        },
        "copy": {
            "mode": "incremental",
            "compare": "size-and-mtime",
            "overwrite": "if-changed",
            "symlinks": "skip",
            "preserveMtime": true,
            "maxConcurrency": 2,
            "retryCount": 1
        },
        "trigger": { "type": "manual" }
    });
    let path = root.join("rule.kfc.json");
    fs::write(&path, serde_json::to_vec_pretty(&rule).unwrap()).unwrap();
    path
}

#[test]
fn validate_preview_and_run_exported_rule() {
    let root = std::env::temp_dir().join(format!("kfc-cli-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let rule = write_rule(&root);
    let binary = env!("CARGO_BIN_EXE_kfc");

    let validate = Command::new(binary)
        .args(["rule", "validate"])
        .arg(&rule)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );

    let preview = Command::new(binary)
        .arg("run")
        .arg(&rule)
        .args(["--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(plan["stats"]["filesToCopy"], 1);

    let run = Command::new(binary)
        .arg("run")
        .arg(&rule)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read(root.join("destination/hello.txt")).unwrap(),
        b"hello from CLI"
    );
    assert!(!root.join("destination/ignored.tmp").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn versioned_rule_copies_newest_version_to_fixed_name() {
    let root = std::env::temp_dir().join(format!("kfc-cli-versioned-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("EDM.DSP.Core_1.0.0.dll"), b"v1").unwrap();
    fs::write(source.join("EDM.DSP.Core_2.0.0.dll"), b"v2").unwrap();
    let rule = json!({
        "schemaVersion": 1,
        "id": Uuid::new_v4(),
        "name": "CLI versioned",
        "enabled": true,
        "source": { "type": "local", "path": source },
        "destination": { "type": "local", "path": destination },
        "filters": { "exclude": [], "includeHidden": true },
        "copy": {
            "mode": "versioned",
            "compare": "size-and-mtime",
            "overwrite": "if-changed",
            "symlinks": "skip",
            "preserveMtime": true,
            "maxConcurrency": 2,
            "retryCount": 1,
            "versioned": {
                "mappings": [{
                    "sourcePattern": "EDM.DSP.Core*.dll",
                    "targetName": "EDM.DSP.Core.dll",
                    "selection": "latest"
                }]
            }
        },
        "trigger": { "type": "manual" }
    });
    let path = root.join("rule.kfc.json");
    fs::write(&path, serde_json::to_vec_pretty(&rule).unwrap()).unwrap();
    let binary = env!("CARGO_BIN_EXE_kfc");

    let validate = Command::new(binary)
        .args(["rule", "validate"])
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );

    let run = Command::new(binary)
        .arg("run")
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read(root.join("destination/EDM.DSP.Core.dll")).unwrap(),
        b"v2"
    );
    assert!(!root.join("destination/EDM.DSP.Core_2.0.0.dll").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_rule_uses_documented_exit_code() {
    let root = std::env::temp_dir().join(format!("kfc-cli-invalid-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("invalid.json");
    fs::write(&path, b"{ not json }").unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_kfc"))
        .args(["rule", "validate"])
        .arg(&path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_rule_uses_execution_error_exit_code() {
    let missing = std::env::temp_dir().join(format!("kfc-missing-{}.json", Uuid::new_v4()));
    let status = Command::new(env!("CARGO_BIN_EXE_kfc"))
        .args(["rule", "validate"])
        .arg(missing)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(3));
}
