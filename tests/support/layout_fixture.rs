//! Synthetic, isolated self-host source facts for the concrete cutover tests.
//! None of these graph/approval records are real repository lifecycle evidence.

use mine::domain::adr::{AdrEntry, AdrRegistry, AdrStatus, sha256_hex};
use mine::domain::config::MineConfig;
use mine::domain::graph::{CommitId, PlanNode, PlanWorkspace, RepoPath};
use mine::infrastructure::toml_store::render_markdown;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub(crate) const ADR: &str = "docs/design/decisions/0023-mine-local-governance-layout.md";
pub(crate) const FROZEN: &str = "docs/design/decisions/9001-frozen-crlf.md";
pub(crate) const FROZEN_BYTES: &[u8] =
    b"# Frozen legacy decision\r\n\r\nOriginal CRLF approval bytes.\r\n";
// mine-release-allow-plan-reference: exact synthetic source authorization for the concrete self-host consumer
const PLAN: &str = "plan24";
// mine-release-allow-plan-reference: exact synthetic source authorization for the concrete self-host consumer
const PLAN_BODY: &str = "docs/plan/plan24-unified-v0-2-0-delivery.md";
const CODE: &[&str] = &[
    "src/application/release_service/curation/layout_cutover.rs",
    "src/application/release_service/curation/layout_metadata.rs",
    "src/application/release_service/curation/layout_staging.rs",
    "scripts/stage-layout-cutover.rs",
];

pub(crate) fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let target = root.join(relative);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, bytes).unwrap();
}

pub(crate) fn git(root: &Path, arguments: &[&str]) -> Output {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .env("HOME", root.join(".git/fixture-home"))
        .env("USERPROFILE", root.join(".git/fixture-home"))
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {output:?}");
    output
}

fn git_text(root: &Path, arguments: &[&str]) -> String {
    String::from_utf8(git(root, arguments).stdout)
        .unwrap()
        .trim()
        .to_string()
}

fn entry(id: &str, path: &str, bytes: &[u8]) -> AdrEntry {
    AdrEntry {
        id: id.into(), path: path.into(), title: format!("Synthetic {id}"),
        status: AdrStatus::APPROVED, planning_blocking: true,
        planning_scopes: vec!["docs/design".into(), "src/fixture-interface".into()],
        content_sha256: sha256_hex(bytes), proposed_at: "2026-10-19T00:00:00Z".into(),
        approved_by: "synthetic-fixture-owner".into(), approved_at: "2026-10-19T00:00:00Z".into(),
        approval_note: "Synthetic fixture, never real owner approval; historic docs/design text stays unchanged".into(),
        supersedes: vec![], superseded_by: String::new(), rejection_reason: String::new(),
        updated_at: "2026-10-19T00:00:00Z".into(),
    }
}

pub(crate) struct LayoutFixture {
    pub directory: tempfile::TempDir,
    pub config: MineConfig,
    pub registry: AdrRegistry,
    pub source: CommitId,
    pub baseline: CommitId,
    pub implementation: CommitId,
}

