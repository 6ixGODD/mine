#![forbid(unsafe_code)]
#![cfg(windows)]
//! Native filesystem interruption after verified copies, without a lib harness.
//! Original deterministic private IO-seam unit coverage is retained unchanged.
//!
//! Rust's standard Windows share-mode contract permits reads while denying
//! write/delete/rename until the handle closes:
//! https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode
#[path = "support/layout_fixture.rs"]
#[allow(dead_code)]
mod fixture;

use fixture::{LayoutFixture, git};
use mine::application::release_service::curation::stage_layout_cutover;
use std::{fs, os::windows::fs::OpenOptionsExt, path::Path};

fn bytes(root: &Path, path: &str) -> Vec<u8> {
    fs::read(root.join(path)).unwrap()
}

#[test]
fn real_copy_failure_retains_source_old_checkout_and_pending_evidence() {
    let fixture = LayoutFixture::new();
    let root = fixture.root();
    let source_index = bytes(root, ".git/index");
    let source_graph = bytes(root, &fixture.config.graph.source);
    let source_approval = bytes(root, fixture::FROZEN);
    let (relative, target) = fixture.checkout("interrupted-native-copy");
    let target_index_path = {
        let observed = git(
            &target,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        );
        String::from_utf8(observed.stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    let target_index = fs::read(&target_index_path).unwrap();
    let old_agents = bytes(&target, "AGENTS.md");
    let old_design = bytes(&target, "docs/design/index.md");
    // AGENTS sorts after all .mine destinations. Git/preflight can still read
    // it, but the real atomic rename cannot replace it while this handle lives.
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ, deliberately no WRITE or DELETE sharing.
        .open(target.join("AGENTS.md"))
        .unwrap();
    let error = stage_layout_cutover(root, &fixture.source, &relative).unwrap_err();
    assert!(
        error.to_string().contains("layout staging pending"),
        "{error}"
    );
    drop(held);

    assert_eq!(bytes(root, ".git/index"), source_index);
    assert_eq!(bytes(root, &fixture.config.graph.source), source_graph);
    assert_eq!(bytes(root, fixture::FROZEN), source_approval);
    assert_eq!(fs::read(&target_index_path).unwrap(), target_index);
    assert_eq!(bytes(&target, "AGENTS.md"), old_agents);
    assert_eq!(bytes(&target, "docs/design/index.md"), old_design);
    assert_eq!(bytes(&target, fixture::FROZEN), fixture::FROZEN_BYTES);
    assert_eq!(
        bytes(&target, ".mine/design/decisions/9001-frozen-crlf.md"),
        fixture::FROZEN_BYTES
    );
    assert_eq!(
        String::from_utf8(git(root, &["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim(),
        fixture.source.as_str()
    );
    assert_eq!(
        String::from_utf8(git(&target, &["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim(),
        fixture.source.as_str()
    );
    let manifests = fs::read_dir(root.join(".mine/runtime/layout-staging"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(manifests.len(), 1);
    let retained = fs::read(&manifests[0]).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&retained).unwrap();
    assert_eq!(manifest["pending"], true);
    assert!(manifest["writes_verified"].as_u64().unwrap() > 2);
    assert_eq!(manifest["deletions_verified"], 0);

    // Releasing the OS handle does not authorize a fresh execution over the
    // partial checkout or allow the evidence record to be overwritten.
    assert!(stage_layout_cutover(root, &fixture.source, &relative).is_err());
    assert_eq!(fs::read(&manifests[0]).unwrap(), retained);
    assert_eq!(bytes(root, ".git/index"), source_index);
    assert_eq!(bytes(root, fixture::FROZEN), fixture::FROZEN_BYTES);
}
