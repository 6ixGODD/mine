#![forbid(unsafe_code)]
//! Actual final metadata reader and native version, with isolated drift cases.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const METADATA: &[&str] = &[
    ".mine/config.toml",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    ".claude-plugin/plugin.json",
    ".claude-plugin/marketplace.json",
    "plugins/mine/.claude-plugin/plugin.json",
    "plugins/mine/.codex-plugin/plugin.json",
    "distribution/platforms.json",
];
fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn checker(root: &Path) -> Output {
    Command::new("python")
        .arg(project().join("scripts/check-release-version.py"))
        .arg("--root")
        .arg(root)
        .output()
        .unwrap()
}
fn copy_metadata() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().unwrap();
    let mut files = METADATA
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    for skill in [
        "mine-arch",
        "mine-plan-create",
        "mine-plan-exec",
        "mine-plan-review",
        "mine-sync",
    ] {
        for prefix in ["skills", "plugins/mine/skills"] {
            files.push(format!("{prefix}/{skill}/SKILL.md"));
        }
    }
    for relative in files {
        let target = fixture.path().join(&relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(project().join(relative), target).unwrap();
    }
    fixture
}
fn mutate_json(root: &Path, relative: &str, change: impl FnOnce(&mut Value)) {
    let path = root.join(relative);
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    change(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn final_product_binary_and_all_managed_versions_agree() {
    let result = checker(&project());
    assert!(result.status.success(), "{result:?}");
    let observed: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(observed["ok"], true);
    assert_eq!(observed["version"], "0.2.0");
    assert!(
        observed["managed_versions"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == "0.2.0")
    );
    assert_eq!(observed["upstream"], "https://github.com/6ixGODD/mine");
    assert_eq!(observed["macos"], "DEFERRED");
    let native = Command::new(env!("CARGO_BIN_EXE_mine"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(native.status.success());
    assert_eq!(
        String::from_utf8(native.stdout).unwrap().trim(),
        "mine 0.2.0"
    );
}

#[test]
fn version_and_upstream_drift_are_refused_without_rewriting_inputs() {
    for relative in ["package.json", "plugins/mine/.codex-plugin/plugin.json"] {
        let fixture = copy_metadata();
        let before = checker(fixture.path());
        assert!(before.status.success(), "{before:?}");
        mutate_json(fixture.path(), relative, |value| {
            value["version"] = "9.9.9".into()
        });
        let bytes = fs::read(fixture.path().join(relative)).unwrap();
        let refused = checker(fixture.path());
        assert_eq!(refused.status.code(), Some(1));
        let error: Value = serde_json::from_slice(&refused.stderr).unwrap();
        assert_eq!(error["ok"], false);
        assert!(error["error"].as_str().unwrap().contains("version drift"));
        assert_eq!(fs::read(fixture.path().join(relative)).unwrap(), bytes);
    }
    let fixture = copy_metadata();
    let relative = "plugins/mine/.claude-plugin/plugin.json";
    mutate_json(fixture.path(), relative, |value| {
        value["repository"] = "https://example.invalid/wrong".into()
    });
    let refused = checker(fixture.path());
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        serde_json::from_slice::<Value>(&refused.stderr).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("upstream drift")
    );
}

#[test]
fn owner_deferred_and_missing_platforms_cannot_silently_enter_the_final_matrix() {
    for change in ["missing-linux", "extra-macos"] {
        let fixture = copy_metadata();
        mutate_json(fixture.path(), "distribution/platforms.json", |value| {
            let rows = value["platforms"].as_array_mut().unwrap();
            if change == "missing-linux" {
                rows.retain(|row| row["os"] != "linux");
            } else {
                rows.push(serde_json::json!({"os":"macos","arch":"aarch64","target":"aarch64-apple-darwin"}));
            }
        });
        let result = checker(fixture.path());
        assert_eq!(result.status.code(), Some(1));
        assert!(
            serde_json::from_slice::<Value>(&result.stderr).unwrap()["error"]
                .as_str()
                .unwrap()
                .contains("owner-deferred")
        );
    }
}