impl LayoutFixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        git(root, &["init", "--initial-branch=master"]);
        for (name, value) in [
            ("user.name", "Synthetic layout fixture"),
            ("user.email", "layout@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "true"),
        ] {
            git(root, &["config", name, value]);
        }
        write(root, "README.md", b"# Unchanged user product\n");
        write(root, "docs/user.md", b"# Unchanged user documentation\r\n");
        // Deliberately byte-preserving legacy baseline. Separate negative
        // controls exercise missing/conflicting attributes, never force pass.
        write(
            root,
            ".gitattributes",
            b"* -text -filter -ident -working-tree-encoding\n",
        );
        write(
            root,
            ".gitignore",
            b".mine/runtime/\n.mine/build/\n.mine/locks/\n.mine/worktrees/\n",
        );
        git(
            root,
            &[
                "add",
                "--",
                "README.md",
                "docs/user.md",
                ".gitattributes",
                ".gitignore",
            ],
        );
        git(root, &["commit", "-m", "fixture: stable user baseline"]);
        let baseline = CommitId::new(git_text(root, &["rev-parse", "HEAD"])).unwrap();
        git(root, &["switch", "-c", "dev"]);
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        use mine::domain::config::*;
        let config = MineConfig {
            schema_version: 1,
            repository_id: "9c672a03-bab3-566e-9588-39cb5e6445c3".into(),
            mine_code_version: "0.2.0".into(),
            branches: BranchesConfig {
                stable: "master".into(),
                integration: "dev".into(),
            },
            design: DesignConfig {
                root: "docs/design/index.md".into(),
                marker: "docs/design/.mine-design.toml".into(),
                language: "en".into(),
                index_soft_limit_lines: 250,
                leaf_soft_limit_lines: 400,
            },
            plan: PlanConfig {
                root: "docs/plan".into(),
                ephemeral: true,
                purge_before_stable_release: true,
                worktrees_root: ".mine/worktrees".into(),
                shared_environment_root: ".".into(),
            },
            graph: GraphConfig {
                source: "docs/plan/execution-graph.toml".into(),
                rendered: "docs/plan/execution-graph.md".into(),
                lock_timeout_ms: 5000,
            },
            adr: AdrConfig {
                registry: "docs/design/decisions/adr-registry.toml".into(),
                rendered_index: "docs/design/decisions/index.md".into(),
                lock_timeout_ms: 5000,
            },
        };
        let raw = format!(
            "# Keep this foreign formatting/comment\n{}",
            config.to_toml()
        )
        .replace('\n', "\r\n");
        write(root, ".mine/config.toml", raw.as_bytes());
        write(
            root,
            ".mine/.gitignore",
            b"# Existing local rules survive\r\n/runtime/\r\n/build/\r\n/locks/\r\n/worktrees/\r\n",
        );
        let marker = mine::domain::design_marker::DesignMarker::new(
            config.repository_id.clone(),
            "2026-10-19T00:00:00Z".into(),
        );
        write(root, &config.design.marker, marker.to_toml().as_bytes());
        write(root, &config.design.root, b"# Owned legacy Design\n");
        write(
            root,
            "docs/design/operations/verification-receipts.md",
            b"# Fixture verification policy\n",
        );
        write(root, "docs/design/diagram.png", b"\x89PNG\r\n\0\xff");
        let approved_body =
            fs::read(project.join("tests/fixtures/layout/approved-layout-adr.md")).unwrap();
        write(root, ADR, &approved_body);
        write(root, FROZEN, FROZEN_BYTES);
        let mut registry = AdrRegistry::empty();
        registry.revision = 1;
        registry
            .entries
            .push(entry("ADR-0023", ADR, &approved_body));
        registry
            .entries
            .push(entry("ADR-9001", FROZEN, FROZEN_BYTES));
        let mut old = entry(
            "ADR-9002",
            "docs/design/decisions/9002-superseded.md",
            b"# Superseded\r\n",
        );
        old.status = AdrStatus::SUPERSEDED;
        old.superseded_by = "ADR-9003".into();
        let mut replacement = entry(
            "ADR-9003",
            "docs/design/decisions/9003-replacement.md",
            b"# Replacement\n",
        );
        replacement.supersedes.push("ADR-9002".into());
        write(root, &old.path, b"# Superseded\r\n");
        write(root, &replacement.path, b"# Replacement\n");
        registry.entries.extend([old, replacement]);
        registry.validate_structure().unwrap();
        let registry_text = format!(
            "# Registry trivia must survive\n{}",
            toml::to_string(&registry).unwrap()
        )
        .replace('\n', "\r\n");
        write(root, &config.adr.registry, registry_text.as_bytes());
        write(
            root,
            &config.adr.rendered_index,
            mine::infrastructure::adr_store::render_index(&registry, "docs/design/decisions")
                .unwrap()
                .as_bytes(),
        );
        write(
            root,
            ".mine/verification-policy.json",
            br#"{
  "schema_version": 1,
  "design_references": ["docs/design/operations/verification-receipts.md"],
  "checks": [{
    "name": "fixture", "command": {"argv": ["git", "rev-parse", "--verify", "HEAD^{tree}"], "cwd": {"kind": "root"}},
    "inputs": {"kind": "full"}, "tools": ["git"], "dependency_locks": [],
    "environment": [], "features": [], "warnings_disqualify": true,
    "independent_required": false, "native_platform": null,
    "retention_bytes": 1048576, "retention_files": 8
  }]
}
"#,
        );
        write(root, "AGENTS.md", b"# Foreign prefix docs/design remains\n<!-- mine-managed-agents -->\n- Design root: `docs/design/index.md`\n- Plans: `docs/plan/`\n");
        for name in CODE {
            write(root, name, &fs::read(project.join(name)).unwrap());
        }
        write(root, PLAN_BODY, b"# Synthetic governed cutover fixture\n\nNo real Plan lifecycle evidence or source Plan file is imported.\n");
        let implementation_report =
            mine::domain::plan_id::report_path(PLAN_BODY, "implementation").unwrap();
        let review_report = mine::domain::plan_id::report_path(PLAN_BODY, "review").unwrap();
        write(
            root,
            &implementation_report,
            b"Synthetic fixture implementation, not real Plan status.\n",
        );
        let mut add = vec![
            "add",
            "--",
            ".mine/config.toml",
            ".mine/.gitignore",
            ".mine/verification-policy.json",
            "docs/design",
            PLAN_BODY,
            &implementation_report,
            "AGENTS.md",
        ];
        add.extend_from_slice(CODE);
        git(root, &add);
        git(
            root,
            &[
                "commit",
                "-m",
                "fixture: submitted implementation and immutable inputs",
            ],
        );
        let implementation = CommitId::new(git_text(root, &["rev-parse", "HEAD"])).unwrap();
        write(root, &review_report, format!("Verdict: ACCEPTED\n\nSynthetic independent fixture review by fixture-reviewer, not the executor.\nReviewed tip: {}\n", implementation.as_str()).as_bytes());
        let node: PlanNode = serde_json::from_value(serde_json::json!({
            "id": PLAN, "path": PLAN_BODY, "title": "Synthetic self-host cutover",
            // mine-release-allow-plan-reference: synthetic registered branch identity for concrete layout authorization
            "branch": "plan/plan24-unified-v0-2-0-delivery", "branch_anchor": baseline.as_str(),
            "status": "ACCEPTED", "design_references": [ADR, "docs/design/index.md"],
            "exclusive_write_paths": ["src/application", "src/domain", "tests", "scripts"],
            "implementation_report": implementation_report, "review_report": review_report,
            "implementation_commits": [implementation.as_str()], "owner": "fixture-executor",
            "run_id": "synthetic-run", "started_at": "2026-10-19T00:00:00Z", "updated_at": "2026-10-19T00:00:00Z",
            "planning_scope": "docs/design", "readiness_checked_at": "2026-10-19T00:00:00Z", "adr_registry_revision": 1,
            // mine-release-allow-plan-reference: synthetic lineage locator for concrete layout authorization
            "lineage_id": PLAN, "worktree_path": ".mine/worktrees/plan24"
        })).unwrap();
        let graph = PlanWorkspace {
            schema_version: 4,
            revision: 3,
            project_id: "synthetic-layout-fixture".into(),
            workspace_id: uuid::Uuid::new_v4().to_string(),
            stable_branch: "master".into(),
            integration_branch: "dev".into(),
            stable_baseline_commit: baseline.as_str().into(),
            design_root: config.design.root.clone(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![node],
        };
        mine::domain::validation::validate(&graph).unwrap();
        write(
            root,
            &config.graph.source,
            toml::to_string(&graph).unwrap().as_bytes(),
        );
        write(
            root,
            &config.graph.rendered,
            render_markdown(&graph).unwrap().as_bytes(),
        );
        git(root, &["add", "--", "docs/plan"]);
        git(
            root,
            &[
                "commit",
                "-m",
                "fixture: independently accepted terminal source",
            ],
        );
        let source = CommitId::new(git_text(root, &["rev-parse", "HEAD"])).unwrap();
        Self {
            directory,
            config,
            registry,
            source,
            baseline,
            implementation,
        }
    }

    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn checkpoint(&mut self, paths: &[&str]) {
        let mut arguments = vec!["add", "--"];
        arguments.extend_from_slice(paths);
        git(self.root(), &arguments);
        git(
            self.root(),
            &["commit", "-m", "fixture: changed source fact"],
        );
        self.source = CommitId::new(git_text(self.root(), &["rev-parse", "HEAD"])).unwrap();
    }
    pub fn checkout(&self, name: &str) -> (RepoPath, PathBuf) {
        let relative = RepoPath::new(format!(".mine/runtime/layout-candidates/{name}")).unwrap();
        let absolute = self.root().join(relative.as_str());
        git(
            self.root(),
            &[
                "worktree",
                "add",
                "--detach",
                absolute.to_str().unwrap(),
                self.source.as_str(),
            ],
        );
        (relative, absolute)
    }
    pub fn commit_candidate(&self, checkout: &Path) -> CommitId {
        git(
            checkout,
            &[
                "add",
                "--",
                ".mine/config.toml",
                ".mine/.gitignore",
                ".mine/.gitattributes",
                ".mine/design",
                ".mine/verification-policy.json",
                "AGENTS.md",
            ],
        );
        git(checkout, &["add", "-u", "--", "docs/design", "docs/plan"]);
        let tree = git_text(checkout, &["write-tree"]);
        let candidate = CommitId::new(git_text(
            checkout,
            &[
                "commit-tree",
                &tree,
                "-p",
                self.baseline.as_str(),
                "-m",
                "fixture: curated candidate without Plan history",
            ],
        ))
        .unwrap();
        git(checkout, &["switch", "--detach", candidate.as_str()]);
        candidate
    }
}
