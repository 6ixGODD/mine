#![forbid(unsafe_code)]

//! Actual candidate CLI boundary, always in an isolated managed repository.
#[path = "support/config.rs"]
mod fixture_config;
use mine::domain::{design_marker::DesignMarker, graph::*, ownership::*};
use mine::infrastructure::{graph_checkpoint::digest, toml_store::render_markdown};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

// mine-release-allow-plan-reference: real-entry semantic registration fixture
const PLAN: &str = "plan81";
// mine-release-allow-plan-reference: real-entry semantic registration fixture
const PLAN_PATH: &str = ".mine/plan/plan81-fixture.md";
const POLICY_PATH: &str = ".mine/design/governance/ownership-policy.json";
const ADR_PATH: &str = ".mine/design/decisions/0042-ownership-policy.md";
const INPUT_PATH: &str = ".mine/plan/semantic-registration.json";
const CLARIFY_PATH: &str = ".mine/runtime/clarification-input.json";
// mine-release-allow-plan-reference: exact registered fixture output namespace
const VERIFICATION_INPUT: &str = ".mine/build/plan81/verification/request.json";
const CLOSURE_INPUT: &str = ".mine/runtime/closure-input.json";

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn cli(root: &Path, args: &[&str]) -> (i32, Value) {
    cli_observed(root, args, None)
}
fn cli_observed(root: &Path, args: &[&str], trace: Option<&Path>) -> (i32, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mine"));
    command
        .current_dir(root)
        .args(["--format", "json"])
        .args(args);
    if let Some(trace) = trace {
        command.env("GIT_TRACE2_EVENT", trace);
    }
    let out = command.output().unwrap();
    let bytes = if out.status.success() {
        &out.stdout
    } else {
        &out.stderr
    };
    let value = serde_json::from_slice(bytes)
        .unwrap_or_else(|e| panic!("CLI {args:?}: {e}: {}", String::from_utf8_lossy(bytes)));
    (out.status.code().unwrap(), value)
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let (code, value) = cli(root, args);
    assert_eq!(code, 0, "CLI {args:?}: {value}");
    value
}
fn path(value: &str) -> RepoPath {
    RepoPath::new(value).unwrap()
}

fn journal_snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let directory = root.join(".mine/runtime/operations");
    if !directory.exists() {
        return Default::default();
    }
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (entry.file_name().into(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

fn directory_alias(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        // cmd's built-in parser treats forward slashes as switches, unlike
        // Rust filesystem APIs. Fixture TMP may legitimately use D:/... .
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(target.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction creation: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
}

struct Fixture {
    dir: tempfile::TempDir,
    policy: Vec<u8>,
    blocked_successor: bool,
}
impl Fixture {
    fn new(binding_override: Option<&str>, approve: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Native Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(root, &["config", key, value]);
        }
        fs::write(
            root.join(".gitignore"),
            ".mine/runtime/\n.mine/locks/\n.mine/worktrees/\n.mine/build/\n",
        )
        .unwrap();
        git(root, &["add", ".gitignore"]);
        git(root, &["commit", "-m", "fixture baseline"]);
        let baseline = git(root, &["rev-parse", "HEAD"]);
        git(root, &["branch", "master", &baseline]);
        fs::create_dir_all(root.join(".mine")).unwrap();
        fs::create_dir_all(root.join(".mine/design/governance")).unwrap();
        fs::create_dir_all(root.join(".mine/design/decisions")).unwrap();
        fs::create_dir_all(root.join(".mine/plan/reports")).unwrap();
        mine::infrastructure::governance_git::establish(root).unwrap();
        git(
            root,
            &["add", "--", ".mine/.gitattributes", ".mine/.gitignore"],
        );
        git(root, &["commit", "-m", "canonical managed byte policy"]);
        let config = fixture_config::canonical();
        fs::write(root.join(".mine/config.toml"), config.to_toml()).unwrap();
        fs::write(root.join(".mine/design/index.md"), "# Fixture Design\n").unwrap();
        fs::write(
            root.join(".mine/design/.mine-design.toml"),
            DesignMarker::new(config.repository_id.clone(), "2026-10-14T00:00:00Z".into())
                .to_toml(),
        )
        .unwrap();
        fs::write(root.join(PLAN_PATH), "# Immutable fixture Plan\n").unwrap();
        let policy = serde_json::to_vec_pretty(&OwnershipPolicyDocument {
            schema_version: 1,
            clarification_actors: vec!["owner".into()],
            mappings: vec![ResponsibilityMapping {
                responsibility: "fixture".into(),
                namespaces: vec![NamespaceClaim {
                    subtree: Namespace::new("app.fixture").unwrap(),
                    excluded: vec![],
                }],
                contracts: vec!["fixture".into()],
                physical_prefixes: vec![path("src")],
                shared_entries: vec![],
                generator_names: vec![],
            }],
            partitions: vec![],
            shared_declarations: vec![],
            generators: vec![],
        })
        .unwrap();
        fs::write(root.join(POLICY_PATH), &policy).unwrap();
        let binding =
            json!({"schema_version":1,"policy_path":POLICY_PATH,"policy_sha256":digest(&policy)})
                .to_string();
        let body = binding_override.map(str::to_string).unwrap_or_else(|| {
            format!("# ADR-0042: Fixture policy\n\n```mine-ownership-policy\n{binding}\n```\n")
        });
        fs::write(root.join(ADR_PATH), body).unwrap();
        ok(
            root,
            &[
                "adr",
                "add",
                "--id",
                "ADR-0042",
                "--path",
                ADR_PATH,
                "--title",
                "Fixture policy",
                "--scope",
                ".mine/design",
            ],
        );
        ok(root, &["adr", "propose", "--id", "ADR-0042"]);
        if approve {
            ok(
                root,
                &[
                    "adr",
                    "approve",
                    "--id",
                    "ADR-0042",
                    "--by",
                    "owner",
                    "--note",
                    "Explicit isolated fixture approval",
                ],
            );
        }
        let workspace = PlanWorkspace {
            schema_version: 4,
            revision: 0,
            project_id: "human-project-label".into(),
            workspace_id: uuid::Uuid::new_v4().to_string(),
            stable_branch: "master".into(),
            integration_branch: "dev".into(),
            stable_baseline_commit: baseline,
            design_root: config.design.root,
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        fs::write(
            root.join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        let identity = PlanIdentity::new(PlanIdentityFields {
            repository_id: config.repository_id,
            workspace_id: workspace.workspace_id,
            plan_id: PLAN.into(),
            plan_path: path(PLAN_PATH),
            body_sha256: Some(digest(&fs::read(root.join(PLAN_PATH)).unwrap())),
            lineage_id: PLAN.into(),
            branch: format!("plan/{PLAN}-fixture"),
        })
        .unwrap();
        let ownership = OwnershipContract::new(OwnershipFields {
            schema_version: 1,
            plan: identity,
            policy: PolicyIdentity::new(PolicyIdentityFields {
                design_references: vec![path(ADR_PATH)],
                sha256: digest(&policy),
            })
            .unwrap(),
            responsibility: "fixture".into(),
            namespace_claims: vec![NamespaceClaim {
                subtree: Namespace::new("app.fixture").unwrap(),
                excluded: vec![],
            }],
            owned_contracts: vec!["fixture".into()],
            excluded_responsibilities: vec![],
            excluded_contracts: vec![],
            physical_mapping: vec![path("src/fixture")],
            shared_declarations: vec![],
            generators: vec![],
        })
        .unwrap();
        fs::write(root.join(INPUT_PATH), serde_json::to_vec(&json!({"policy_path":POLICY_PATH,"approving_adr":"ADR-0042","ownership":ownership})).unwrap()).unwrap();
        git(
            root,
            &["add", ".mine/config.toml", ".mine/design", ".mine/plan"],
        );
        git(root, &["commit", "-m", "explicit managed fixture inputs"]);
        Self {
            dir,
            policy,
            blocked_successor: false,
        }
    }
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn register(&self, revision: &str) -> (i32, Value) {
        cli(
            self.root(),
            &[
                "plan",
                "add",
                "--actor",
                "fixture-registrar",
                "--id",
                PLAN,
                "--path",
                PLAN_PATH,
                "--title",
                "Fixture",
                "--design-ref",
                ".mine/design/index.md",
                "--write",
                "src/fixture",
                "--expected-revision",
                revision,
                "--ownership-input",
                INPUT_PATH,
            ],
        )
    }
    fn graph(&self) -> Vec<u8> {
        fs::read(self.root().join(".mine/plan/execution-graph.toml")).unwrap()
    }
    fn change_input(&self, mutate: impl FnOnce(&mut Value)) {
        let mut input: Value =
            serde_json::from_slice(&fs::read(self.root().join(INPUT_PATH)).unwrap()).unwrap();
        mutate(&mut input);
        fs::write(
            self.root().join(INPUT_PATH),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        git(self.root(), &["add", INPUT_PATH]);
        git(self.root(), &["commit", "-m", "adversarial fixture input"]);
    }

    fn clarify_input(&self, mutate: impl FnOnce(&mut Value)) {
        fs::create_dir_all(self.root().join(".mine/runtime")).unwrap();
        let registration: Value =
            serde_json::from_slice(&fs::read(self.root().join(INPUT_PATH)).unwrap()).unwrap();
        let mut input = json!({
            "policy_path":POLICY_PATH,"approving_adr":"ADR-0042",
            "plan":registration["ownership"]["plan"],"actor":"owner","reason":"bounded mapping closure",
            "kind":"mapping","expected_tail":null,"physical_mapping":["src/fixture","src/fixture2"],"generators":[]
        });
        mutate(&mut input);
        fs::write(
            self.root().join(CLARIFY_PATH),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
    }
    fn clarify(&self, revision: &str) -> (i32, Value) {
        cli(
            self.root(),
            &[
                "plan",
                "clarify",
                "--id",
                PLAN,
                "--input",
                CLARIFY_PATH,
                "--expected-revision",
                revision,
            ],
        )
    }
    fn seed_legacy_node(&self, known_body: bool) {
        let mut workspace: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&self.graph()).unwrap()).unwrap();
        workspace.schema_version = if known_body { 5 } else { 4 };
        workspace.plans.push(serde_json::from_value(json!({
            "id":PLAN,"path":PLAN_PATH,"title":"Legacy fixture","status":"READY",
            "branch":format!("plan/{PLAN}-fixture"),"lineage_id":PLAN,"worktree_path":format!(".mine/worktrees/{PLAN}"),
            "design_references":[".mine/design/index.md"],"exclusive_write_paths":["src/fixture"]
        })).unwrap());
        if known_body {
            workspace.plans[0].body_sha256 =
                Some(digest(&fs::read(self.root().join(PLAN_PATH)).unwrap()));
        }
        fs::write(
            self.root().join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            self.root().join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        git(
            self.root(),
            &[
                "add",
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(
            self.root(),
            &["commit", "-m", "explicit legacy authority fixture"],
        );
    }

    fn seed_held_owner(&self, scope: &str) {
        // mine-release-allow-plan-reference: independent held-owner runtime fixture
        let id = "plan82";
        let plan_path = format!(".mine/plan/{id}-held.md");
        fs::write(self.root().join(&plan_path), "# Held fixture Plan\n").unwrap();
        let mut workspace: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&self.graph()).unwrap()).unwrap();
        workspace.plans.push(serde_json::from_value(json!({
            "id":id,"path":plan_path,"title":"Held fixture","status":"IN_PROGRESS",
            "branch":format!("plan/{id}-held"),"lineage_id":id,"worktree_path":format!(".mine/worktrees/{id}"),
            "branch_anchor":git(self.root(), &["rev-parse", "HEAD"]),
            "design_references":[".mine/design/index.md"],"exclusive_write_paths":[scope],
            "owner":"other","run_id":"held-fixture","started_at":"2026-10-14T00:00:00Z"
        })).unwrap());
        fs::write(
            self.root().join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            self.root().join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        git(self.root(), &["add", ".mine/plan"]);
        git(
            self.root(),
            &["commit", "-m", "explicit held-owner fixture"],
        );
    }

    fn verification_policy(&self) -> Value {
        use mine::domain::verification_receipt::*;
        let policy = VerificationPolicy::new(PolicyFields {
            schema_version: 1,
            design_references: vec![path(".mine/design/index.md")],
            checks: vec![CheckPolicy {
                name: "probe".into(),
                command: CheckCommand::new(CommandFields {
                    argv: vec!["git".into(), "--version".into()],
                    cwd: CommandDirectory::Root,
                })
                .unwrap(),
                inputs: InputScope::Full,
                tools: vec!["git".into()],
                dependency_locks: vec![],
                environment: vec![],
                features: vec![],
                warnings_disqualify: true,
                independent_required: false,
                native_platform: None,
                retention_bytes: 4096,
                retention_files: 4,
            }],
        })
        .unwrap();
        fs::write(
            self.root().join(".mine/verification-policy.json"),
            serde_json::to_vec(&policy).unwrap(),
        )
        .unwrap();
        git(
            self.root(),
            &[
                "add",
                ".mine/verification-policy.json",
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(
            self.root(),
            &[
                "commit",
                "-m",
                "explicit native verification fixture policy",
            ],
        );
        json!({"toolchains":{"git":digest(git(self.root(), &["--version"]).as_bytes())}, "platform":std::env::consts::OS,
            "architecture":std::env::consts::ARCH,"features":[],"environment":[]})
    }

    fn verification(&self, verb: &str, input: &Value, receipt: Option<&str>) -> (i32, Value) {
        self.verification_from(self.root(), verb, input, receipt)
    }

    fn verification_from(
        &self,
        source: &Path,
        verb: &str,
        input: &Value,
        receipt: Option<&str>,
    ) -> (i32, Value) {
        fs::create_dir_all(self.root().join(VERIFICATION_INPUT).parent().unwrap()).unwrap();
        fs::write(
            self.root().join(VERIFICATION_INPUT),
            serde_json::to_vec(input).unwrap(),
        )
        .unwrap();
        let mut args = vec!["verification", verb, "--input", VERIFICATION_INPUT];
        if let Some(receipt) = receipt {
            args.extend(["--receipt", receipt]);
        }
        cli(source, &args)
    }

    fn implemented_subject(&self) -> std::path::PathBuf {
        assert_eq!(self.register("0").0, 0);
        let mut workspace: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&self.graph()).unwrap()).unwrap();
        // Explicit legacy-active fixture, not fabricated prepare/start evidence.
        if self.blocked_successor {
            let id = format!("{PLAN}-01");
            let path = format!(".mine/plan/{id}-successor.md");
            fs::write(
                self.root().join(&path),
                "# Explicit blocked successor fixture\n",
            )
            .unwrap();
            workspace.plans.push(serde_json::from_value(json!({
                "id":id,"path":path,"title":"Blocked successor fixture","status":"BLOCKED",
                "branch":format!("plan/{id}-successor"),"lineage_id":id,"worktree_path":format!(".mine/worktrees/{id}"),
                "hard_predecessors":[PLAN],"design_references":[".mine/design/index.md"],"exclusive_write_paths":["src/successor"]
            })).unwrap());
            git(self.root(), &["add", &path]);
        }
        workspace.revision = 2;
        let node = workspace.get_mut(PLAN).unwrap();
        node.status = mine::domain::status::PlanStatus::Implemented;
        node.owner = "executor".into();
        node.run_id = "fixture-run".into();
        node.branch_anchor = git(self.root(), &["rev-parse", "HEAD"]);
        fs::write(
            self.root().join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            self.root().join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        git(
            self.root(),
            &[
                "add",
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(
            self.root(),
            &["commit", "-m", "explicit implemented subject fixture"],
        );
        let target = self.root().join(format!(".mine/worktrees/{PLAN}"));
        git(
            self.root(),
            &[
                "worktree",
                "add",
                "-b",
                &format!("plan/{PLAN}-fixture"),
                target.to_str().unwrap(),
                "HEAD",
            ],
        );
        fs::create_dir_all(target.join("src/fixture")).unwrap();
        fs::write(
            target.join("src/fixture/implementation.txt"),
            "actual implementation fixture\n",
        )
        .unwrap();
        git(&target, &["add", "src/fixture/implementation.txt"]);
        git(
            &target,
            &["commit", "-m", "actual Plan branch implementation"],
        );
        workspace.revision = 3;
        workspace.get_mut(PLAN).unwrap().implementation_commits =
            vec![git(&target, &["rev-parse", "HEAD"])];
        fs::write(
            self.root().join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            self.root().join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        git(
            self.root(),
            &[
                "add",
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(
            self.root(),
            &["commit", "-m", "explicit fixture implementation evidence"],
        );
        target
    }

    fn closure_assertion(&self) -> Value {
        use mine::domain::ports::Clock;
        use mine::infrastructure::system::SystemClock;
        let registration: Value =
            serde_json::from_slice(&fs::read(self.root().join(INPUT_PATH)).unwrap()).unwrap();
        json!({"schema_version":1,"plan":registration["ownership"]["plan"],"writer_owner":"executor",
            "writer_run_id":"fixture-run","reviewer":"reviewer","independent_review":true,"writer_stopped":true,
            "stopped_at":SystemClock.now_utc_rfc3339(),"stop_basis":"Fixture controller completed implementation and handed off the checkout"})
    }

    fn closure_input(&self, target: &Path, assertion: &str, rejected: bool) -> Value {
        let report_path = format!(".mine/plan/reports/{PLAN}-fixture-review.md");
        let verdict = if rejected { "REJECTED" } else { "ACCEPTED" };
        let report = format!("# Independent fixture review\n\nVerdict: {verdict}\n\n{assertion}\n");
        let report_root = if rejected { self.root() } else { target };
        fs::create_dir_all(report_root.join(".mine/plan/reports")).unwrap();
        fs::write(report_root.join(&report_path), &report).unwrap();
        git(report_root, &["add", &report_path]);
        git(
            report_root,
            &[
                "commit",
                "--allow-empty",
                "-m",
                "pin independent closure report fixture",
            ],
        );
        let review = json!({"reviewed_tip":git(target, &["rev-parse", "HEAD"]),"report":{"path":report_path,"bytes":report.len(),"sha256":digest(report.as_bytes())}});
        let mut intent = json!({"kind":"acceptance","plan_id":PLAN,"review":review});
        if rejected {
            let cp = format!("{PLAN}-C01");
            let cp_path = format!(".mine/plan/{cp}-fixture.md");
            let body = b"# Real compensation fixture\n";
            fs::write(self.root().join(&cp_path), body).unwrap();
            git(self.root(), &["add", &cp_path]);
            git(
                self.root(),
                &["commit", "-m", "explicit compensation body before handoff"],
            );
            let mut identity = self.closure_assertion()["plan"].clone();
            identity["plan_id"] = json!(cp);
            identity["plan_path"] = json!(cp_path);
            identity["branch"] = json!(format!("plan/{cp}-fixture"));
            identity["body_sha256"] = json!(digest(body));
            intent["kind"] = json!("rejected_handoff");
            intent["compensation"] = identity;
        }
        let value = json!({"schema_version":1,"actor":"reviewer","intent":intent});
        fs::create_dir_all(self.root().join(".mine/runtime")).unwrap();
        fs::write(
            self.root().join(CLOSURE_INPUT),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        value
    }

    fn begin_closure(&self, revision: &str) -> (i32, Value) {
        cli(
            self.root(),
            &[
                "operation",
                "begin",
                "--expected-revision",
                revision,
                "--input",
                CLOSURE_INPUT,
            ],
        )
    }

    fn stage_closure(
        &self,
        operation: &Value,
        expected_stage: u64,
        verification: Value,
    ) -> (i32, Value) {
        self.stage_closure_observed(operation, expected_stage, verification, None)
    }

    fn stage_closure_observed(
        &self,
        operation: &Value,
        expected_stage: u64,
        verification: Value,
        trace: Option<&Path>,
    ) -> (i32, Value) {
        let request = json!({"schema_version":1,"expected_stage_revision":expected_stage,"operation":operation,"verification":verification});
        fs::write(
            self.root().join(CLOSURE_INPUT),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        cli_observed(
            self.root(),
            &[
                "operation",
                "stage",
                "--expected-revision",
                "3",
                "--input",
                CLOSURE_INPUT,
            ],
            trace,
        )
    }

    fn integrated_closure(&self, unknown: bool) -> (Value, Value) {
        let mut context = self.verification_policy();
        if unknown {
            let policy_path = self.root().join(".mine/verification-policy.json");
            let mut policy: Value =
                serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
            policy["checks"][0]["environment"] =
                json!([{"name":"external_data","secret_dependent":false,"observable":false}]);
            policy["checks"][0]["independent_required"] = json!(true);
            policy["checks"][0]["native_platform"] = json!(std::env::consts::OS);
            fs::write(policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
            git(self.root(), &["add", ".mine/verification-policy.json"]);
            git(
                self.root(),
                &["commit", "-m", "explicit unknown-input native gate fixture"],
            );
            context["environment"] = json!([{"name":"external_data","sha256":null,"secret_dependent":false,"observable":false}]);
        }
        let target = self.implemented_subject();
        let fence = format!("```mine-review-closure\n{}\n```", self.closure_assertion());
        self.closure_input(&target, &fence, false);
        let (code, begin) = self.begin_closure("3");
        assert_eq!(code, 0, "{begin}");
        git(
            self.root(),
            &[
                "merge",
                "--no-ff",
                "--no-edit",
                &format!("plan/{PLAN}-fixture"),
            ],
        );
        let mut operation = begin["data"]["operation"].clone();
        operation["stage"] = json!({"kind":"acceptance","stage":"integrated"});
        operation["stage_revision"] = json!(1);
        operation["integrated_commit"] = json!(git(self.root(), &["rev-parse", "HEAD"]));
        let (code, stage) = self.stage_closure(&operation, 0, Value::Null);
        assert_eq!(code, 0, "{stage}");
        (operation, context)
    }

    fn verification_run(&self, context: &Value) -> (Value, Value, Value, Value) {
        use mine::domain::ports::Clock;
        use mine::infrastructure::{
            system::SystemClock, verification_store::FileVerificationStore,
        };
        let (code, value) = self.verification(
            "capture",
            &json!({"schema_version":1,"plan_id":PLAN,"actor":"runner",
            "check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()}),
            None,
        );
        assert_eq!(code, 0, "{value}");
        let capture = value["data"]["capture"].clone();
        let plan: PlanIdentity = serde_json::from_value(capture["plan"].clone()).unwrap();
        let store = FileVerificationStore::new(self.root(), self.root()).unwrap();
        let policy = store
            .load_policy(&path(".mine/verification-policy.json"))
            .unwrap();
        let lease = store.lease_outputs(&plan).unwrap();
        let output = Command::new("git")
            .current_dir(self.root())
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        let output_path = format!(".mine/build/{PLAN}/gate-probe.log");
        fs::create_dir_all(self.root().join(&output_path).parent().unwrap()).unwrap();
        fs::write(self.root().join(&output_path), &output.stdout).unwrap();
        let artifact = store
            .retain(&plan, &path(&output_path), policy.check("probe").unwrap())
            .unwrap();
        let observation = json!({"ended_at":SystemClock.now_utc_rfc3339(),"exit_code":output.status.code(),"outcome":"passed",
            "warnings_observed":false,"artifacts":[artifact]});
        let (code, recorded) = self.verification(
            "record",
            &json!({"schema_version":1,"capture":capture,"observation":observation}),
            None,
        );
        assert_eq!(code, 0, "{recorded}");
        drop(lease);
        let receipt_path = format!(
            ".mine/plan/reports/verification/{PLAN}/{}.json",
            recorded["data"]["decision"]["original"]["receipt_id"]
                .as_str()
                .unwrap()
        );
        let receipt = file_json(self.root(), &receipt_path);
        git(
            self.root(),
            &["add", &format!(".mine/plan/reports/verification/{PLAN}")],
        );
        git(
            self.root(),
            &["commit", "-m", "retain real gate run evidence"],
        );
        (receipt, capture, observation, recorded)
    }

    fn gate_run(&self, operation: &Value, context: &Value) -> (Value, Value, Value) {
        use mine::{domain::ports::Clock, infrastructure::system::SystemClock};
        let (receipt, capture, observation, recorded) = self.verification_run(context);
        let claim = json!({"schema_version":1,"operation_id":operation["operation_id"],"target_stage":{"kind":"acceptance","stage":"verified"},
            "target_stage_revision":2,"expected_graph_revision":3,"check":"probe","receipt":receipt,"capture":capture,"observation":observation,
            "attested_by":"runner","attested_at":SystemClock.now_utc_rfc3339(),"statement":"executed_for_this_gate"});
        (receipt, claim, recorded)
    }

    fn cleaned_closure(&self, unknown: bool) -> (Value, Value, Value, Value) {
        use mine::infrastructure::verification_store::FileVerificationStore;
        let (integrated, context) = self.integrated_closure(unknown);
        let (receipt, claim, _) = self.gate_run(&integrated, &context);
        let identity = unknown.then(|| retained_claim(self.root(), &claim));
        let mut verified = integrated;
        verified["stage"] = json!({"kind":"acceptance","stage":"verified"});
        verified["stage_revision"] = json!(2);
        let mut evidence = vec![receipt.clone()];
        if let Some(identity) = &identity {
            evidence.push(identity.clone());
        }
        verified["verification_evidence"] = json!(evidence);
        let gate = json!({"checks":[{"check":"probe","receipt":receipt,"context":context,"fresh_observation":identity}]});
        let (code, value) = self.stage_closure(&verified, 1, gate.clone());
        assert_eq!(code, 0, "{value}");
        let plan: PlanIdentity = serde_json::from_value(verified["plan"].clone()).unwrap();
        assert!(
            FileVerificationStore::new(self.root(), self.root())
                .unwrap()
                .cleanup_outputs(&plan)
                .unwrap()
        );
        let target = self.root().join(format!(".mine/worktrees/{PLAN}"));
        git(
            self.root(),
            &["worktree", "remove", target.to_str().unwrap()],
        );
        git(
            self.root(),
            &["branch", "-d", &format!("plan/{PLAN}-fixture")],
        );
        let mut cleaned = verified;
        cleaned["stage"] = json!({"kind":"acceptance","stage":"cleaned"});
        cleaned["stage_revision"] = json!(3);
        cleaned["cleanup"] = json!({"worktree":format!(".mine/worktrees/{PLAN}"),"branch":format!("plan/{PLAN}-fixture"),"worktree_removed":true,"branch_removed":true});
        let (code, value) = self.stage_closure(&cleaned, 2, gate.clone());
        assert_eq!(code, 0, "{value}");
        (cleaned, gate, receipt, context)
    }

    fn finalize_closure(&self, accepted: &Value, gate: &Value, revision: &str) -> (i32, Value) {
        fs::write(self.root().join(CLOSURE_INPUT),serde_json::to_vec(&json!({"schema_version":1,"expected_stage_revision":3,"operation":accepted,"verification":gate})).unwrap()).unwrap();
        cli(
            self.root(),
            &[
                "operation",
                "finalize",
                "--expected-revision",
                revision,
                "--input",
                CLOSURE_INPUT,
            ],
        )
    }
}

fn file_json(root: &Path, relative: &str) -> Value {
    let body = fs::read(root.join(relative)).unwrap();
    json!({"path":relative,"bytes":body.len(),"sha256":digest(&body)})
}

fn retained_claim(root: &Path, claim: &Value) -> Value {
    let relative = format!(
        ".mine/plan/reports/verification/{PLAN}/{}.gate-claim",
        uuid::Uuid::new_v4()
    );
    fs::write(root.join(&relative), serde_json::to_vec(claim).unwrap()).unwrap();
    git(root, &["add", &relative]);
    git(root, &["commit", "-m", "explicit gate claim fixture"]);
    file_json(root, &relative)
}

#[test]
fn actual_cli_policy_registration_uses_frozen_adr_binding_and_separate_revisions() {
    let fixture = Fixture::new(None, true);
    let (code, value) = fixture.register("0");
    assert_eq!(code, 0, "{value}");
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let node = workspace.get(PLAN).unwrap();
    let provenance = node.ownership_policy.as_ref().unwrap();
    assert_eq!(workspace.schema_version, 7);
    assert_eq!(workspace.revision, 1);
    assert_eq!(provenance.admitted_graph_revision, 0);
    assert_eq!(provenance.adr_registry_revision, 3);
    assert_eq!(provenance.approval_actor, "owner");
    assert_eq!(provenance.policy_sha256, digest(&fixture.policy));
    assert!(node.body_sha256.is_some());
    let first = provenance.clone();
    let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
    assert_eq!(
        show["data"]["plan"]["effective_authority"]["policy"]["sha256"],
        json!(digest(&fixture.policy))
    );
    ok(
        fixture.root(),
        &[
            "plan",
            "withdraw",
            "--id",
            PLAN,
            "--expected-revision",
            "1",
            "--reason",
            "unused fixture registration",
            "--actor",
            "fixture-registrar",
            "--operation-id",
            &uuid::Uuid::new_v4().to_string(),
        ],
    );
    let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
    assert_eq!(
        show["data"]["plan"]["ownership_policy"],
        serde_json::to_value(&first).unwrap()
    );
    let wave = ok(fixture.root(), &["graph", "wave"]);
    assert_eq!(wave["data"]["scheduling"]["authority_mode"], "semantic");
    assert!(wave["data"]["wave"].as_array().unwrap().is_empty());
    let before = fixture.graph();
    let _ = ok(fixture.root(), &["graph", "wave"]);
    assert_eq!(
        fixture.graph(),
        before,
        "read-only reconstruction must not refresh provenance"
    );
}

#[test]
fn actual_cli_registration_refuses_missing_or_empty_conservative_mapping_without_mutation() {
    let fixture = Fixture::new(None, true);
    let graph = fixture.graph();
    let view = fs::read(fixture.root().join(".mine/plan/execution-graph.md")).unwrap();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let refs = git(
        fixture.root(),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    let index = fs::read(fixture.root().join(".git/index")).unwrap();
    let journal = journal_snapshot(fixture.root());
    for empty in [false, true] {
        let mut args = vec![
            "plan",
            "add",
            "--id",
            PLAN,
            "--path",
            PLAN_PATH,
            "--title",
            "Missing mapping",
            "--actor",
            "fixture-registrar",
            "--expected-revision",
            "0",
            "--design-ref",
            ".mine/design/index.md",
        ];
        if empty {
            args.extend(["--write", ""]);
        }
        let (code, value) = cli(fixture.root(), &args);
        assert_ne!(code, 0, "{value}");
        assert_eq!(value["error"]["code"], "MINE_GRAPH_INVALID", "{value}");
        if !empty {
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("conservative mapping is required"),
                "{value}"
            );
        }
        assert_eq!(fixture.graph(), graph);
        assert_eq!(
            fs::read(fixture.root().join(".mine/plan/execution-graph.md")).unwrap(),
            view
        );
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(
            git(
                fixture.root(),
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            ),
            refs
        );
        assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
        assert_eq!(journal_snapshot(fixture.root()), journal);
    }
}

#[test]
fn actual_cli_registration_validates_authority_without_execution_admission() {
    for semantic in [false, true] {
        let fixture = Fixture::new(None, true);
        fixture.seed_held_owner("src/fixture");
        let mut args = vec![
            "plan",
            "add",
            "--id",
            PLAN,
            "--path",
            PLAN_PATH,
            "--title",
            "Declared overlapping responsibility",
            "--actor",
            "fixture-registrar",
            "--expected-revision",
            "0",
            "--design-ref",
            ".mine/design/index.md",
        ];
        if semantic {
            args.extend(["--ownership-input", INPUT_PATH]);
        } else {
            args.extend(["--write", "src/fixture"]);
        }
        let (code, value) = cli(fixture.root(), &args);
        assert_eq!(code, 0, "semantic={semantic}: {value}");
        let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
        assert_eq!(show["data"]["plan"]["status"], "DRAFT");
        let workspace: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        let node = workspace.get(PLAN).unwrap();
        assert_eq!(workspace.revision, 1);
        assert_eq!(node.ownership.is_some(), semantic);
        assert_eq!(node.exclusive_write_paths.is_empty(), semantic);
        assert_eq!(
            workspace
                .plans
                .iter()
                .filter(|p| p.status == mine::domain::status::PlanStatus::InProgress)
                .count(),
            1
        );
    }
}

#[test]
fn actual_cli_policy_registration_refuses_stale_revision_actor_spoof_and_path_escape_without_write()
{
    for variant in 0..3 {
        let fixture = Fixture::new(None, true);
        if variant == 1 {
            fixture.change_input(|input| input["approved_by"] = json!("attacker"));
        }
        if variant == 2 {
            fixture.change_input(|input| input["policy_path"] = json!(".mine/config.toml"));
        }
        let before = fixture.graph();
        let (code, value) = fixture.register(if variant == 0 { "9" } else { "0" });
        assert_ne!(code, 0, "{variant}: {value}");
        if variant == 0 {
            assert_eq!(value["error"]["code"], "MINE_REVISION_CONFLICT");
        }
        assert_eq!(fixture.graph(), before);
    }
}

#[test]
fn actual_cli_policy_registration_refuses_unapproved_or_unbound_adr() {
    for (body, approve) in [
        (None, false),
        (
            Some("# Approved principle, no concrete policy binding\n"),
            true,
        ),
        (
            Some(
                "# Duplicate keys\n```mine-ownership-policy\n{\"schema_version\":1,\"schema_version\":1,\"policy_path\":\".mine/design/p.json\",\"policy_sha256\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}\n```\n",
            ),
            true,
        ),
    ] {
        let fixture = Fixture::new(body, approve);
        let before = fixture.graph();
        let (code, value) = fixture.register("0");
        assert_ne!(code, 0, "{value}");
        assert_eq!(fixture.graph(), before);
    }
}

#[test]
fn actual_cli_policy_binding_uses_markdown_structure_not_literal_marker_substrings() {
    let baseline = Fixture::new(None, true);
    let binding = json!({"schema_version":1,"policy_path":POLICY_PATH,"policy_sha256":digest(&baseline.policy)}).to_string();
    for body in [
        format!("# Long fence\n````mine-ownership-policy\n{binding}\n````\n"),
        format!("# Tilde fence\n~~~mine-ownership-policy\n{binding}\n~~~\n"),
        format!(
            "# Literal example is not approval\n````markdown\n```mine-ownership-policy\n{{}}\n```\n````\n\n```mine-ownership-policy\n{binding}\n```\n"
        ),
    ] {
        let fixture = Fixture::new(Some(&body), true);
        let (code, value) = fixture.register("0");
        assert_eq!(code, 0, "{value}");
    }
    let body = format!(
        "# Ambiguous approval\n```mine-ownership-policy\n{binding}\n```\n\n~~~mine-ownership-policy\n{binding}\n~~~\n"
    );
    let fixture = Fixture::new(Some(&body), true);
    let before = fixture.graph();
    assert_ne!(fixture.register("0").0, 0);
    assert_eq!(fixture.graph(), before);
}

#[test]
fn repeated_policy_binding_preserves_its_first_graph_and_adr_observation() {
    let fixture = Fixture::new(None, true);
    let second_id = format!("plan{}", 82);
    let second_path = format!(".mine/plan/{second_id}-fixture.md");
    let second_input = ".mine/plan/second-semantic-registration.json";
    let body = b"# Second immutable fixture Plan\n";
    fs::write(fixture.root().join(&second_path), body).unwrap();
    let mut input: Value =
        serde_json::from_slice(&fs::read(fixture.root().join(INPUT_PATH)).unwrap()).unwrap();
    input["ownership"]["plan"]["plan_id"] = json!(second_id);
    input["ownership"]["plan"]["plan_path"] = json!(second_path);
    input["ownership"]["plan"]["lineage_id"] = json!(second_id);
    input["ownership"]["plan"]["branch"] = json!(format!("plan/{second_id}-fixture"));
    input["ownership"]["plan"]["body_sha256"] = json!(digest(body));
    input["ownership"]["physical_mapping"] = json!(["src/fixture2"]);
    fs::write(
        fixture.root().join(second_input),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    git(fixture.root(), &["add", &second_path, second_input]);
    git(
        fixture.root(),
        &["commit", "-m", "second immutable registration inputs"],
    );
    let (code, value) = fixture.register("0");
    assert_eq!(code, 0, "{value}");
    ok(
        fixture.root(),
        &[
            "plan",
            "add",
            "--id",
            &second_id,
            "--path",
            &second_path,
            "--title",
            "Second fixture",
            "--actor",
            "fixture-registrar",
            "--design-ref",
            ".mine/design/index.md",
            "--write",
            "src/fixture2",
            "--expected-revision",
            "1",
            "--ownership-input",
            second_input,
        ],
    );
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let first = workspace
        .get(PLAN)
        .unwrap()
        .ownership_policy
        .as_ref()
        .unwrap();
    let second = workspace
        .get(&second_id)
        .unwrap()
        .ownership_policy
        .as_ref()
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(second.admitted_graph_revision, 0);
    assert_eq!(second.adr_registry_revision, 3);
    let before = fixture.graph();
    let _ = ok(fixture.root(), &["graph", "wave"]);
    assert_eq!(fixture.graph(), before);
}

#[test]
fn actual_cli_policy_tamper_is_not_authorized_by_recomputed_caller_hash() {
    let fixture = Fixture::new(None, true);
    let mut altered = fixture.policy.clone();
    altered.push(b'\n');
    fs::write(fixture.root().join(POLICY_PATH), &altered).unwrap();
    fixture.change_input(|input| input["ownership"]["policy"]["sha256"] = json!(digest(&altered)));
    git(fixture.root(), &["add", POLICY_PATH]);
    git(
        fixture.root(),
        &["commit", "-m", "unapproved exact policy bytes"],
    );
    let before = fixture.graph();
    let (code, value) = fixture.register("0");
    assert_ne!(code, 0, "{value}");
    assert_eq!(fixture.graph(), before);
}

#[test]
fn actual_cli_legacy_show_and_wave_do_not_invent_a_semantic_policy_or_body_digest() {
    let fixture = Fixture::new(None, true);
    for owned in [
        POLICY_PATH,
        ADR_PATH,
        INPUT_PATH,
        ".mine/design/decisions/adr-registry.toml",
        ".mine/design/decisions/index.md",
    ] {
        fs::remove_file(fixture.root().join(owned)).unwrap();
    }
    let mut workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    workspace.plans.push(serde_json::from_value(json!({
        "id":PLAN,"path":PLAN_PATH,"title":"Legacy fixture","status":"READY",
        "branch":format!("plan/{PLAN}-fixture"),"lineage_id":PLAN,"worktree_path":format!(".mine/worktrees/{PLAN}"),
        "design_references":[".mine/design/index.md"],"exclusive_write_paths":["src/fixture"]
    })).unwrap());
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.md"),
        render_markdown(&workspace).unwrap(),
    )
    .unwrap();
    git(fixture.root(), &["add", "-u", ".mine/design", ".mine/plan"]);
    git(
        fixture.root(),
        &["commit", "-m", "genuine policy-free legacy fixture"],
    );
    let before = fixture.graph();
    let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
    assert!(show["data"]["plan"]["effective_authority"]["policy"].is_null());
    assert!(
        show["data"]["plan"]["effective_authority"]["original"]["plan"]["body_sha256"].is_null()
    );
    assert_eq!(
        show["data"]["plan"]["effective_authority"]["plan"]["body_sha256"],
        json!(digest(&fs::read(fixture.root().join(PLAN_PATH)).unwrap()))
    );
    let wave = ok(fixture.root(), &["graph", "wave"]);
    assert_eq!(wave["data"]["scheduling"]["authority_mode"], "conservative");
    assert!(wave["data"]["scheduling"]["policy"].is_null());
    assert_eq!(wave["data"]["wave"], json!([PLAN]));
    assert_eq!(fixture.graph(), before);
}

#[test]
fn actual_cli_policy_refuses_approved_body_tamper_and_multiple_binding_blocks() {
    let fixture = Fixture::new(None, true);
    fs::write(
        fixture.root().join(ADR_PATH),
        "# Modified approved decision\n",
    )
    .unwrap();
    git(fixture.root(), &["add", ADR_PATH]);
    git(fixture.root(), &["commit", "-m", "tampered approved ADR"]);
    let before = fixture.graph();
    assert_ne!(fixture.register("0").0, 0);
    assert_eq!(fixture.graph(), before);
    let multiple = "# Multiple bindings\n```mine-ownership-policy\n{}\n```\n```mine-ownership-policy\n{}\n```\n";
    let fixture = Fixture::new(Some(multiple), true);
    let before = fixture.graph();
    assert_ne!(fixture.register("0").0, 0);
    assert_eq!(fixture.graph(), before);
}

#[test]
fn every_existing_plan_mutation_retains_stale_caller_revision_before_expensive_git() {
    let fixture = Fixture::new(None, true);
    // Clarification's explicit actor is supplied by its typed input. Keep that
    // input valid so this tests the observer gate, not missing-file decoding.
    fixture.clarify_input(|_| {});
    fs::create_dir_all(fixture.root().join(".mine/runtime")).unwrap();
    let trace = fixture.root().join(".mine/runtime/observer-trace.jsonl");
    let review = format!(".mine/plan/reports/{PLAN}-fixture-review.md");
    let implementation = format!(".mine/plan/reports/{PLAN}-fixture-implementation.md");
    let compensation = format!("{PLAN}-C01");
    let commit = "1".repeat(40);
    let operation = uuid::Uuid::new_v4().to_string();
    let cases: Vec<Vec<&str>> = vec![
        vec![
            "plan",
            "add",
            "--actor",
            "fixture-registrar",
            "--id",
            PLAN,
            "--path",
            PLAN_PATH,
            "--title",
            "Fixture",
            "--design-ref",
            ".mine/design/index.md",
        ],
        vec!["plan", "clarify", "--id", PLAN, "--input", CLARIFY_PATH],
        vec!["plan", "start", "--id", PLAN, "--owner", "executor"],
        vec![
            "plan",
            "implemented",
            "--actor",
            "executor",
            "--id",
            PLAN,
            "--report",
            &implementation,
            "--commit",
            &commit,
        ],
        vec![
            "plan",
            "accept",
            "--id",
            PLAN,
            "--operation-id",
            &operation,
            "--input",
            CLOSURE_INPUT,
        ],
        vec![
            "plan",
            "reject",
            "--actor",
            "reviewer",
            "--id",
            PLAN,
            "--reason",
            "fixture",
            "--review-report",
            &review,
            "--compensating-plan",
            &compensation,
        ],
        vec!["plan", "release", "--id", PLAN, "--actor", "coordinator"],
        vec![
            "plan",
            "withdraw",
            "--id",
            PLAN,
            "--reason",
            "fixture",
            "--actor",
            "coordinator",
            "--operation-id",
            &operation,
        ],
        vec![
            "plan",
            "rewire-compensation",
            "--id",
            PLAN,
            "--actor",
            "coordinator",
        ],
    ];
    let before = fixture.graph();
    for mut args in cases {
        args.extend(["--expected-revision", "9"]);
        let (code, value) = cli_observed(fixture.root(), &args, Some(&trace));
        assert_ne!(code, 0, "{args:?}: {value}");
        assert_eq!(
            value["error"]["code"], "MINE_REVISION_CONFLICT",
            "{args:?}: {value}"
        );
        assert_eq!(fixture.graph(), before);
    }
    let records = fs::read_to_string(trace).unwrap();
    for line in records.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["event"] == "start" {
            let argv = record["argv"].as_array().unwrap();
            for forbidden in ["cat-file", "for-each-ref", "status", "merge-base"] {
                assert!(
                    !argv.iter().any(|arg| arg.as_str() == Some(forbidden)),
                    "stale caller launched {record}"
                );
            }
        }
    }
}

#[test]
fn actual_cli_clarification_appends_only_and_rechecks_actor_tail_body_and_policy() {
    let fixture = Fixture::new(None, true);
    assert_eq!(fixture.register("0").0, 0);
    let original: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    fixture.clarify_input(|_| {});
    let (code, value) = fixture.clarify("1");
    assert_eq!(code, 0, "{value}");
    let after: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    assert_eq!(after.revision, 2);
    assert_eq!(after.plans[0].ownership, original.plans[0].ownership);
    assert_eq!(
        after.plans[0].exclusive_write_paths,
        original.plans[0].exclusive_write_paths
    );
    assert_eq!(after.plans[0].body_sha256, original.plans[0].body_sha256);
    assert_eq!(after.plans[0].clarifications.len(), 1);
    let tail = after.plans[0].clarifications[0].digest();
    for variant in 0..5 {
        fixture.clarify_input(|input| {
            input["expected_tail"] = json!(tail);
            match variant {
                0 => input["actor"] = json!("intruder"),
                1 => input["expected_tail"] = Value::Null,
                2 => input["plan"]["body_sha256"] = json!("0".repeat(64)),
                3 => input["physical_mapping"] = json!(["tests/foreign"]),
                _ => input["approving_adr"] = json!("ADR-0043"),
            }
        });
        let before = fixture.graph();
        assert_ne!(fixture.clarify("2").0, 0, "variant {variant}");
        assert_eq!(fixture.graph(), before);
    }
    fixture.clarify_input(|input| input["expected_tail"] = json!(tail));
    assert_eq!(
        fixture.clarify("1").1["error"]["code"],
        "MINE_REVISION_CONFLICT"
    );
}

#[test]
fn actual_cli_first_activation_preserves_known_or_absent_original_digest_and_mapping() {
    for known in [false, true] {
        let fixture = Fixture::new(None, true);
        fixture.seed_legacy_node(known);
        let original: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
        assert!(show["data"]["plan"]["effective_authority"]["policy"].is_null());
        let registration: Value =
            serde_json::from_slice(&fs::read(fixture.root().join(INPUT_PATH)).unwrap()).unwrap();
        fixture.clarify_input(|input| {
            input["kind"] = json!("baseline_attestation");
            input["physical_mapping"] = json!(["src/fixture"]);
            input["baseline"] = registration["ownership"].clone();
            input["baseline"]["plan"]["body_sha256"] = json!(original.plans[0].body_sha256);
        });
        let (code, value) = fixture.clarify("0");
        assert_eq!(code, 0, "known={known}: {value}");
        let after: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(after.schema_version, 7);
        assert_eq!(after.plans[0].body_sha256, original.plans[0].body_sha256);
        assert_eq!(
            after.plans[0].exclusive_write_paths,
            original.plans[0].exclusive_write_paths
        );
        assert_eq!(
            after.plans[0].clarifications[0].original_body,
            original.plans[0].body_sha256
        );
        assert!(
            after.plans[0].clarifications[0]
                .plan
                .fields()
                .body_sha256
                .is_some()
        );
        assert_eq!(
            after.plans[0]
                .ownership_policy
                .as_ref()
                .unwrap()
                .admitted_graph_revision,
            0
        );
        let before = fixture.graph();
        let _ = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
        assert_eq!(fixture.graph(), before);
    }
}

#[test]
fn actual_cli_clarification_and_first_activation_refuse_held_owner_conflicts() {
    for first_activation in [false, true] {
        let fixture = Fixture::new(None, true);
        let revision = if first_activation {
            fixture.seed_legacy_node(false);
            "0"
        } else {
            assert_eq!(fixture.register("0").0, 0);
            "1"
        };
        fixture.seed_held_owner(if first_activation {
            "src/fixture"
        } else {
            "src/fixture2"
        });
        fixture.clarify_input(|input| {
            if first_activation {
                let registration: Value =
                    serde_json::from_slice(&fs::read(fixture.root().join(INPUT_PATH)).unwrap())
                        .unwrap();
                input["kind"] = json!("baseline_attestation");
                input["physical_mapping"] = json!(["src/fixture"]);
                input["baseline"] = registration["ownership"].clone();
                input["baseline"]["plan"]["body_sha256"] = Value::Null;
            }
        });
        let before = fixture.graph();
        let (code, value) = fixture.clarify(revision);
        assert_ne!(code, 0, "{value}");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("clarification ownership blocked"),
            "{value}"
        );
        assert_eq!(fixture.graph(), before);
        let wave = ok(fixture.root(), &["graph", "wave"]);
        assert_eq!(wave["revision_before"], revision.parse::<u64>().unwrap());
    }
}

#[test]
fn actual_cli_start_rechecks_common_held_owner_before_preparation_or_graph_mutation() {
    let fixture = Fixture::new(None, true);
    fixture.seed_legacy_node(false);
    fixture.seed_held_owner("src/fixture");
    let wave = ok(fixture.root(), &["graph", "wave"]);
    assert_eq!(wave["data"]["wave"], json!([]));
    let before = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let (code, value) = cli(
        fixture.root(),
        &[
            "plan",
            "start",
            "--id",
            PLAN,
            "--expected-revision",
            "0",
            "--owner",
            "contender",
            "--run-id",
            "must-not-start",
        ],
    );
    assert_ne!(code, 0, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ownership admission blocked"),
        "{value}"
    );
    assert_eq!(fixture.graph(), before);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    assert!(
        git(
            fixture.root(),
            &["branch", "--list", &format!("plan/{PLAN}-fixture")]
        )
        .is_empty()
    );
    assert!(
        !fixture
            .root()
            .join(format!(".mine/worktrees/{PLAN}"))
            .exists()
    );
}

#[test]
fn actual_cli_supported_schema_mutation_preserves_absent_historical_provenance() {
    for version in [3, 4, 5, 6] {
        let fixture = Fixture::new(None, true);
        fixture.seed_legacy_node(false);
        let mut workspace: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        workspace.schema_version = version;
        fs::write(
            fixture.root().join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        fs::write(
            fixture.root().join(".mine/plan/execution-graph.md"),
            render_markdown(&workspace).unwrap(),
        )
        .unwrap();
        git(fixture.root(), &["add", ".mine/plan"]);
        // v4 setup is already committed, so allow a deliberate empty fixture commit.
        git(
            fixture.root(),
            &["commit", "--allow-empty", "-m", "supported schema fixture"],
        );
        ok(
            fixture.root(),
            &[
                "plan",
                "withdraw",
                "--id",
                PLAN,
                "--expected-revision",
                "0",
                "--reason",
                "unused legacy fixture",
                "--actor",
                "coordinator",
                "--operation-id",
                &uuid::Uuid::new_v4().to_string(),
            ],
        );
        let migrated: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(migrated.schema_version, 7);
        assert_eq!(migrated.revision, 1);
        let node = &migrated.plans[0];
        assert!(node.body_sha256.is_none());
        assert!(node.ownership.is_none());
        assert!(node.ownership_policy.is_none());
        assert!(node.acceptance.is_none());
        assert!(node.preparations.is_empty());
        assert!(node.started_preparation_generation.is_none());
    }
}

#[test]
fn actual_cli_operation_query_is_read_only_during_pending_view_and_rejects_foreign_binding() {
    use mine::domain::lifecycle_operation::*;
    use mine::infrastructure::{
        file_lock, graph_checkpoint::CheckpointKernel, toml_store::GRAPH_LOCK_NAME,
    };
    let fixture = Fixture::new(None, true);
    let mut candidate: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    candidate.revision = 1;
    let id = OperationId::generate();
    let record = {
        let guard = file_lock::acquire_exclusive(
            &fixture.root().join(".mine/locks").join(GRAPH_LOCK_NAME),
            std::time::Duration::from_secs(2),
        )
        .unwrap();
        let kernel = CheckpointKernel::under_lock(fixture.root(), &guard).unwrap();
        kernel
            .begin(
                id.clone(),
                0,
                "query-fixture",
                &candidate,
                &mut NoInterruption,
            )
            .unwrap()
    };
    let journal = fixture
        .root()
        .join(format!(".mine/runtime/operations/{}.json", id.as_str()));
    let bytes = fs::read(&journal).unwrap();
    let graph = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    fs::remove_file(fixture.root().join(".mine/plan/execution-graph.md")).unwrap();
    for _ in 0..2 {
        let value = ok(
            fixture.root(),
            &["operation", "show", "--operation-id", id.as_str()],
        );
        assert_eq!(
            value["data"]["operation"],
            serde_json::to_value(&record.operation).unwrap()
        );
        assert!(value["data"].get("checkpoint").is_none());
        assert!(!value["data"].to_string().contains("original_index\":"));
        assert_eq!(fixture.graph(), graph);
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert!(
            !fixture
                .root()
                .join(".mine/plan/execution-graph.md")
                .exists()
        );
    }
    let mut foreign: Value = serde_json::from_slice(&bytes).unwrap();
    foreign["operation"]["repository_id"] = json!(uuid::Uuid::new_v4().to_string());
    let foreign = serde_json::to_vec(&foreign).unwrap();
    fs::write(&journal, &foreign).unwrap();
    assert_ne!(
        cli(
            fixture.root(),
            &["operation", "show", "--operation-id", id.as_str()]
        )
        .0,
        0
    );
    assert_eq!(fs::read(&journal).unwrap(), foreign);
    assert_eq!(fixture.graph(), graph);
    assert_ne!(
        cli(
            fixture.root(),
            &["operation", "show", "--operation-id", "../foreign"]
        )
        .0,
        0
    );
}

#[test]
fn actual_cli_coordinator_lock_refuses_external_directory_alias_before_creation() {
    let fixture = Fixture::new(None, true);
    assert_eq!(fixture.register("0").0, 0);
    let graph = fixture.graph();
    let outside = tempfile::tempdir().unwrap();
    let locks = fixture.root().join(".mine/locks");
    // Move only this disposable fixture's existing lock directory; no real
    // repository locks or foreign processes are touched.
    fs::rename(&locks, fixture.root().join(".mine/fixture-saved-locks")).unwrap();
    directory_alias(&locks, outside.path());
    for args in [
        vec!["plan", "show", "--id", PLAN],
        vec![
            "plan",
            "release",
            "--id",
            PLAN,
            "--expected-revision",
            "1",
            "--actor",
            "coordinator",
        ],
    ] {
        let (code, value) = cli(fixture.root(), &args);
        assert_ne!(code, 0, "{value}");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("link/reparse"),
            "{value}"
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        assert_eq!(fixture.graph(), graph);
    }
}

fn withdrawal_subject(artifacts: u8) -> (Fixture, std::path::PathBuf) {
    let fixture = Fixture::new(None, true);
    fixture.seed_legacy_node(true);
    let target = fixture.root().join(format!(".mine/worktrees/{PLAN}"));
    let branch = format!("plan/{PLAN}-fixture");
    if artifacts > 0 {
        git(fixture.root(), &["branch", &branch, "HEAD"]);
    }
    if artifacts > 1 {
        git(
            fixture.root(),
            &["worktree", "add", target.to_str().unwrap(), &branch],
        );
    }
    (fixture, target)
}
fn withdrawal_args(operation: &str) -> Vec<&str> {
    vec![
        "plan",
        "withdraw",
        "--id",
        PLAN,
        "--reason",
        "retired fixture",
        "--actor",
        "coordinator",
        "--operation-id",
        operation,
        "--expected-revision",
        "0",
    ]
}

fn preparation_args<'a>(
    id: &'a str,
    operation: &'a str,
    revision: &'a str,
    actor: &'a str,
) -> Vec<&'a str> {
    vec![
        "plan",
        "prepare",
        "--id",
        id,
        "--operation-id",
        operation,
        "--expected-revision",
        revision,
        "--actor",
        actor,
    ]
}

fn legacy_current_handoff_fixture(
    switched: bool,
) -> (Fixture, std::path::PathBuf, String, String, Value, Vec<u8>) {
    let fixture = Fixture::new(None, true);
    fixture.seed_legacy_node(false);
    let root = fixture.root();
    let anchor = git(root, &["rev-parse", "HEAD"]);
    let branch = format!("plan/{PLAN}-fixture");
    let target = root.join(format!(".mine/worktrees/{PLAN}"));
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            target.to_str().unwrap(),
            "HEAD",
        ],
    );
    fs::write(
        target.join("rejected-product.txt"),
        "legacy rejected-only product\n",
    )
    .unwrap();
    git(&target, &["add", "rejected-product.txt"]);
    git(
        &target,
        &[
            "commit",
            "-m",
            "legacy implementation P before its Root report",
        ],
    );
    let reviewed = git(&target, &["rev-parse", "HEAD"]);
    let cp = format!("{PLAN}-C01");
    let cp_path = format!(".mine/plan/{cp}-fixture.md");
    let report_path = format!(".mine/plan/reports/{PLAN}-fixture-review.md");
    let historical=b"# Historical independent review\n\nVerdict: REJECTED\n\nOriginal legacy findings are retained verbatim.\n";
    let cp_body = b"# Legacy compensation body created on Root after P\n";
    fs::write(root.join(&report_path), historical).unwrap();
    fs::write(root.join(&cp_path), cp_body).unwrap();
    let mut graph: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let mut compensation = graph.plans[0].clone();
    compensation.id = cp.clone();
    compensation.path = cp_path.clone();
    compensation.branch = format!("plan/{cp}-fixture");
    // Both unknown historical digests remain unknown in this imported snapshot.
    assert!(compensation.body_sha256.is_none());
    graph.plans[0].status = mine::domain::status::PlanStatus::Rejected;
    graph.plans[0].owner = "executor".into();
    graph.plans[0].run_id = "fixture-run".into();
    graph.plans[0].branch_anchor = anchor;
    graph.plans[0].review_report = report_path.clone();
    graph.plans[0].compensating_plan = cp.clone();
    graph.plans.push(compensation);
    fs::write(
        root.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&graph).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".mine/plan/execution-graph.md"),
        render_markdown(&graph).unwrap(),
    )
    .unwrap();
    git(root, &["add", ".mine/plan"]);
    git(
        root,
        &[
            "commit",
            "-m",
            "explicit fully-rewired legacy rejected and READY snapshot",
        ],
    );
    if switched {
        git(root, &["branch", &format!("plan/{cp}-fixture"), &reviewed]);
        git(
            &target,
            &["switch", "--no-guess", &format!("plan/{cp}-fixture")],
        );
    }
    let current = format!(
        "\n## Current recovery evidence (2026-10-16)\n\n```mine-review-closure\n{}\n```\n",
        fixture.closure_assertion()
    );
    let mut report = historical.to_vec();
    report.extend_from_slice(current.as_bytes());
    fs::write(root.join(&report_path), &report).unwrap();
    git(root, &["add", &report_path]);
    git(
        root,
        &[
            "commit",
            "-m",
            "append current handoff evidence without rewriting legacy review",
        ],
    );
    assert!(report.starts_with(historical));
    assert!(!target.join(&report_path).exists());
    assert!(!target.join(&cp_path).exists());
    assert_eq!(git(root, &["rev-parse", &branch]), reviewed);
    let mut identity = fixture.closure_assertion()["plan"].clone();
    identity["plan_id"] = json!(cp);
    identity["plan_path"] = json!(cp_path);
    identity["branch"] = json!(format!("plan/{cp}-fixture"));
    identity["body_sha256"] = json!(digest(cp_body));
    let begin = json!({"schema_version":1,"actor":"reviewer","intent":{"kind":"rejected_handoff","plan_id":PLAN,
        "review":{"reviewed_tip":reviewed,"report":{"path":report_path,"bytes":report.len(),"sha256":digest(&report)}},"compensation":identity}});
    (fixture, target, cp, reviewed, begin, historical.to_vec())
}

#[test]
fn actual_cli_checkpointed_view_restore_and_repeat_do_not_mutate_graph_or_git() {
    let (fixture, _) = withdrawal_subject(0);
    let source = fixture.graph();
    let view = fixture.root().join(".mine/plan/execution-graph.md");
    let canonical = fs::read(&view).unwrap();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.root().join(".git/index")).unwrap();
    fs::remove_file(&view).unwrap();
    let response = ok(
        fixture.root(),
        &["graph", "render", "--expected-revision", "0"],
    );
    assert_eq!(response["data"]["action"], "restored");
    assert_eq!(response["revision_before"], 0);
    assert_eq!(response["revision_after"], 0);
    assert_eq!(fs::read(&view).unwrap(), canonical);
    assert_eq!(fixture.graph(), source);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
    let repeated = ok(
        fixture.root(),
        &["graph", "render", "--expected-revision", "0"],
    );
    assert_eq!(repeated["data"]["action"], "unchanged");
    assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
    assert_eq!(git(fixture.root(), &["status", "--porcelain"]), "");
}

#[test]
fn actual_cli_checkpointed_view_restore_preserves_foreign_staged_and_stale_states() {
    for failure in 0..5 {
        let (fixture, _) = withdrawal_subject(0);
        let view = fixture.root().join(".mine/plan/execution-graph.md");
        match failure {
            0 => fs::write(&view, b"foreign edited projection\n").unwrap(),
            1 => {
                fs::remove_file(&view).unwrap();
                git(fixture.root(), &["add", ".mine/plan/execution-graph.md"]);
            }
            2 => {
                fs::remove_file(&view).unwrap();
                fs::write(fixture.root().join("foreign.txt"), "preserve\n").unwrap();
            }
            3 => {
                fs::remove_file(&view).unwrap();
                fs::write(fixture.root().join(".git/index.lock"), "foreign lock\n").unwrap();
            }
            4 => {
                fs::remove_file(&view).unwrap();
            }
            _ => unreachable!(),
        }
        let graph = fixture.graph();
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let index = fs::read(fixture.root().join(".git/index")).unwrap();
        let before_view = fs::read(&view).ok();
        let (code, response) = cli(
            fixture.root(),
            &[
                "graph",
                "render",
                "--expected-revision",
                if failure == 4 { "1" } else { "0" },
            ],
        );
        assert_ne!(code, 0, "{failure}: {response}");
        if failure == 4 {
            assert_eq!(response["error"]["code"], "MINE_REVISION_CONFLICT");
        }
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
        assert_eq!(fs::read(&view).ok(), before_view);
    }
}

#[test]
fn checkpointed_view_restore_destination_race_and_lost_response_are_non_clobbering() {
    use mine::{
        domain::lifecycle_operation::*,
        infrastructure::{file_lock::acquire_exclusive, graph_checkpoint::CheckpointKernel},
    };
    struct Observe {
        view: std::path::PathBuf,
        boundary: Boundary,
        collision: bool,
    }
    impl BoundaryObserver for Observe {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == self.boundary {
                if self.collision {
                    fs::write(&self.view, b"racing foreign projection\n")?;
                } else {
                    return Err(OperationError::Interrupted(boundary));
                }
            }
            Ok(())
        }
    }
    for (boundary, collision) in [
        (Boundary::ViewRestorePrepared, true),
        (Boundary::ViewRestorePrepared, false),
        (Boundary::ViewRestorePublished, false),
    ] {
        let (fixture, _) = withdrawal_subject(0);
        let view = fixture.root().join(".mine/plan/execution-graph.md");
        let canonical = fs::read(&view).unwrap();
        fs::remove_file(&view).unwrap();
        let root = fixture.root().canonicalize().unwrap();
        let head = git(&root, &["rev-parse", "HEAD"]);
        let graph = fixture.graph();
        let lock = acquire_exclusive(
            &root.join(".mine/locks/execution-graph.lock"),
            std::time::Duration::from_secs(2),
        )
        .unwrap();
        let kernel = CheckpointKernel::under_lock(&root, &lock).unwrap();
        let error = kernel
            .restore_view_under_lock(
                0,
                &mut Observe {
                    view: view.clone(),
                    boundary,
                    collision,
                },
            )
            .unwrap_err();
        if boundary == Boundary::ViewRestorePublished {
            assert!(error.to_string().contains("was published"));
            assert_eq!(fs::read(&view).unwrap(), canonical);
        } else if collision {
            assert_eq!(fs::read(&view).unwrap(), b"racing foreign projection\n");
        } else {
            assert!(!view.exists());
        }
        drop(lock);
        if boundary == Boundary::ViewRestorePublished {
            let replay = ok(&root, &["graph", "render", "--expected-revision", "0"]);
            assert_eq!(replay["data"]["action"], "unchanged");
        }
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    }
}

#[test]
fn actual_cli_native_prepare_persists_distinct_generation_and_replays_without_refresh() {
    let (fixture, target) = withdrawal_subject(0);
    let original = git(fixture.root(), &["rev-parse", "HEAD"]);
    let stable = git(fixture.root(), &["rev-parse", "master"]);
    let id = uuid::Uuid::new_v4().to_string();
    let args = preparation_args(PLAN, &id, "0", "coordinator");
    let response = ok(fixture.root(), &args);
    assert_eq!(response["revision_before"], 0);
    assert_eq!(response["revision_after"], 1);
    assert_eq!(response["data"]["plan"]["action"], "created");
    let generation = &response["data"]["plan"]["generation"];
    assert_eq!(generation["ordinal"], 1);
    assert_eq!(generation["actor"], "coordinator");
    assert_eq!(generation["operation_id"], id);
    assert_eq!(generation["observed_root_commit"], original);
    assert_eq!(generation["prepared_branch_commit"], original);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), original);
    let checkpoint = git(fixture.root(), &["rev-parse", "HEAD"]);
    assert_ne!(checkpoint, original);
    assert_eq!(git(fixture.root(), &["status", "--porcelain"]), "");
    let before = fixture.graph();
    let replay = ok(fixture.root(), &args);
    assert_eq!(replay["data"]["plan"]["replay"], true);
    assert_eq!(replay["data"]["plan"]["generation"], *generation);
    assert_eq!(fixture.graph(), before);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), checkpoint);
    let changed = preparation_args(PLAN, &id, "0", "different-actor");
    assert_ne!(cli(fixture.root(), &changed).0, 0);
    assert_eq!(fixture.graph(), before);
    let reviewer_id = uuid::Uuid::new_v4().to_string();
    let reuse = ok(
        fixture.root(),
        &preparation_args(PLAN, &reviewer_id, "1", "reviewer"),
    );
    assert_eq!(reuse["revision_before"], 1);
    assert_eq!(reuse["revision_after"], 1);
    assert_eq!(reuse["data"]["plan"]["action"], "reused");
    assert_eq!(reuse["data"]["plan"]["request_actor"], "reviewer");
    assert_eq!(reuse["data"]["plan"]["generation"], *generation);
    assert_eq!(fixture.graph(), before);
    assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    assert_eq!(git(fixture.root(), &["rev-parse", "master"]), stable);
}

#[test]
fn actual_cli_native_start_pins_preparation_and_checkpoints_before_grant() {
    let (fixture, target) = withdrawal_subject(0);
    let original = git(fixture.root(), &["rev-parse", "HEAD"]);
    let id = uuid::Uuid::new_v4().to_string();
    ok(
        fixture.root(),
        &preparation_args(PLAN, &id, "0", "coordinator"),
    );
    let before = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    assert_ne!(head, original);
    assert_ne!(
        cli(
            fixture.root(),
            &["plan", "start", "--id", PLAN, "--expected-revision", "1"]
        )
        .0,
        0
    );
    assert_eq!(fixture.graph(), before);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    let response = ok(
        fixture.root(),
        &[
            "plan",
            "start",
            "--id",
            PLAN,
            "--expected-revision",
            "1",
            "--owner",
            "executor",
            "--run-id",
            "native-start",
        ],
    );
    assert_eq!(response["revision_before"], 1);
    assert_eq!(response["revision_after"], 2);
    assert_eq!(response["data"]["branch_anchor"], original);
    assert_eq!(response["data"]["operation"]["actor"], "executor");
    assert_eq!(response["data"]["operation"]["outcome"], "committed");
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let node = workspace.get(PLAN).unwrap();
    assert_eq!(node.started_preparation_generation, Some(1));
    assert_eq!(node.branch_anchor, original);
    assert_eq!(node.owner, "executor");
    assert_eq!(node.run_id, "native-start");
    assert_eq!(git(fixture.root(), &["status", "--porcelain"]), "");
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), original);
    assert_eq!(
        git(
            fixture.root(),
            &["rev-list", "--count", &format!("{original}..HEAD")]
        ),
        "2"
    );
    let replay_id = uuid::Uuid::new_v4().to_string();
    let reuse = ok(
        fixture.root(),
        &preparation_args(PLAN, &replay_id, "2", "reviewer"),
    );
    assert_eq!(reuse["data"]["plan"]["action"], "reused");
    assert_eq!(reuse["revision_after"], 2);
}

#[test]
fn actual_cli_native_prepare_refuses_replaced_ready_checkout_for_reuse_refresh_and_replay() {
    let (fixture, target) = withdrawal_subject(0);
    let first = uuid::Uuid::new_v4().to_string();
    ok(
        fixture.root(),
        &preparation_args(PLAN, &first, "0", "coordinator"),
    );
    let original_gitfile = fs::read(target.join(".git")).unwrap();
    let retained = fixture.root().join(".mine/worktrees/retained-original");
    git(
        fixture.root(),
        &[
            "worktree",
            "move",
            target.to_str().unwrap(),
            retained.to_str().unwrap(),
        ],
    );
    git(&retained, &["switch", "--detach"]);
    git(
        fixture.root(),
        &[
            "worktree",
            "add",
            target.to_str().unwrap(),
            &format!("plan/{PLAN}-fixture"),
        ],
    );
    let replacement_gitfile = fs::read(target.join(".git")).unwrap();
    assert_ne!(original_gitfile, replacement_gitfile);
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let graph = fixture.graph();
    for mode in 0..3 {
        let next = uuid::Uuid::new_v4().to_string();
        let mut args = if mode == 2 {
            preparation_args(PLAN, &first, "0", "coordinator")
        } else {
            preparation_args(PLAN, &next, "1", "coordinator")
        };
        if mode == 1 {
            args.push("--reprepare");
        }
        let (code, response) = cli(fixture.root(), &args);
        assert_ne!(code, 0, "replacement admitted mode{mode}: {response}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(target.join(".git")).unwrap(), replacement_gitfile);
        assert!(retained.exists());
    }
}

#[test]
fn actual_cli_legacy_rejection_handoff_admits_only_its_compensation_without_fabricating_history() {
    use mine::domain::lifecycle_operation::{OperationStage, RejectionStage};
    let fixture = Fixture::new(None, true);
    fixture.seed_legacy_node(false);
    let root = fixture.root();
    let mut workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let node = workspace.get_mut(PLAN).unwrap();
    node.status = mine::domain::status::PlanStatus::Implemented;
    node.owner = "executor".into();
    node.run_id = "fixture-run".into();
    node.branch_anchor = git(root, &["rev-parse", "HEAD"]);
    fs::write(
        root.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".mine/plan/execution-graph.md"),
        render_markdown(&workspace).unwrap(),
    )
    .unwrap();
    git(root, &["add", ".mine/plan"]);
    git(
        root,
        &[
            "commit",
            "-m",
            "explicit original-body-absent legacy implemented fixture",
        ],
    );
    let target = root.join(format!(".mine/worktrees/{PLAN}"));
    let base_branch = format!("plan/{PLAN}-fixture");
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &base_branch,
            target.to_str().unwrap(),
            "HEAD",
        ],
    );
    let assertion = format!(
        "```mine-review-closure\n{}\n```",
        fixture.closure_assertion()
    );
    let intent = fixture.closure_input(&target, &assertion, true);
    let reviewed = git(&target, &["rev-parse", "HEAD"]);
    let report = intent["intent"]["review"]["report"]["path"]
        .as_str()
        .unwrap();
    assert!(!target.join(report).exists());
    let (code, begin) = fixture.begin_closure("0");
    assert_eq!(code, 0, "{begin}");
    let mut operation = begin["data"]["operation"].clone();
    let cp = format!("{PLAN}-C01");
    let cp_path = format!(".mine/plan/{cp}-fixture.md");
    ok(
        root,
        &[
            "plan",
            "reject",
            "--id",
            PLAN,
            "--actor",
            "reviewer",
            "--reason",
            "bounded rejected fixture",
            "--review-report",
            report,
            "--compensating-plan",
            &cp,
            "--expected-revision",
            "0",
        ],
    );
    fn advance(fixture: &Fixture, current: &Value, stage: RejectionStage, revision: &str) -> Value {
        let terminal = stage == RejectionStage::Complete;
        let expected = current["stage_revision"].as_u64().unwrap();
        let mut next = current.clone();
        next["stage_revision"] = json!(expected + 1);
        next["stage"] = json!(OperationStage::RejectedHandoff(stage));
        if terminal {
            next["outcome"] = json!("committed");
        }
        fs::write(
            fixture.root().join(CLOSURE_INPUT),
            serde_json::to_vec(&json!({"schema_version":1,
            "expected_stage_revision":expected,"operation":next,"verification":null}))
            .unwrap(),
        )
        .unwrap();
        let response = ok(
            fixture.root(),
            &[
                "operation",
                if terminal { "finalize" } else { "stage" },
                "--expected-revision",
                revision,
                "--input",
                CLOSURE_INPUT,
            ],
        );
        if terminal {
            assert!(response["data"]["checkpoint_operation_id"].is_null());
            assert_eq!(response["revision_before"], response["revision_after"]);
        }
        response["data"]["operation"].clone()
    }
    operation = advance(&fixture, &operation, RejectionStage::Rejected, "1");
    ok(
        root,
        &[
            "plan",
            "add",
            "--id",
            &cp,
            "--path",
            &cp_path,
            "--title",
            "Compensation fixture",
            "--design-ref",
            ".mine/design/index.md",
            "--write",
            "src/fixture",
            "--actor",
            "coordinator",
            "--expected-revision",
            "1",
        ],
    );
    operation = advance(
        &fixture,
        &operation,
        RejectionStage::CompensationRegistered,
        "2",
    );
    let unchanged = fixture.graph();
    let no_op = ok(
        root,
        &[
            "plan",
            "rewire-compensation",
            "--id",
            PLAN,
            "--actor",
            "coordinator",
            "--expected-revision",
            "2",
        ],
    );
    assert!(no_op["data"]["operation"].is_null());
    assert_eq!(fixture.graph(), unchanged);
    operation = advance(&fixture, &operation, RejectionStage::Rewired, "2");
    ok(
        root,
        &[
            "plan",
            "release",
            "--id",
            &cp,
            "--actor",
            "coordinator",
            "--expected-revision",
            "2",
        ],
    );
    operation = advance(&fixture, &operation, RejectionStage::Released, "3");
    operation = advance(&fixture, &operation, RejectionStage::Complete, "3");
    assert_eq!(
        operation["operation_id"],
        begin["data"]["operation"]["operation_id"]
    );
    let graph = fixture.graph();
    let wave = ok(root, &["graph", "wave"]);
    assert_eq!(wave["data"]["wave"], json!([cp]));
    let shown = ok(root, &["plan", "show", "--id", PLAN]);
    let authority = &shown["data"]["plan"]["effective_authority"];
    assert!(authority["original"]["plan"]["body_sha256"].is_null());
    assert_eq!(
        authority["plan"]["body_sha256"],
        operation["plan"]["body_sha256"]
    );
    assert!(authority["effective"].is_null());
    assert!(authority["policy"].is_null());
    assert_eq!(fixture.graph(), graph);
    let prepare = uuid::Uuid::new_v4().to_string();
    let root_before = git(root, &["rev-parse", "HEAD"]);
    let gitfile_before = fs::read(target.join(".git")).unwrap();
    let prepared = ok(root, &preparation_args(&cp, &prepare, "3", "coordinator"));
    assert_ne!(root_before, reviewed);
    assert_eq!(
        prepared["data"]["plan"]["generation"]["prepared_branch_commit"],
        reviewed
    );
    assert_eq!(
        prepared["data"]["plan"]["generation"]["observed_root_commit"],
        root_before
    );
    assert_eq!(
        prepared["data"]["plan"]["generation"]["worktree"],
        format!(".mine/worktrees/{PLAN}")
    );
    assert_eq!(fs::read(target.join(".git")).unwrap(), gitfile_before);
    assert_eq!(
        git(&target, &["branch", "--show-current"]),
        format!("plan/{cp}-fixture")
    );
    assert!(!target.join(&cp_path).exists());
    ok(
        root,
        &[
            "plan",
            "start",
            "--id",
            &cp,
            "--owner",
            "comp-executor",
            "--run-id",
            "comp-run",
            "--expected-revision",
            "4",
        ],
    );
    assert!(!target.join(&cp_path).exists());
    // The executor, not the native binary, adds only its exact frozen body
    // after the completed start; no Root graph or product file is imported.
    let frozen = fs::read(root.join(&cp_path)).unwrap();
    use std::io::Write;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target.join(&cp_path))
        .unwrap();
    output.write_all(&frozen).unwrap();
    output.sync_all().unwrap();
    drop(output);
    git(&target, &["add", &cp_path]);
    git(
        &target,
        &[
            "commit",
            "-m",
            "executor materializes exact registered compensation body",
        ],
    );
    assert_eq!(git(&target, &["rev-parse", "HEAD^"]), reviewed);
    assert_eq!(
        git(
            &target,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        cp_path
    );
    let active = uuid::Uuid::new_v4().to_string();
    let reused = ok(root, &preparation_args(&cp, &active, "5", "reviewer"));
    assert_eq!(reused["data"]["plan"]["action"], "reused");
    let after: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    assert!(after.get(PLAN).unwrap().body_sha256.is_none());
    assert_eq!(after.get(&cp).unwrap().branch_anchor, reviewed);
    assert_eq!(
        after.get(&cp).unwrap().started_preparation_generation,
        Some(1)
    );
    assert_eq!(after.revision, 5);
    assert_eq!(git(root, &["rev-parse", &base_branch]), reviewed);
    assert_eq!(git(root, &["branch", "--show-current"]), "dev");
    // A restarted rejected writer must invalidate the stopped handoff, even
    // when its old report and immutable Plan bytes still happen to match.
    git(
        root,
        &[
            "update-ref",
            &format!("refs/heads/{base_branch}"),
            "HEAD",
            &reviewed,
        ],
    );
    assert_ne!(cli(root, &["graph", "wave"]).0, 0);
}

#[test]
fn actual_cli_legacy_current_handoff_observation_preserves_graph_and_recovers_already_switched_checkout()
 {
    use mine::domain::lifecycle_operation::{OperationStage, RejectionStage};
    for switched in [false, true] {
        let (fixture, target, cp, reviewed, begin_input, historical) =
            legacy_current_handoff_fixture(switched);
        let root = fixture.root();
        let original_graph = fixture.graph();
        let original_head = git(root, &["rev-parse", "HEAD"]);
        let original_gitfile = fs::read(target.join(".git")).unwrap();
        let expected_branch = git(&target, &["branch", "--show-current"]);
        let report = begin_input["intent"]["review"]["report"]["path"]
            .as_str()
            .unwrap();
        assert!(
            fs::read(root.join(report))
                .unwrap()
                .starts_with(&historical)
        );
        let refused_id = uuid::Uuid::new_v4().to_string();
        let mut refused = preparation_args(&cp, &refused_id, "0", "coordinator");
        if switched {
            refused.push("--recover-preparation");
        }
        assert_ne!(cli(root, &refused).0, 0);
        assert_eq!(fixture.graph(), original_graph);
        assert_eq!(git(&target, &["branch", "--show-current"]), expected_branch);
        assert_eq!(git(root, &["rev-parse", "HEAD"]), original_head);
        fs::create_dir_all(root.join(".mine/runtime")).unwrap();
        fs::write(
            root.join(CLOSURE_INPUT),
            serde_json::to_vec(&begin_input).unwrap(),
        )
        .unwrap();
        let (code, begun) = fixture.begin_closure("0");
        assert_eq!(code, 0, "switched={switched}: {begun}");
        let mut operation = begun["data"]["operation"].clone();
        let id = operation["operation_id"].clone();
        // A lost begin response is recovered by querying the existing identity,
        // not by inventing another current-observation operation.
        let queried = ok(
            root,
            &["operation", "show", "--operation-id", id.as_str().unwrap()],
        );
        assert_eq!(queried["data"]["operation"], operation);
        assert_eq!(operation["observed_graph_revision"], 0);
        assert_eq!(operation["actor"], "reviewer");
        assert_eq!(fixture.graph(), original_graph);
        assert_eq!(git(root, &["rev-parse", "HEAD"]), original_head);
        for stage in [
            RejectionStage::Rejected,
            RejectionStage::CompensationRegistered,
            RejectionStage::Rewired,
            RejectionStage::Released,
            RejectionStage::Complete,
        ] {
            let expected = operation["stage_revision"].as_u64().unwrap();
            operation["stage_revision"] = json!(expected + 1);
            operation["stage"] = json!(OperationStage::RejectedHandoff(stage));
            if stage == RejectionStage::Complete {
                operation["outcome"] = json!("committed");
            }
            fs::write(
                root.join(CLOSURE_INPUT),
                serde_json::to_vec(&json!({"schema_version":1,
                "expected_stage_revision":expected,"operation":operation,"verification":null}))
                .unwrap(),
            )
            .unwrap();
            let response = ok(
                root,
                &[
                    "operation",
                    if stage == RejectionStage::Complete {
                        "finalize"
                    } else {
                        "stage"
                    },
                    "--expected-revision",
                    "0",
                    "--input",
                    CLOSURE_INPUT,
                ],
            );
            operation = response["data"]["operation"].clone();
            assert_eq!(operation["operation_id"], id);
            assert_eq!(fixture.graph(), original_graph);
            assert_eq!(git(root, &["rev-parse", "HEAD"]), original_head);
            assert_eq!(git(&target, &["rev-parse", "HEAD"]), reviewed);
            assert_eq!(git(&target, &["branch", "--show-current"]), expected_branch);
            assert_eq!(fs::read(target.join(".git")).unwrap(), original_gitfile);
        }
        let current_stage = operation["stage_revision"].as_u64().unwrap();
        fs::write(
            root.join(CLOSURE_INPUT),
            serde_json::to_vec(&json!({"schema_version":1,
            "expected_stage_revision":current_stage,"operation":operation,"verification":null}))
            .unwrap(),
        )
        .unwrap();
        let replay = ok(
            root,
            &[
                "operation",
                "finalize",
                "--expected-revision",
                "0",
                "--input",
                CLOSURE_INPUT,
            ],
        );
        assert_eq!(replay["data"]["replay"], true);
        assert!(replay["data"]["checkpoint_operation_id"].is_null());
        fs::write(
            root.join(CLOSURE_INPUT),
            serde_json::to_vec(&begin_input).unwrap(),
        )
        .unwrap();
        assert_ne!(
            fixture.begin_closure("0").0,
            0,
            "new UUID must not replace current/historical node operation"
        );
        assert_eq!(fixture.graph(), original_graph);
        assert_eq!(git(root, &["rev-parse", "HEAD"]), original_head);
        let preparation = uuid::Uuid::new_v4().to_string();
        let mut args = preparation_args(&cp, &preparation, "0", "coordinator");
        if switched {
            assert_ne!(
                cli(root, &args).0,
                0,
                "legacy existing branch requires explicit recovery"
            );
            assert_eq!(fixture.graph(), original_graph);
            args.push("--recover-preparation");
        }
        let prepared = ok(root, &args);
        assert_eq!(
            prepared["data"]["plan"]["generation"]["prepared_branch_commit"],
            reviewed
        );
        assert_eq!(
            prepared["data"]["plan"]["generation"]["observed_root_commit"],
            original_head
        );
        assert_eq!(fs::read(target.join(".git")).unwrap(), original_gitfile);
        ok(
            root,
            &[
                "plan",
                "start",
                "--id",
                &cp,
                "--owner",
                "comp-executor",
                "--run-id",
                "current-observation",
                "--expected-revision",
                "1",
            ],
        );
        let graph: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(graph.revision, 2);
        assert!(graph.get(PLAN).unwrap().body_sha256.is_none());
        assert!(graph.get(&cp).unwrap().body_sha256.is_none());
        assert_eq!(graph.get(&cp).unwrap().branch_anchor, reviewed);
        assert_eq!(
            graph.get(&cp).unwrap().started_preparation_generation,
            Some(1)
        );
        assert_eq!(
            git(root, &["rev-parse", &format!("plan/{PLAN}-fixture")]),
            reviewed
        );
        assert!(
            fs::read(root.join(report))
                .unwrap()
                .starts_with(&historical)
        );
        assert!(!target.join(report).exists());
        assert!(!target.join(format!(".mine/plan/{cp}-fixture.md")).exists());
    }
}

#[test]
fn actual_cli_legacy_current_handoff_refuses_unbound_evidence_without_creating_intent() {
    for failure in 0..12 {
        let (fixture, target, cp, reviewed, mut input, historical) =
            legacy_current_handoff_fixture(false);
        let root = fixture.root();
        match failure {
            0 | 1 | 11 => {
                let mut graph: PlanWorkspace =
                    toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
                if failure == 11 {
                    graph.schema_version = 5;
                }
                let node = graph.get_mut(PLAN).unwrap();
                match failure {
                    0 => node.owner.clear(),
                    1 => node.run_id.clear(),
                    11 => node.body_sha256 = Some(digest(b"unrelated historical body")),
                    _ => unreachable!(),
                }
                fs::write(
                    root.join(".mine/plan/execution-graph.toml"),
                    toml::to_string(&graph).unwrap(),
                )
                .unwrap();
                fs::write(
                    root.join(".mine/plan/execution-graph.md"),
                    render_markdown(&graph).unwrap(),
                )
                .unwrap();
                git(
                    root,
                    &[
                        "add",
                        ".mine/plan/execution-graph.toml",
                        ".mine/plan/execution-graph.md",
                    ],
                );
                git(
                    root,
                    &["commit", "-m", "adversarial legacy original identity"],
                );
            }
            2 => {
                input["intent"]["compensation"]["body_sha256"] =
                    json!(digest(b"wrong compensation"))
            }
            3 => input["intent"]["compensation"]["plan_path"] = json!(".mine/plan/foreign.md"),
            4 => input["intent"]["review"]["report"]["sha256"] = json!(digest(b"wrong report")),
            5 => {
                input["intent"]["review"]["reviewed_tip"] = json!(git(root, &["rev-parse", "HEAD"]))
            }
            6 => fs::write(
                target.join("untracked-writer-output.txt"),
                b"preserve writer output\n",
            )
            .unwrap(),
            7 => {
                git(root, &["branch", "foreign-same-tip", &reviewed]);
                git(&target, &["switch", "--no-guess", "foreign-same-tip"]);
            }
            8 => {
                let report = input["intent"]["review"]["report"]["path"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                fs::write(root.join(&report), &historical).unwrap();
                git(root, &["add", &report]);
                git(
                    root,
                    &["commit", "-m", "legacy report without modern stop evidence"],
                );
                input["intent"]["review"]["report"]["bytes"] = json!(historical.len());
                input["intent"]["review"]["report"]["sha256"] = json!(digest(&historical));
            }
            9 => {
                fs::write(target.join("later-writer.txt"), b"writer resumed\n").unwrap();
                git(&target, &["add", "later-writer.txt"]);
                git(&target, &["commit", "-m", "writer advanced after review"]);
            }
            10 => {}
            _ => unreachable!(),
        }
        fs::create_dir_all(root.join(".mine/runtime")).unwrap();
        fs::write(
            root.join(CLOSURE_INPUT),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        let graph = fixture.graph();
        let head = git(root, &["rev-parse", "HEAD"]);
        let source_tip = git(&target, &["rev-parse", "HEAD"]);
        let source_branch = git(&target, &["branch", "--show-current"]);
        let gitfile = fs::read(target.join(".git")).unwrap();
        let source_ref = git(root, &["rev-parse", &format!("plan/{PLAN}-fixture")]);
        let catalog = root.join(".mine/runtime/operations/.catalog");
        let before_catalog = fs::read(&catalog).ok();
        let (code, response) = fixture.begin_closure(if failure == 10 { "1" } else { "0" });
        assert_ne!(code, 0, "failure={failure}: {response}");
        if failure == 10 {
            assert_eq!(response["error"]["code"], "MINE_REVISION_CONFLICT");
        }
        assert_eq!(fixture.graph(), graph, "failure={failure}");
        assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(&target, &["rev-parse", "HEAD"]), source_tip);
        assert_eq!(git(&target, &["branch", "--show-current"]), source_branch);
        assert_eq!(fs::read(target.join(".git")).unwrap(), gitfile);
        assert_eq!(
            git(root, &["rev-parse", &format!("plan/{PLAN}-fixture")]),
            source_ref
        );
        assert_eq!(fs::read(&catalog).ok(), before_catalog);
        let journal_dir = root.join(".mine/runtime/operations");
        if journal_dir.exists() {
            assert!(
                !fs::read_dir(&journal_dir).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "json")
                }),
                "failure={failure}: refusal created a new intent"
            );
        }
        assert!(!target.join(format!(".mine/plan/{cp}-fixture.md")).exists());
        if failure == 6 {
            assert_eq!(
                fs::read(target.join("untracked-writer-output.txt")).unwrap(),
                b"preserve writer output\n"
            );
        }
    }
}

#[test]
fn actual_cli_concurrent_starts_grant_once_and_reobserve_disjoint_ownership() {
    use mine::domain::status::PlanStatus;
    use std::process::Stdio;
    for disjoint in [false, true] {
        let fixture = Fixture::new(None, true);
        fixture.seed_legacy_node(false);
        let root = fixture.root();
        // Explicit imported legacy READY fixture, never the live bootstrap graph.
        // mine-release-allow-plan-reference: isolated concurrent-start fixture
        let second_id = "plan82";
        // mine-release-allow-plan-reference: isolated concurrent-start fixture body
        let second_path = ".mine/plan/plan82-fixture.md";
        let mut graph: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        let mut second = graph.get(PLAN).unwrap().clone();
        second.id = second_id.into();
        second.path = second_path.into();
        second.branch = format!("plan/{second_id}-fixture");
        second.lineage_id = second_id.into();
        second.worktree_path = format!(".mine/worktrees/{second_id}");
        if disjoint {
            second.exclusive_write_paths = vec!["src/other".into()];
        }
        graph.plans.push(second);
        fs::write(
            root.join(second_path),
            b"# Second independently prepared Plan\n",
        )
        .unwrap();
        fs::write(
            root.join(".mine/plan/execution-graph.toml"),
            toml::to_string(&graph).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(".mine/plan/execution-graph.md"),
            render_markdown(&graph).unwrap(),
        )
        .unwrap();
        git(
            root,
            &[
                "add",
                second_path,
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(
            root,
            &["commit", "-m", "explicit two-Plan concurrency fixture"],
        );
        for (id, revision) in [(PLAN, "0"), (second_id, "1")] {
            let preparation = uuid::Uuid::new_v4().to_string();
            ok(
                root,
                &preparation_args(id, &preparation, revision, "coordinator"),
            );
        }
        let before = git(root, &["rev-parse", "HEAD"]);
        let spawn = |id: &str, owner: &str| {
            Command::new(env!("CARGO_BIN_EXE_mine"))
                .current_dir(root)
                .args([
                    "--format",
                    "json",
                    "plan",
                    "start",
                    "--id",
                    id,
                    "--expected-revision",
                    "2",
                    "--owner",
                    owner,
                    "--run-id",
                    "concurrent-fixture",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        };
        let first = spawn(PLAN, "first-executor");
        let second = spawn(second_id, "second-executor");
        let results = [
            first.wait_with_output().unwrap(),
            second.wait_with_output().unwrap(),
        ];
        assert_eq!(
            results
                .iter()
                .filter(|output| output.status.success())
                .count(),
            1
        );
        let winner = if results[0].status.success() {
            PLAN
        } else {
            second_id
        };
        let loser = if winner == PLAN { second_id } else { PLAN };
        let loser_output = if winner == PLAN {
            &results[1]
        } else {
            &results[0]
        };
        let error: Value = serde_json::from_slice(&loser_output.stderr).unwrap();
        // A contender can exhaust the existing bounded lock wait before it
        // obtains a snapshot. Neither that refusal nor a stale snapshot grants
        // execution. Do not change the product's timeout to time this test.
        assert!(
            matches!(
                error["error"]["code"].as_str(),
                Some("MINE_LOCK_TIMEOUT" | "MINE_REVISION_CONFLICT")
            ),
            "unexpected concurrent refusal: {error}"
        );
        let started: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(started.revision, 3);
        assert_eq!(started.get(winner).unwrap().status, PlanStatus::InProgress);
        assert_eq!(started.get(loser).unwrap().status, PlanStatus::Ready);
        assert_eq!(
            git(root, &["rev-list", "--count", &format!("{before}..HEAD")]),
            "1"
        );
        let node = started.get(winner).unwrap();
        assert_eq!(node.started_preparation_generation, Some(1));
        assert_eq!(
            node.branch_anchor,
            node.preparations[0].prepared_branch_commit.as_str()
        );
        let original = fixture.graph();
        let root_tip = git(root, &["rev-parse", "HEAD"]);
        let (stale_code, stale) = cli(
            root,
            &[
                "plan",
                "start",
                "--id",
                loser,
                "--expected-revision",
                "2",
                "--owner",
                "stale-executor",
                "--run-id",
                "after-winning-checkpoint",
            ],
        );
        assert_ne!(stale_code, 0, "{stale}");
        assert_eq!(stale["error"]["code"], "MINE_REVISION_CONFLICT");
        assert_eq!(fixture.graph(), original);
        assert_eq!(git(root, &["rev-parse", "HEAD"]), root_tip);
        let (code, response) = cli(
            root,
            &[
                "plan",
                "start",
                "--id",
                loser,
                "--expected-revision",
                "3",
                "--owner",
                "rereading-executor",
                "--run-id",
                "after-observer-reread",
            ],
        );
        if disjoint {
            assert_eq!(code, 0, "{response}");
            let joined: PlanWorkspace =
                toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
            assert_eq!(joined.revision, 4);
            assert!(
                joined
                    .plans
                    .iter()
                    .all(|node| node.status == PlanStatus::InProgress)
            );
            assert_eq!(joined.get(winner), started.get(winner));
            assert_eq!(
                joined.get(loser).unwrap().started_preparation_generation,
                Some(1)
            );
            assert_eq!(
                git(root, &["rev-list", "--count", &format!("{before}..HEAD")]),
                "2"
            );
        } else {
            assert_ne!(
                code, 0,
                "overlapping held ownership must refuse: {response}"
            );
            assert_eq!(fixture.graph(), original);
            assert_eq!(git(root, &["rev-parse", "HEAD"]), root_tip);
        }
        assert_eq!(git(root, &["branch", "--show-current"]), "dev");
        assert_eq!(git(root, &["status", "--porcelain"]), "");
        for node in &started.plans {
            let target = root.join(&node.worktree_path);
            assert_eq!(git(&target, &["branch", "--show-current"]), node.branch);
            assert_eq!(
                git(&target, &["rev-parse", "HEAD"]),
                node.preparations[0].prepared_branch_commit.as_str()
            );
            assert_eq!(git(&target, &["status", "--porcelain"]), "");
        }
    }
}

#[test]
fn actual_cli_ordinary_write_retains_actor_generated_id_and_recovers_same_checkpoint() {
    let fixture = Fixture::new(None, true);
    let root = fixture.root();
    let before = git(root, &["rev-parse", "HEAD"]);
    let hook = root.join(".git/hooks/reference-transaction");
    fs::write(
        &hook,
        b"#!/bin/sh\nif [ \"$1\" = prepared ]; then exit 1; fi\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (code, response) = fixture.register("0");
    assert_eq!(code, 7, "{response}");
    assert_eq!(response["error"]["code"], "MINE_GRAPH_CHECKPOINT_PENDING");
    let operation = response["error"]["details"]["operation"].clone();
    assert_eq!(operation["actor"], "fixture-registrar");
    assert_eq!(operation["observed_graph_revision"], 0);
    let id = operation["operation_id"].as_str().unwrap();
    let queried = ok(root, &["operation", "show", "--operation-id", id]);
    assert_eq!(queried["data"]["operation"], operation);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), before);
    let graph_before_resume = fixture.graph();
    let current: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&graph_before_resume).unwrap()).unwrap();
    assert_eq!(current.revision, 1);
    assert!(current.get(PLAN).is_some());
    // Remove only this fixture's own injected hook, not a product/worktree path.
    fs::remove_file(hook).unwrap();
    let resume_path = ".mine/runtime/ordinary-finalize.json";
    fs::write(root.join(resume_path),serde_json::to_vec(&json!({"schema_version":1,
        "expected_stage_revision":operation["stage_revision"],"operation":operation,"verification":null})).unwrap()).unwrap();
    let resumed = ok(
        root,
        &[
            "operation",
            "finalize",
            "--expected-revision",
            "1",
            "--input",
            resume_path,
        ],
    );
    assert_eq!(resumed["data"]["checkpoint_operation_id"], id);
    assert_eq!(resumed["data"]["operation"]["actor"], "fixture-registrar");
    assert_eq!(resumed["revision_before"], 1);
    assert_eq!(resumed["revision_after"], 1);
    assert_eq!(fixture.graph(), graph_before_resume);
    assert_eq!(
        git(root, &["rev-list", "--count", &format!("{before}..HEAD")]),
        "1"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "");
    let completed = resumed["data"]["operation"].clone();
    fs::write(root.join(resume_path),serde_json::to_vec(&json!({"schema_version":1,
        "expected_stage_revision":completed["stage_revision"],"operation":completed,"verification":null})).unwrap()).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    let replay = ok(
        root,
        &[
            "operation",
            "finalize",
            "--expected-revision",
            "1",
            "--input",
            resume_path,
        ],
    );
    assert_eq!(replay["data"]["replay"], true);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.graph(), graph_before_resume);
}

#[test]
fn ordinary_write_refuses_root_movement_after_business_validation_without_overwriting_it() {
    use mine::infrastructure::toml_store::TomlStore;
    let (fixture, _) = withdrawal_subject(0);
    let root = fixture.root();
    let graph = fixture.graph();
    let original = git(root, &["rev-parse", "HEAD"]);
    let store = TomlStore::new(root);
    let error = store
        .save_with_revision(0, "fixture-actor", |mut workspace| {
            // Deterministic noncooperating Git writer at the callback boundary.
            fs::write(
                root.join("foreign-product.txt"),
                "preserve external commit\n",
            )
            .unwrap();
            git(root, &["add", "foreign-product.txt"]);
            git(root, &["commit", "-m", "external Root movement fixture"]);
            workspace.revision = 1;
            Ok(workspace)
        })
        .unwrap_err();
    assert!(error.to_string().contains("Root moved"), "{error}");
    let foreign = git(root, &["rev-parse", "HEAD"]);
    assert_ne!(foreign, original);
    assert_eq!(
        git(root, &["rev-list", "--count", &format!("{original}..HEAD")]),
        "1"
    );
    assert_eq!(fixture.graph(), graph);
    assert_eq!(
        fs::read(root.join("foreign-product.txt")).unwrap(),
        b"preserve external commit\n"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "");
    assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn actual_cli_native_prepare_requires_actor_and_refuses_foreign_symbolic_refs() {
    for actor in [None, Some("   ")] {
        let (fixture, target) = withdrawal_subject(0);
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let graph = fixture.graph();
        let id = uuid::Uuid::new_v4().to_string();
        let mut args = preparation_args(PLAN, &id, "0", actor.unwrap_or("unused"));
        if actor.is_none() {
            args.truncate(args.len() - 2);
        }
        assert_ne!(cli(fixture.root(), &args).0, 0);
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), original);
        assert!(!target.exists());
    }
    for dangling in [false, true] {
        let (fixture, target) = withdrawal_subject(0);
        let reference = format!("refs/heads/plan/{PLAN}-fixture");
        let destination = if dangling {
            "refs/heads/missing-fixture"
        } else {
            "refs/heads/dev"
        };
        git(fixture.root(), &["symbolic-ref", &reference, destination]);
        let graph = fixture.graph();
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let id = uuid::Uuid::new_v4().to_string();
        assert_ne!(
            cli(
                fixture.root(),
                &preparation_args(PLAN, &id, "0", "coordinator")
            )
            .0,
            0
        );
        assert_eq!(
            git(fixture.root(), &["symbolic-ref", &reference]),
            destination
        );
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), original);
        assert!(!target.exists());
    }
}

#[test]
fn actual_cli_native_prepare_two_plans_reuse_after_checkpoint_churn_and_refuse_ready_advancement() {
    let (fixture, target) = withdrawal_subject(0);
    // mine-release-allow-plan-reference: second exact registered isolated preparation subject
    let second = "plan82";
    let second_path = format!(".mine/plan/{second}-fixture.md");
    fs::write(
        fixture.root().join(&second_path),
        "# Second immutable Plan\n",
    )
    .unwrap();
    let mut workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let mut node = workspace.get(PLAN).unwrap().clone();
    node.id = second.into();
    node.path = second_path.clone();
    node.branch = format!("plan/{second}-fixture");
    node.lineage_id = second.into();
    node.worktree_path = format!(".mine/worktrees/{second}");
    node.body_sha256 = Some(digest(
        &fs::read(fixture.root().join(&second_path)).unwrap(),
    ));
    node.exclusive_write_paths = vec!["src/second".into()];
    workspace.plans.push(node);
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.md"),
        render_markdown(&workspace).unwrap(),
    )
    .unwrap();
    git(fixture.root(), &["add", ".mine/plan"]);
    git(
        fixture.root(),
        &["commit", "-m", "explicit two-Plan fixture registration"],
    );
    let first = uuid::Uuid::new_v4().to_string();
    let prepared = ok(
        fixture.root(),
        &preparation_args(PLAN, &first, "0", "coordinator"),
    );
    let generation = prepared["data"]["plan"]["generation"].clone();
    let original = git(&target, &["rev-parse", "HEAD"]);
    let next = uuid::Uuid::new_v4().to_string();
    ok(
        fixture.root(),
        &preparation_args(second, &next, "1", "coordinator"),
    );
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let graph = fixture.graph();
    let reuse_id = uuid::Uuid::new_v4().to_string();
    let reused = ok(
        fixture.root(),
        &preparation_args(PLAN, &reuse_id, "2", "reviewer"),
    );
    assert_eq!(reused["data"]["plan"]["generation"], generation);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), original);
    fs::write(target.join("premature.txt"), "not permitted while READY\n").unwrap();
    git(&target, &["add", "premature.txt"]);
    git(
        &target,
        &["commit", "-m", "premature READY advancement fixture"],
    );
    let advanced = git(&target, &["rev-parse", "HEAD"]);
    let rejected_id = uuid::Uuid::new_v4().to_string();
    assert_ne!(
        cli(
            fixture.root(),
            &preparation_args(PLAN, &rejected_id, "2", "reviewer")
        )
        .0,
        0
    );
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), advanced);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn actual_cli_native_prepare_rejects_intermediate_product_change_and_supports_explicit_refresh() {
    let (fixture, target) = withdrawal_subject(0);
    let id = uuid::Uuid::new_v4().to_string();
    ok(
        fixture.root(),
        &preparation_args(PLAN, &id, "0", "coordinator"),
    );
    let original_tip = git(&target, &["rev-parse", "HEAD"]);
    fs::write(
        fixture.root().join("product.txt"),
        "intermediate product change\n",
    )
    .unwrap();
    git(fixture.root(), &["add", "product.txt"]);
    git(fixture.root(), &["commit", "-m", "product change"]);
    git(fixture.root(), &["revert", "--no-edit", "HEAD"]);
    let refresh_base = git(fixture.root(), &["rev-parse", "HEAD"]);
    let graph = fixture.graph();
    let next = uuid::Uuid::new_v4().to_string();
    let args = preparation_args(PLAN, &next, "1", "coordinator");
    assert_ne!(cli(fixture.root(), &args).0, 0);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), original_tip);
    let mut refresh = args;
    refresh.push("--reprepare");
    let response = ok(fixture.root(), &refresh);
    assert_eq!(response["data"]["plan"]["action"], "reprepared");
    assert_eq!(response["revision_after"], 2);
    assert_eq!(response["data"]["plan"]["generation"]["ordinal"], 2);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), refresh_base);
    let graph: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    assert_eq!(graph.get(PLAN).unwrap().preparations.len(), 2);
    assert_eq!(
        graph.get(PLAN).unwrap().status,
        mine::domain::status::PlanStatus::Ready
    );
    assert!(graph.get(PLAN).unwrap().branch_anchor.is_empty());
}

#[test]
fn actual_cli_native_prepare_legacy_recovery_preserves_original_absent_body_identity() {
    let fixture = Fixture::new(None, true);
    fixture.seed_legacy_node(false);
    let branch = format!("plan/{PLAN}-fixture");
    let target = fixture.root().join(format!(".mine/worktrees/{PLAN}"));
    let original = git(fixture.root(), &["rev-parse", "HEAD"]);
    git(
        fixture.root(),
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            target.to_str().unwrap(),
            &original,
        ],
    );
    let id = uuid::Uuid::new_v4().to_string();
    let mut args = preparation_args(PLAN, &id, "0", "coordinator");
    assert_ne!(cli(fixture.root(), &args).0, 0);
    args.push("--recover-preparation");
    let response = ok(fixture.root(), &args);
    assert_eq!(response["data"]["plan"]["action"], "recovered_preparation");
    let graph: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let node = graph.get(PLAN).unwrap();
    assert!(node.body_sha256.is_none());
    assert!(node.preparations[0].plan.fields().body_sha256.is_some());
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), original);
}

#[test]
fn actual_cli_native_prepare_unproven_legacy_compensation_is_preserved_without_switch_or_intent() {
    for prepared in [false, true] {
        let (fixture, target, cp, preserved) = compensation_withdrawal_subject(prepared);
        let root_before = git(fixture.root(), &["rev-parse", "HEAD"]);
        let branch_before = git(&target, &["branch", "--show-current"]);
        let graph = fixture.graph();
        let gitfile = fs::read(target.join(".git")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let (code, response) = cli(
            fixture.root(),
            &preparation_args(&cp, &id, "0", "coordinator"),
        );
        assert_ne!(code, 0, "unproven legacy stop admitted: {response}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), root_before);
        assert_eq!(git(&target, &["branch", "--show-current"]), branch_before);
        assert_eq!(git(&target, &["rev-parse", "HEAD"]), preserved);
        assert_eq!(
            git(
                fixture.root(),
                &["rev-parse", &format!("plan/{PLAN}-fixture")]
            ),
            preserved
        );
        assert_eq!(fs::read(target.join(".git")).unwrap(), gitfile);
        assert!(!target.join(format!(".mine/plan/{cp}-fixture.md")).exists());
        let location = mine::infrastructure::operation_store::OperationLocation::new(
            fixture.root(),
            &mine::domain::lifecycle_operation::OperationId::parse(&id).unwrap(),
        )
        .unwrap()
        .resolve()
        .unwrap();
        assert!(!location.exists());
    }
}

#[test]
fn actual_cli_native_prepare_recovers_recorded_effects_and_refuses_unrecorded_attach() {
    use mine::{
        application::{
            graph_service::GraphService, native_plan_workspace::NativePlanWorkspace,
            plan_service::PlanPrepareRequest,
        },
        domain::lifecycle_operation::*,
        infrastructure::toml_store::TomlStore,
    };
    struct Interrupt {
        boundary: Boundary,
        occurrence: usize,
        seen: usize,
    }
    impl BoundaryObserver for Interrupt {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == self.boundary {
                self.seen += 1;
                if self.seen == self.occurrence {
                    return Err(OperationError::Interrupted(boundary));
                }
            }
            Ok(())
        }
    }
    for (boundary, occurrence, ambiguous_attach) in [
        (Boundary::Intent, 1, false),
        (Boundary::PlanGitAttempted, 1, false),
        (Boundary::PlanGitReturned, 1, false),
        (Boundary::PlanGitRecorded, 1, false),
        (Boundary::PlanGitAttempted, 2, false),
        (Boundary::PlanGitReturned, 2, true),
        (Boundary::PlanCheckoutObserved, 1, false),
        (Boundary::PlanGitRecorded, 2, false),
        (Boundary::GraphWritten, 1, false),
        (Boundary::ViewWritten, 1, false),
        (Boundary::CasReturned, 1, false),
        (Boundary::IndexPublished, 1, false),
        (Boundary::Complete, 1, false),
        (Boundary::Response, 1, false),
    ] {
        let (fixture, target) = withdrawal_subject(0);
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let id = uuid::Uuid::new_v4().to_string();
        let graph_before = fixture.graph();
        let store = TomlStore::new(fixture.root());
        let graph = GraphService::new(&store);
        let request = PlanPrepareRequest {
            id: PLAN.into(),
            expected_revision: 0,
            operation_id: OperationId::parse(&id).unwrap(),
            actor: "coordinator".into(),
            mode: PreparationMode::Initial,
        };
        let error = NativePlanWorkspace::new(&graph)
            .prepare(
                request,
                fixture.root(),
                &mut Interrupt {
                    boundary,
                    occurrence,
                    seen: 0,
                },
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("injected interruption"),
            "{boundary:?}/{occurrence}: {error}"
        );
        let before_resume = fixture.graph();
        let head_before_resume = git(fixture.root(), &["rev-parse", "HEAD"]);
        let args = preparation_args(PLAN, &id, "0", "coordinator");
        let (code, response) = cli(fixture.root(), &args);
        if ambiguous_attach {
            assert_ne!(
                code, 0,
                "unrecorded admin identity must not be adopted: {response}"
            );
            assert_eq!(before_resume, graph_before);
            assert_eq!(fixture.graph(), before_resume);
            assert_eq!(
                git(fixture.root(), &["rev-parse", "HEAD"]),
                head_before_resume
            );
            assert!(target.exists());
            let query = ok(
                fixture.root(),
                &["operation", "show", "--operation-id", &id],
            );
            assert_eq!(query["data"]["operation"]["outcome"], "pending");
            continue;
        }
        assert_eq!(code, 0, "{boundary:?}/{occurrence}: {response}");
        assert_eq!(response["data"]["plan"]["generation"]["operation_id"], id);
        assert_eq!(git(&target, &["rev-parse", "HEAD"]), original);
        assert_eq!(
            git(
                fixture.root(),
                &["rev-list", "--count", &format!("{original}..HEAD")]
            ),
            "1"
        );
        let current: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(current.revision, 1);
        assert_eq!(current.get(PLAN).unwrap().preparations.len(), 1);
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let replay = ok(fixture.root(), &args);
        assert_eq!(replay["data"]["plan"]["replay"], true);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        for suffix in ["index", "lock-witness"] {
            assert!(
                !fixture
                    .root()
                    .join(format!(".mine/runtime/operations/{id}.{suffix}"))
                    .exists()
            );
        }
        assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    }
}

#[test]
fn actual_cli_native_withdrawal_no_artifacts_branch_only_and_checkout() {
    for artifacts in 0..3 {
        let (fixture, target) = withdrawal_subject(artifacts);
        let branch = format!("plan/{PLAN}-fixture");
        let setting = format!("branch.{branch}.description");
        git(
            fixture.root(),
            &["config", &setting, "preserve branch configuration"],
        );
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let stable = git(fixture.root(), &["rev-parse", "master"]);
        let id = uuid::Uuid::new_v4().to_string();
        let args = withdrawal_args(&id);
        let (code, value) = cli(fixture.root(), &args);
        assert_eq!(code, 0, "{value}");
        assert_eq!(value["revision_before"], 0);
        assert_eq!(value["revision_after"], 1);
        assert_eq!(value["data"]["cleaned_git_artifacts"], artifacts > 0);
        assert_eq!(value["data"]["replay"], false);
        assert!(!target.exists());
        assert!(
            git(
                fixture.root(),
                &[
                    "for-each-ref",
                    "--format=%(refname)",
                    &format!("refs/heads/{branch}")
                ]
            )
            .is_empty()
        );
        assert_eq!(
            git(fixture.root(), &["config", "--get", &setting]),
            "preserve branch configuration"
        );
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD^"]), original);
        let delta = git(
            fixture.root(),
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                &original,
                &head,
            ],
        );
        assert_eq!(
            delta.lines().collect::<std::collections::BTreeSet<_>>(),
            [
                ".mine/plan/execution-graph.md",
                ".mine/plan/execution-graph.toml"
            ]
            .into_iter()
            .collect()
        );
        let graph = fixture.graph();
        let record = fs::read(
            fixture
                .root()
                .join(format!(".mine/runtime/operations/{id}.json")),
        )
        .unwrap();
        let stored: Value = serde_json::from_slice(&record).unwrap();
        assert_eq!(
            stored["checkpoint"]["plan_git"]["intent"]["original_generation"]["kind"],
            "absent"
        );
        assert_eq!(stored["checkpoint"]["plan_git"]["removal"], "complete");
        assert_eq!(stored["checkpoint"]["plan_git"]["deletion"], "complete");
        let (code, replay) = cli(fixture.root(), &args);
        assert_eq!(code, 0, "{replay}");
        assert_eq!(replay["data"]["replay"], true);
        assert_eq!(replay["revision_before"], 1);
        assert_eq!(replay["revision_after"], 1);
        assert_eq!(
            replay["data"]["withdrawn_at"],
            value["data"]["withdrawn_at"]
        );
        for (field, changed) in [("--actor", "foreign"), ("--reason", "different reason")] {
            let mut changed_args = args.clone();
            let position = changed_args.iter().position(|v| *v == field).unwrap() + 1;
            changed_args[position] = changed;
            assert_ne!(cli(fixture.root(), &changed_args).0, 0);
        }
        assert_eq!(fixture.graph(), graph);
        assert_eq!(
            fs::read(
                fixture
                    .root()
                    .join(format!(".mine/runtime/operations/{id}.json"))
            )
            .unwrap(),
            record
        );
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        let new_id = uuid::Uuid::new_v4().to_string();
        let mut new_args = withdrawal_args(&new_id);
        *new_args.last_mut().unwrap() = "1";
        let (code, refusal) = cli(fixture.root(), &new_args);
        assert_ne!(code, 0, "{refusal}");
        assert_eq!(refusal["error"]["code"], "MINE_INVALID_TRANSITION");
        assert!(
            !fixture
                .root()
                .join(format!(".mine/runtime/operations/{new_id}.json"))
                .exists()
        );
        fs::write(
            fixture.root().join("later-product.txt"),
            "legitimate later integration\n",
        )
        .unwrap();
        git(fixture.root(), &["add", "later-product.txt"]);
        git(
            fixture.root(),
            &["commit", "-m", "later ordinary product integration"],
        );
        let later = git(fixture.root(), &["rev-parse", "HEAD"]);
        let (code, replay) = cli(fixture.root(), &args);
        assert_eq!(code, 0, "{replay}");
        assert_eq!(replay["data"]["replay"], true);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), later);
        assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
        assert_eq!(git(fixture.root(), &["rev-parse", "master"]), stable);
    }
}

#[test]
fn actual_cli_native_withdrawal_recovers_each_git_effect_and_publication_boundary() {
    use mine::{
        application::{
            graph_service::GraphService, native_plan_workspace::NativePlanWorkspace,
            plan_service::PlanWithdrawRequest,
        },
        domain::lifecycle_operation::*,
        infrastructure::toml_store::TomlStore,
    };
    struct Interrupt {
        boundary: Boundary,
        occurrence: usize,
        seen: usize,
    }
    impl BoundaryObserver for Interrupt {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == self.boundary {
                self.seen += 1;
                if self.seen == self.occurrence {
                    return Err(OperationError::Interrupted(boundary));
                }
            }
            Ok(())
        }
    }
    for (boundary, occurrence) in [
        (Boundary::Intent, 1),
        (Boundary::PlanGitAttempted, 1),
        (Boundary::PlanGitReturned, 1),
        (Boundary::PlanGitRecorded, 1),
        (Boundary::PlanGitAttempted, 2),
        (Boundary::PlanGitReturned, 2),
        (Boundary::PlanGitRecorded, 2),
        (Boundary::GraphWritten, 1),
        (Boundary::GraphRecorded, 1),
        (Boundary::ViewWritten, 1),
        (Boundary::CasReturned, 1),
        (Boundary::IndexPublished, 1),
        (Boundary::Complete, 1),
        (Boundary::Response, 1),
    ] {
        let (fixture, _) = withdrawal_subject(2);
        let store = TomlStore::new(fixture.root());
        let graph = GraphService::new(&store);
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let id = uuid::Uuid::new_v4().to_string();
        let request = PlanWithdrawRequest {
            expected_revision: 0,
            id: PLAN.into(),
            reason: "retired fixture".into(),
            operation_id: OperationId::parse(&id).unwrap(),
            actor: "coordinator".into(),
        };
        let error = NativePlanWorkspace::new(&graph)
            .withdraw(
                request,
                &mut Interrupt {
                    boundary,
                    occurrence,
                    seen: 0,
                },
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("injected interruption"),
            "{boundary:?}/{occurrence}: {error}"
        );
        let query = ok(
            fixture.root(),
            &["operation", "show", "--operation-id", &id],
        );
        assert_eq!(query["data"]["operation"]["operation_id"], id);
        assert!(query["data"]["plan_git"]["step"].is_string());
        let (code, value) = cli(fixture.root(), &withdrawal_args(&id));
        assert_eq!(code, 0, "{boundary:?}/{occurrence}: {value}");
        assert_eq!(value["data"]["operation"]["operation_id"], id);
        for suffix in ["index", "lock-witness"] {
            assert!(
                !fixture
                    .root()
                    .join(format!(".mine/runtime/operations/{id}.{suffix}"))
                    .exists()
            );
        }
        assert_eq!(
            git(
                fixture.root(),
                &["rev-list", "--count", &format!("{original}..HEAD")]
            ),
            "1"
        );
        let node: PlanWorkspace =
            toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
        assert_eq!(node.revision, 1);
        assert_eq!(
            node.get(PLAN).unwrap().status,
            mine::domain::status::PlanStatus::Withdrawn
        );
        assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    }
}

#[test]
fn native_withdrawal_cancelled_request_retains_pending_and_recovers_same_id() {
    use mine::{
        application::{
            graph_service::GraphService, native_plan_workspace::NativePlanWorkspace,
            plan_service::PlanWithdrawRequest,
        },
        domain::{error::MineError, lifecycle_operation::*},
        infrastructure::{git_evidence::GitRequest, toml_store::TomlStore},
    };
    struct CancelAt<'a> {
        root: &'a Path,
        boundary: Boundary,
        reached: bool,
    }
    impl BoundaryObserver for CancelAt<'_> {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == self.boundary {
                GitRequest::new(self.root).cancellation().cancel();
                self.reached = true;
            }
            Ok(())
        }
    }
    for boundary in [Boundary::Intent, Boundary::GraphWritten] {
        let (fixture, target) = withdrawal_subject(2);
        let store = TomlStore::new(fixture.root());
        let graph = GraphService::new(&store);
        let id = uuid::Uuid::new_v4().to_string();
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let stable = git(fixture.root(), &["rev-parse", "master"]);
        let before = fixture.graph();
        let mut observer = CancelAt {
            root: fixture.root(),
            boundary,
            reached: false,
        };
        let error = NativePlanWorkspace::new(&graph)
            .withdraw(
                PlanWithdrawRequest {
                    expected_revision: 0,
                    id: PLAN.into(),
                    reason: "retired fixture".into(),
                    operation_id: OperationId::parse(&id).unwrap(),
                    actor: "coordinator".into(),
                },
                &mut observer,
            )
            .unwrap_err();
        assert!(observer.reached, "{boundary:?}: {error}");
        let MineError::GraphCheckpointPending {
            operation,
            revision_after,
            detail,
        } = error
        else {
            panic!(
                "{boundary:?}: durable cancellation must report exact pending identity, got {error}"
            )
        };
        assert_eq!(operation.fields().operation_id.as_str(), id);
        assert!(detail.contains("cancelled"), "{detail}");
        assert_eq!(
            revision_after,
            Some(if boundary == Boundary::Intent { 0 } else { 1 })
        );
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), original);
        if boundary == Boundary::Intent {
            assert_eq!(fixture.graph(), before);
            assert!(target.exists());
        }
        let retained = ok(
            fixture.root(),
            &["operation", "show", "--operation-id", &id],
        );
        assert_eq!(retained["data"]["operation"]["operation_id"], id);
        let (code, recovered) = cli(fixture.root(), &withdrawal_args(&id));
        assert_eq!(code, 0, "{boundary:?}: {recovered}");
        assert_eq!(recovered["data"]["operation"]["operation_id"], id);
        assert_eq!(recovered["revision_after"], 1);
        assert!(!target.exists());
        assert_eq!(
            git(
                fixture.root(),
                &["rev-list", "--count", &format!("{original}..HEAD")]
            ),
            "1"
        );
        assert_eq!(git(fixture.root(), &["rev-parse", "master"]), stable);
        assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    }
}

#[test]
fn actual_cli_native_withdrawal_refuses_foreign_state_without_cleanup_or_intent() {
    for failure in 0..5 {
        let (fixture, target) = withdrawal_subject(if failure == 4 { 1 } else { 2 });
        let branch = format!("plan/{PLAN}-fixture");
        let marker = match failure {
            0 => {
                let p = target.join("untracked-user.txt");
                fs::write(&p, b"preserve user bytes").unwrap();
                Some(p)
            }
            1 => {
                let p = target.join(".mine/build/user/cache.bin");
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, b"preserve ignored bytes").unwrap();
                Some(p)
            }
            2 => {
                fs::write(target.join("unique-product.txt"), "not integrated\n").unwrap();
                git(&target, &["add", "unique-product.txt"]);
                git(&target, &["commit", "-m", "foreign Plan advancement"]);
                None
            }
            3 => {
                git(
                    fixture.root(),
                    &[
                        "symbolic-ref",
                        &format!("refs/heads/{branch}"),
                        "refs/heads/dev",
                    ],
                );
                None
            }
            _ => {
                let foreign = fixture.root().join(".mine/worktrees/foreign");
                git(
                    fixture.root(),
                    &["worktree", "add", foreign.to_str().unwrap(), &branch],
                );
                None
            }
        };
        let graph = fixture.graph();
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let refs = git(
            fixture.root(),
            &[
                "for-each-ref",
                "--format=%(refname) %(symref) %(objectname)",
            ],
        );
        let inventory = git(fixture.root(), &["worktree", "list", "--porcelain"]);
        let id = uuid::Uuid::new_v4().to_string();
        let (code, value) = cli(fixture.root(), &withdrawal_args(&id));
        assert_ne!(code, 0, "case{failure}: {value}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(
            git(
                fixture.root(),
                &[
                    "for-each-ref",
                    "--format=%(refname) %(symref) %(objectname)"
                ]
            ),
            refs
        );
        assert_eq!(
            git(fixture.root(), &["worktree", "list", "--porcelain"]),
            inventory
        );
        assert!(
            !fixture
                .root()
                .join(format!(".mine/runtime/operations/{id}.json"))
                .exists()
        );
        if let Some(marker) = marker {
            assert!(!fs::read(marker).unwrap().is_empty());
        }
    }
}

#[test]
fn pending_plan_git_intent_blocks_new_graph_writes_without_a_revision_change() {
    use mine::{
        application::{
            graph_service::GraphService, native_plan_workspace::NativePlanWorkspace,
            plan_service::PlanWithdrawRequest,
        },
        domain::lifecycle_operation::*,
        infrastructure::toml_store::TomlStore,
    };
    struct Stop;
    impl BoundaryObserver for Stop {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == Boundary::Intent {
                Err(OperationError::Interrupted(boundary))
            } else {
                Ok(())
            }
        }
    }
    let (fixture, target) = withdrawal_subject(2);
    let store = TomlStore::new(fixture.root());
    let service = GraphService::new(&store);
    let id = uuid::Uuid::new_v4().to_string();
    let request = PlanWithdrawRequest {
        expected_revision: 0,
        id: PLAN.into(),
        reason: "retired fixture".into(),
        operation_id: OperationId::parse(&id).unwrap(),
        actor: "coordinator".into(),
    };
    assert!(
        NativePlanWorkspace::new(&service)
            .withdraw(request, &mut Stop)
            .is_err()
    );
    let before = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let journal = fixture
        .root()
        .join(format!(".mine/runtime/operations/{id}.json"));
    let intent = fs::read(&journal).unwrap();
    let new_id = uuid::Uuid::new_v4().to_string();
    for args in [
        vec![
            "plan",
            "release",
            "--id",
            PLAN,
            "--expected-revision",
            "0",
            "--actor",
            "coordinator",
        ],
        withdrawal_args(&new_id),
    ] {
        let (code, result) = cli(fixture.root(), &args);
        assert_ne!(code, 0, "{result}");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("pending"),
            "{result}"
        );
        assert_eq!(fixture.graph(), before);
        assert_eq!(fs::read(&journal).unwrap(), intent);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert!(target.is_dir());
    }
    let (code, stale) = cli(
        fixture.root(),
        &[
            "plan",
            "release",
            "--id",
            PLAN,
            "--expected-revision",
            "99",
            "--actor",
            "coordinator",
        ],
    );
    assert_ne!(code, 0, "{stale}");
    assert_eq!(stale["error"]["code"], "MINE_REVISION_CONFLICT");
    let (code, resumed) = cli(fixture.root(), &withdrawal_args(&id));
    assert_eq!(code, 0, "{resumed}");
    assert_eq!(resumed["revision_after"], 1);
}

#[test]
fn actual_cli_native_withdrawal_preserves_unknown_changes_after_removal_intent() {
    use mine::{
        application::{
            graph_service::GraphService, native_plan_workspace::NativePlanWorkspace,
            plan_service::PlanWithdrawRequest,
        },
        domain::lifecycle_operation::*,
        infrastructure::toml_store::TomlStore,
    };
    struct StopAfterRemoval;
    impl BoundaryObserver for StopAfterRemoval {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == Boundary::PlanGitRecorded {
                Err(OperationError::Interrupted(boundary))
            } else {
                Ok(())
            }
        }
    }
    for failure in 0..4 {
        let (fixture, target) = withdrawal_subject(2);
        let branch = format!("plan/{PLAN}-fixture");
        let store = TomlStore::new(fixture.root());
        let service = GraphService::new(&store);
        let id = uuid::Uuid::new_v4().to_string();
        let request = PlanWithdrawRequest {
            expected_revision: 0,
            id: PLAN.into(),
            reason: "retired fixture".into(),
            operation_id: OperationId::parse(&id).unwrap(),
            actor: "coordinator".into(),
        };
        assert!(
            NativePlanWorkspace::new(&service)
                .withdraw(request, &mut StopAfterRemoval)
                .is_err()
        );
        assert!(!target.exists());
        let journal_path = fixture
            .root()
            .join(format!(".mine/runtime/operations/{id}.json"));
        let original = fs::read(&journal_path).unwrap();
        let mut record: Value = serde_json::from_slice(&original).unwrap();
        match failure {
            0 => {
                git(
                    fixture.root(),
                    &[
                        "symbolic-ref",
                        &format!("refs/heads/{branch}"),
                        "refs/heads/dev",
                    ],
                );
            }
            1 => {
                fs::create_dir_all(&target).unwrap();
                fs::write(
                    target.join("foreign.txt"),
                    "preserve replacement directory\n",
                )
                .unwrap();
            }
            2 => {
                record["checkpoint"]
                    .as_object_mut()
                    .unwrap()
                    .remove("plan_git");
                fs::write(&journal_path, serde_json::to_vec(&record).unwrap()).unwrap();
            }
            _ => {
                record["checkpoint"]["plan_git"]["intent"]["reason"] = json!("corrupt identity");
                fs::write(&journal_path, serde_json::to_vec(&record).unwrap()).unwrap();
            }
        }
        let graph = fixture.graph();
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let frozen = fs::read(&journal_path).unwrap();
        let (code, value) = cli(fixture.root(), &withdrawal_args(&id));
        assert_ne!(code, 0, "case{failure}: {value}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(&journal_path).unwrap(), frozen);
        if failure == 1 {
            assert_eq!(
                fs::read_to_string(target.join("foreign.txt")).unwrap(),
                "preserve replacement directory\n"
            );
        }
        assert!(
            !git(
                fixture.root(),
                &[
                    "for-each-ref",
                    "--format=%(refname)",
                    &format!("refs/heads/{branch}")
                ]
            )
            .is_empty()
        );
    }
}

fn compensation_withdrawal_subject(
    prepared: bool,
) -> (Fixture, std::path::PathBuf, String, String) {
    let (fixture, target) = withdrawal_subject(2);
    let base_branch = format!("plan/{PLAN}-fixture");
    let anchor = git(fixture.root(), &["rev-parse", "HEAD"]);
    fs::write(
        target.join("rejected-product.txt"),
        "preserve rejected-only implementation\n",
    )
    .unwrap();
    git(&target, &["add", "rejected-product.txt"]);
    git(
        &target,
        &["commit", "-m", "rejected implementation fixture"],
    );
    let preserved = git(&target, &["rev-parse", "HEAD"]);
    let cp = format!("{PLAN}-C01");
    let cp_path = format!(".mine/plan/{cp}-fixture.md");
    let cp_branch = format!("plan/{cp}-fixture");
    let body = b"# Frozen compensation withdrawal fixture\n";
    fs::write(fixture.root().join(&cp_path), body).unwrap();
    let mut graph: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let mut compensation = graph.plans[0].clone();
    compensation.id = cp.clone();
    compensation.path = cp_path;
    compensation.branch = cp_branch.clone();
    compensation.body_sha256 = Some(digest(body));
    graph.plans[0].status = mine::domain::status::PlanStatus::Rejected;
    graph.plans[0].branch_anchor = anchor;
    graph.plans[0].compensating_plan = cp.clone();
    graph.plans.push(compensation);
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.toml"),
        toml::to_string(&graph).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.md"),
        render_markdown(&graph).unwrap(),
    )
    .unwrap();
    git(fixture.root(), &["add", ".mine/plan"]);
    git(
        fixture.root(),
        &[
            "commit",
            "-m",
            "explicit legacy rejected compensation registration",
        ],
    );
    if prepared {
        git(fixture.root(), &["branch", &cp_branch, &preserved]);
        git(&target, &["switch", "--no-guess", &cp_branch]);
    }
    assert_eq!(git(fixture.root(), &["rev-parse", &base_branch]), preserved);
    (fixture, target, cp, preserved)
}

#[test]
fn actual_cli_native_withdrawal_compensation_preserves_rejected_ref_and_unowned_checkout() {
    for prepared in [false, true] {
        let (fixture, target, cp, preserved) = compensation_withdrawal_subject(prepared);
        let id = uuid::Uuid::new_v4().to_string();
        let mut args = withdrawal_args(&id);
        let position = args.iter().position(|v| *v == "--id").unwrap() + 1;
        args[position] = &cp;
        let graph = fixture.graph();
        let original = git(fixture.root(), &["rev-parse", "HEAD"]);
        let trace = fixture
            .root()
            .join(format!(".mine/build/{cp}/verification/withdraw.trace"));
        fs::create_dir_all(trace.parent().unwrap()).unwrap();
        let (code, value) = cli_observed(fixture.root(), &args, Some(&trace));
        let trace_bytes = fs::read(&trace).unwrap();
        let mut commands = std::collections::BTreeMap::<String, usize>::new();
        let mut count = 0;
        for row in std::str::from_utf8(&trace_bytes).unwrap().lines() {
            let row: Value = serde_json::from_str(row).unwrap();
            if row["event"] == "start" {
                count += 1;
                let argv = row["argv"].as_array().unwrap();
                let command = argv
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|arg| {
                        matches!(
                            *arg,
                            "rev-parse"
                                | "status"
                                | "config"
                                | "ls-files"
                                | "check-attr"
                                | "for-each-ref"
                                | "worktree"
                                | "symbolic-ref"
                                | "cat-file"
                                | "merge-base"
                                | "update-ref"
                                | "diff-index"
                                | "diff-tree"
                                | "hash-object"
                                | "write-tree"
                                | "update-index"
                                | "commit-tree"
                        )
                    })
                    .unwrap_or("other");
                *commands.entry(command.into()).or_default() += 1;
            }
        }
        println!(
            "native_compensation_withdrawal_git_start_events={count}; prepared={prepared}; commands={commands:?}"
        );
        if prepared && code != 0 {
            // Retain failing evidence outside this disposable fixture before
            // its panic unwinds TempDir. TEMP is Plan-isolated by the runner.
            use std::io::Write;
            let mut retained = tempfile::NamedTempFile::new().unwrap();
            retained.write_all(&trace_bytes).unwrap();
            let (_, path) = retained.keep().unwrap();
            println!("failed withdrawal trace retained at {}", path.display());
        }
        if prepared {
            assert_eq!(code, 0, "{value}");
            assert!(!target.exists());
        } else {
            assert_ne!(code, 0, "{value}");
            assert!(target.is_dir());
            assert_eq!(fixture.graph(), graph);
            assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), original);
            assert!(
                !fixture
                    .root()
                    .join(format!(".mine/runtime/operations/{id}.json"))
                    .exists()
            );
        }
        assert_eq!(
            git(
                fixture.root(),
                &["rev-parse", &format!("plan/{PLAN}-fixture")]
            ),
            preserved
        );
        assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    }
}

#[tokio::test]
async fn actual_mcp_native_withdrawal_and_cli_replay_share_the_exact_checkpoint() {
    use rmcp::{
        model::CallToolRequestParams, service::serve_client,
        transport::child_process::TokioChildProcess,
    };
    let (fixture, _) = withdrawal_subject(0);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
    command.current_dir(fixture.root()).args(["mcp", "serve"]);
    let client = serve_client((), TokioChildProcess::builder(command).spawn().unwrap().0)
        .await
        .unwrap();
    let inventory = client.peer().list_tools(Default::default()).await.unwrap();
    let schema = serde_json::to_value(
        &inventory
            .tools
            .iter()
            .find(|t| t.name == "mine_plan_withdraw")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    for field in ["id", "actor", "reason", "expected_revision", "operation_id"] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == field),
            "{schema}"
        );
    }
    let id = uuid::Uuid::new_v4().to_string();
    let args = json!({"id":PLAN,"actor":"coordinator","reason":"retired fixture","expected_revision":0,"operation_id":id});
    let graph = fixture.graph();
    for missing in ["actor", "operation_id"] {
        let mut bad = args.clone();
        bad.as_object_mut().unwrap().remove(missing);
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("mine_plan_withdraw")
                    .with_arguments(bad.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(fixture.graph(), graph);
    }
    let result = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_plan_withdraw")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let value = result.structured_content.unwrap();
    assert_eq!(value["data"]["operation"]["operation_id"], id);
    assert_eq!(value["revision_before"], 0);
    assert_eq!(value["revision_after"], 1);
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let (code, replay) = cli(fixture.root(), &withdrawal_args(&id));
    assert_eq!(code, 0, "{replay}");
    assert_eq!(replay["data"]["replay"], true);
    assert_eq!(replay["data"]["operation"], value["data"]["operation"]);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    client.cancel().await.unwrap();
}

#[test]
fn actual_cli_verification_root_controls_keep_linked_source_and_full_keys() {
    use mine::application::verification_service::VerificationBackend;
    use mine::domain::ports::Clock;
    use mine::infrastructure::{system::SystemClock, verification_store::FileVerificationStore};
    let fixture = Fixture::new(None, true);
    let context = fixture.verification_policy();
    let target = fixture.implemented_subject();
    let graph = fixture.graph();
    // A real registered checkout, not a mocked source resolver. Root policy
    // differs deliberately; only Root request files may be read as controls.
    fs::write(
        fixture.root().join(".mine/verification-policy.json"),
        b"invalid Root policy",
    )
    .unwrap();
    let store = FileVerificationStore::new(&target, fixture.root()).unwrap();
    let policy = store
        .load_policy(&path(".mine/verification-policy.json"))
        .unwrap();
    let manifest = store.manifest(&policy, "probe").unwrap();
    let input = json!({"schema_version":1,"plan_id":PLAN,"actor":"runner","check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()});
    let (code, value) = fixture.verification_from(&target, "capture", &input, None);
    assert_eq!(code, 0, "{value}");
    let capture = value["data"]["capture"].clone();
    assert_eq!(
        capture["manifest"],
        serde_json::to_value(&manifest).unwrap()
    );
    assert!(!target.join(VERIFICATION_INPUT).exists());
    let exported = fixture.root().join(format!(
        ".mine/build/{PLAN}/verification/exported-capture.json"
    ));
    fs::write(exported, serde_json::to_vec(&capture).unwrap()).unwrap();
    assert_eq!(store.manifest(&policy, "probe").unwrap(), manifest);
    // Also observe the Root Full key before/after another mutable output.
    let root_store = FileVerificationStore::new(fixture.root(), fixture.root()).unwrap();
    let root_manifest = root_store.manifest(&policy, "probe").unwrap();
    fs::write(
        fixture
            .root()
            .join(format!(".mine/build/{PLAN}/verification/context.json")),
        serde_json::to_vec(&context).unwrap(),
    )
    .unwrap();
    assert_eq!(
        root_store.manifest(&policy, "probe").unwrap(),
        root_manifest
    );

    let plan: PlanIdentity = serde_json::from_value(capture["plan"].clone()).unwrap();
    let _lease = store.lease_outputs(&plan).unwrap();
    let output = Command::new("git")
        .current_dir(&target)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let log = format!(".mine/build/{PLAN}/verification/probe.log");
    fs::write(fixture.root().join(&log), output.stdout).unwrap();
    let artifact = store
        .retain(&plan, &path(&log), policy.check("probe").unwrap())
        .unwrap();
    let record = json!({"schema_version":1,"capture":capture,"observation":{
        "ended_at":SystemClock.now_utc_rfc3339(),"exit_code":0,"outcome":"passed","warnings_observed":false,"artifacts":[artifact]}});
    let (code, value) = fixture.verification_from(&target, "record", &record, None);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["disposition"], "RUN");
    let receipt = format!(
        ".mine/plan/reports/verification/{PLAN}/{}.json",
        value["data"]["decision"]["original"]["receipt_id"]
            .as_str()
            .unwrap()
    );
    let original = fs::read(target.join(&receipt)).unwrap();
    fs::create_dir_all(fixture.root().join(&receipt).parent().unwrap()).unwrap();
    fs::write(fixture.root().join(&receipt), b"invalid Root receipt").unwrap();
    let check = json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context});
    let (code, value) = fixture.verification_from(&target, "check", &check, Some(&receipt));
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["disposition"], "REUSED");
    assert_eq!(fs::read(target.join(&receipt)).unwrap(), original);

    // Root has the genuine body; it must never repair missing/wrong source.
    let body = fs::read(target.join(PLAN_PATH)).unwrap();
    fs::write(target.join(PLAN_PATH), b"wrong linked Plan body").unwrap();
    for (verb, request, receipt) in [
        ("capture", &input, None),
        ("record", &record, None),
        ("check", &check, Some(receipt.as_str())),
    ] {
        let (code, value) = fixture.verification_from(&target, verb, request, receipt);
        if verb == "check" {
            assert_eq!(code, 0, "{value}");
            assert_eq!(value["data"]["decision"]["disposition"], "INVALIDATED");
        } else {
            assert_ne!(code, 0, "{verb}: {value}");
        }
    }
    fs::write(target.join(PLAN_PATH), body).unwrap();
    // Retained artifact validation is source-relative too, not Root fallback.
    let retained = record["observation"]["artifacts"][0]["path"]
        .as_str()
        .unwrap();
    let artifact_body = fs::read(target.join(retained)).unwrap();
    fs::create_dir_all(fixture.root().join(retained).parent().unwrap()).unwrap();
    fs::write(fixture.root().join(retained), &artifact_body).unwrap();
    fs::write(target.join(retained), b"corrupt linked retained artifact").unwrap();
    let (code, value) = fixture.verification_from(&target, "check", &check, Some(&receipt));
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["reason"], "artifacts_changed");
    fs::write(target.join(retained), artifact_body).unwrap();
    fs::write(fixture.root().join(&receipt), &original).unwrap();
    fs::write(target.join(&receipt), b"malformed linked receipt").unwrap();
    let (code, value) = fixture.verification_from(&target, "check", &check, Some(&receipt));
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["reason"], "evidence_unavailable");
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    assert!(!target.join(".mine/build").exists());
}

#[test]
fn actual_cli_verification_root_controls_refuse_foreign_namespace_and_linked_fallback() {
    use mine::domain::ports::Clock;
    use mine::infrastructure::system::SystemClock;
    let fixture = Fixture::new(None, true);
    let context = fixture.verification_policy();
    let target = fixture.implemented_subject();
    let input = json!({"schema_version":1,"plan_id":PLAN,"actor":"runner","check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()});
    let (code, value) = fixture.verification_from(&target, "capture", &input, None);
    assert_eq!(code, 0, "{value}");
    let record = json!({"schema_version":1,"capture":value["data"]["capture"],"observation":{"ended_at":SystemClock.now_utc_rfc3339(),"exit_code":0,"outcome":"passed","warnings_observed":false,"artifacts":[]}});
    let check = json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context});
    let graph = fixture.graph();
    let foreign = VERIFICATION_INPUT.replace(PLAN, &format!("{PLAN}-C01"));
    fs::create_dir_all(fixture.root().join(&foreign).parent().unwrap()).unwrap();
    for (verb, request) in [("capture", &input), ("record", &record), ("check", &check)] {
        fs::write(
            fixture.root().join(&foreign),
            serde_json::to_vec(request).unwrap(),
        )
        .unwrap();
        let mut args = vec!["verification", verb, "--input", &foreign];
        if verb == "check" {
            args.extend(["--receipt", ".mine/plan/reports/missing.json"]);
        }
        let (code, value) = cli(&target, &args);
        assert_ne!(code, 0, "{verb}: {value}");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("namespace"),
            "{value}"
        );
    }
    // Root absence cannot be rescued by valid same-named checkout bytes.
    fs::remove_file(fixture.root().join(VERIFICATION_INPUT)).unwrap();
    fs::create_dir_all(target.join(VERIFICATION_INPUT).parent().unwrap()).unwrap();
    fs::write(
        target.join(VERIFICATION_INPUT),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    assert_ne!(
        cli(
            &target,
            &["verification", "capture", "--input", VERIFICATION_INPUT]
        )
        .0,
        0
    );
    let request_path = fixture.root().join(VERIFICATION_INPUT);
    fs::create_dir(&request_path).unwrap();
    assert_ne!(
        cli(
            &target,
            &["verification", "capture", "--input", VERIFICATION_INPUT]
        )
        .0,
        0
    );
    fs::remove_dir(&request_path).unwrap();
    let oversized = fs::File::create(&request_path).unwrap();
    oversized.set_len(32 * 1024 * 1024 + 1).unwrap();
    drop(oversized);
    let (code, value) = cli(
        &target,
        &["verification", "capture", "--input", VERIFICATION_INPUT],
    );
    assert_ne!(code, 0, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("bounded regular file"),
        "{value}"
    );
    fs::remove_file(&request_path).unwrap();
    // A real directory link still cannot grant a Root-contained or external
    // alias. Windows junction creation does not require symlink privilege.
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("request.json"),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let linked_class = request_path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("linked-control");
    directory_alias(&linked_class, outside.path());
    let linked_selector = format!(".mine/build/{PLAN}/linked-control/request.json");
    let (code, value) = cli(
        &target,
        &["verification", "capture", "--input", &linked_selector],
    );
    assert_ne!(code, 0, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("link/reparse"),
        "{value}"
    );
    // A reserved path needs both output class and filename; no arbitrary source
    // selector, shorthand ID, case alias or traversal can grant a content read.
    for selector in [
        PLAN_PATH.to_string(),
        format!(".mine/build/{PLAN}/request.json"),
        VERIFICATION_INPUT.replace(PLAN, "P81"),
        VERIFICATION_INPUT.replace(".mine", ".MINE"),
        format!(".mine/build/{PLAN}/verification/../request.json"),
    ] {
        let (code, _) = cli(&target, &["verification", "capture", "--input", &selector]);
        assert_ne!(code, 0, "{selector}");
    }
    assert_eq!(fixture.graph(), graph);
    assert!(
        !target
            .join(format!(".mine/plan/reports/verification/{PLAN}"))
            .exists()
    );
}

#[test]
fn actual_cli_verification_compensation_control_namespace_is_not_lineage_namespace() {
    use mine::domain::ports::Clock;
    use mine::infrastructure::system::SystemClock;
    let fixture = Fixture::new(None, true);
    let context = fixture.verification_policy();
    fixture.seed_legacy_node(true);
    let cp = format!("{PLAN}-C01");
    let cp_path = format!(".mine/plan/{cp}-fixture.md");
    let body = b"# Frozen compensation fixture\n";
    fs::write(fixture.root().join(&cp_path), body).unwrap();
    let mut workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    // Explicit imported legacy authority, not fabricated native prepare/start.
    let mut compensation = workspace.plans[0].clone();
    compensation.id = cp.clone();
    compensation.path = cp_path;
    compensation.branch = format!("plan/{cp}-fixture");
    compensation.body_sha256 = Some(digest(body));
    workspace.plans[0].status = mine::domain::status::PlanStatus::Rejected;
    workspace.plans[0].branch_anchor = git(fixture.root(), &["rev-parse", "HEAD"]);
    workspace.plans[0].compensating_plan = cp.clone();
    workspace.plans.push(compensation);
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root().join(".mine/plan/execution-graph.md"),
        render_markdown(&workspace).unwrap(),
    )
    .unwrap();
    git(fixture.root(), &["add", ".mine/plan"]);
    git(
        fixture.root(),
        &["commit", "-m", "explicit legacy compensation fixture"],
    );
    let graph = fixture.graph();
    let input = json!({"schema_version":1,"plan_id":cp,"actor":"runner","check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()});
    let (code, value) = fixture.verification("capture", &input, None);
    assert_ne!(code, 0, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("namespace"),
        "{value}"
    );
    let cp_input = VERIFICATION_INPUT.replace(PLAN, &cp);
    fs::create_dir_all(fixture.root().join(&cp_input).parent().unwrap()).unwrap();
    fs::write(
        fixture.root().join(&cp_input),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let (code, value) = cli(
        fixture.root(),
        &["verification", "capture", "--input", &cp_input],
    );
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["capture"]["plan"]["plan_id"], cp);
    assert_eq!(value["data"]["capture"]["plan"]["lineage_id"], PLAN);
    assert_eq!(fixture.graph(), graph);
}

#[test]
fn actual_cli_verification_capture_record_check_uses_fresh_context_without_running_checks() {
    use mine::domain::ports::Clock;
    use mine::infrastructure::{system::SystemClock, verification_store::FileVerificationStore};
    let fixture = Fixture::new(None, true);
    assert_eq!(fixture.register("0").0, 0);
    let context = fixture.verification_policy();
    let graph = fixture.graph();
    let capture_input = json!({"schema_version":1,"plan_id":PLAN,"actor":"runner","check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()});
    let (code, value) = fixture.verification("capture", &capture_input, None);
    assert_eq!(code, 0, "{value}");
    let capture = value["data"]["capture"].clone();
    let plan: PlanIdentity = serde_json::from_value(capture["plan"].clone()).unwrap();
    let store = FileVerificationStore::new(fixture.root(), fixture.root()).unwrap();
    let policy = store
        .load_policy(&path(".mine/verification-policy.json"))
        .unwrap();
    let _producer = store.lease_outputs(&plan).unwrap();
    // The runner actually executes the captured command, outside MINE. Its
    // output is retained through the accepted store/owned-output lease path.
    let output = Command::new("git")
        .current_dir(fixture.root())
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let run_output = fixture.root().join(format!(".mine/build/{PLAN}/probe.log"));
    fs::create_dir_all(run_output.parent().unwrap()).unwrap();
    fs::write(&run_output, &output.stdout).unwrap();
    let artifact = store
        .retain(
            &plan,
            &path(&format!(".mine/build/{PLAN}/probe.log")),
            policy.check("probe").unwrap(),
        )
        .unwrap();
    let record_input = json!({"schema_version":1,"capture":capture,"observation":{
        "ended_at":SystemClock.now_utc_rfc3339(),"exit_code":output.status.code(),"outcome":"passed",
        "warnings_observed":false,"artifacts":[artifact]
    }});
    let (code, value) = fixture.verification("record", &record_input, None);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["disposition"], "RUN");
    let receipt = format!(
        ".mine/plan/reports/verification/{PLAN}/{}.json",
        value["data"]["decision"]["original"]["receipt_id"]
            .as_str()
            .unwrap()
    );
    let before = fs::read(fixture.root().join(&receipt)).unwrap();
    let check = json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context});
    let trace = fixture
        .root()
        .join(".mine/runtime/verification-trace.jsonl");
    let (code, _) = fixture.verification("check", &check, Some(&receipt));
    assert_eq!(code, 0);
    let (code, value) = cli_observed(
        fixture.root(),
        &[
            "verification",
            "check",
            "--receipt",
            &receipt,
            "--input",
            VERIFICATION_INPUT,
        ],
        Some(&trace),
    );
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["decision"]["disposition"], "REUSED");
    for line in fs::read_to_string(trace).unwrap().lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["event"] == "start" {
            assert!(
                !record["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "--version"),
                "check executed the runner command: {record}"
            );
        }
    }
    let mut changed = check.clone();
    changed["context"]["toolchains"]["git"] = json!("0".repeat(64));
    assert_eq!(
        fixture.verification("check", &changed, Some(&receipt)).1["data"]["decision"]["disposition"],
        "INVALIDATED"
    );
    changed = check.clone();
    changed.as_object_mut().unwrap().remove("context");
    assert_ne!(fixture.verification("check", &changed, Some(&receipt)).0, 0);
    assert_ne!(
        fixture
            .verification("check", &check, Some(&receipt.replace(PLAN, "foreign")))
            .0,
        0
    );
    fs::write(
        fixture.root().join("relevant-untracked-input"),
        "new source",
    )
    .unwrap();
    assert_eq!(
        fixture.verification("check", &check, Some(&receipt)).1["data"]["decision"]["reason"],
        "inputs_changed"
    );
    assert_eq!(fs::read(fixture.root().join(&receipt)).unwrap(), before);
    assert_eq!(fixture.graph(), graph);
}

#[test]
fn actual_cli_verification_refuses_duplicate_context_keys_and_changed_live_policy_before_record() {
    use mine::domain::ports::Clock;
    use mine::infrastructure::system::SystemClock;
    let fixture = Fixture::new(None, true);
    assert_eq!(fixture.register("0").0, 0);
    let context = fixture.verification_policy();
    let input = json!({"schema_version":1,"plan_id":PLAN,"actor":"runner","check":"probe","context":context,"started_at":SystemClock.now_utc_rfc3339()});
    let (code, value) = fixture.verification("capture", &input, None);
    assert_eq!(code, 0, "{value}");
    let capture = value["data"]["capture"].clone();
    let mut policy: Value = serde_json::from_slice(
        &fs::read(fixture.root().join(".mine/verification-policy.json")).unwrap(),
    )
    .unwrap();
    policy["checks"][0]["warnings_disqualify"] = json!(false);
    fs::write(
        fixture.root().join(".mine/verification-policy.json"),
        serde_json::to_vec(&policy).unwrap(),
    )
    .unwrap();
    let record = json!({"schema_version":1,"capture":capture,"observation":{"ended_at":SystemClock.now_utc_rfc3339(),"exit_code":0,"outcome":"passed","warnings_observed":false,"artifacts":[]}});
    let graph = fixture.graph();
    let (code, value) = fixture.verification("record", &record, None);
    assert_ne!(code, 0, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("live policy"),
        "{value}"
    );
    let encoded = serde_json::to_string(&input).unwrap();
    let tool = context["toolchains"]["git"].as_str().unwrap();
    let duplicate = encoded.replace(
        &format!("\"toolchains\":{{\"git\":\"{tool}\"}}"),
        &format!("\"toolchains\":{{\"git\":\"{tool}\",\"git\":\"{tool}\"}}"),
    );
    assert_ne!(duplicate, encoded);
    fs::write(fixture.root().join(VERIFICATION_INPUT), duplicate).unwrap();
    assert_ne!(
        cli(
            fixture.root(),
            &["verification", "capture", "--input", VERIFICATION_INPUT]
        )
        .0,
        0
    );
    assert_eq!(fixture.graph(), graph);
}

#[test]
fn actual_cli_closure_begin_pins_real_independent_acceptance_and_rejection_handoffs() {
    for rejected in [false, true] {
        let fixture = Fixture::new(None, true);
        let target = fixture.implemented_subject();
        let assertion = format!(
            "```mine-review-closure\n{}\n```",
            fixture.closure_assertion()
        );
        let input = fixture.closure_input(&target, &assertion, rejected);
        let graph = fixture.graph();
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let (code, value) = fixture.begin_closure("3");
        assert_eq!(code, 0, "{value}");
        assert_eq!(value["revision_before"], 3);
        assert_eq!(value["revision_after"], 3);
        assert_eq!(value["data"]["operation"]["outcome"], "pending");
        assert_eq!(
            value["data"]["operation"]["review"],
            input["intent"]["review"]
        );
        let id = value["data"]["operation"]["operation_id"].as_str().unwrap();
        let journal = fs::read(
            fixture
                .root()
                .join(format!(".mine/runtime/operations/{id}.json")),
        )
        .unwrap();
        let queried = ok(fixture.root(), &["operation", "show", "--operation-id", id]);
        assert_eq!(queried["data"]["operation"], value["data"]["operation"]);
        assert_ne!(fixture.begin_closure("3").0, 0);
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(
            fs::read(
                fixture
                    .root()
                    .join(format!(".mine/runtime/operations/{id}.json"))
            )
            .unwrap(),
            journal
        );
    }
}

#[tokio::test]
async fn actual_cli_mcp_pending_operations_recover_lost_begin_identity_and_refuse_late_corruption()
{
    use rmcp::{
        model::CallToolRequestParams, service::serve_client,
        transport::child_process::TokioChildProcess,
    };
    for rejected in [false, true] {
        let fixture = Fixture::new(None, true);
        let target = fixture.implemented_subject();
        let fence = format!(
            "```mine-review-closure\n{}\n```",
            fixture.closure_assertion()
        );
        fixture.closure_input(&target, &fence, rejected);
        let graph = fixture.graph();
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let begin = [
            "operation",
            "begin",
            "--expected-revision",
            "3",
            "--input",
            CLOSURE_INPUT,
        ];
        let (code, value) = cli(fixture.root(), &begin);
        assert_eq!(code, 0, "{value}");
        let operation = value["data"]["operation"].clone();
        let journal_dir = fixture.root().join(".mine/runtime/operations");
        let catalog_path = journal_dir.join(".catalog");
        let catalog = fs::read(&catalog_path).unwrap();
        // A runner whose successful begin response was lost discovers this
        // same durable ID through ordinary read-only Plan/status surfaces.
        let show = ok(fixture.root(), &["plan", "show", "--id", PLAN]);
        let status = ok(fixture.root(), &["graph", "status"]);
        let summary = status["data"]["pending_operations"].clone();
        assert_eq!(summary.as_array().unwrap().len(), 1);
        assert_eq!(summary[0]["operation"], operation);
        assert_eq!(show["data"]["plan"]["pending_operations"], summary);
        assert!(
            !serde_json::to_string(&summary)
                .unwrap()
                .contains("original_index")
        );
        assert_ne!(cli(fixture.root(), &begin).0, 0);
        assert_eq!(fs::read(&catalog_path).unwrap(), catalog);
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);

        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
        command.current_dir(fixture.root()).args(["mcp", "serve"]);
        let client = serve_client((), TokioChildProcess::builder(command).spawn().unwrap().0)
            .await
            .unwrap();
        for tool in [
            "mine_graph_status",
            "mine_workspace_status",
            "mine_plan_show",
        ] {
            let args = if tool == "mine_plan_show" {
                json!({"id":PLAN})
            } else {
                json!({})
            };
            let result = client
                .peer()
                .call_tool(
                    CallToolRequestParams::new(tool)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_ne!(result.is_error, Some(true), "{result:?}");
            let data = result.structured_content.unwrap();
            let pending = if tool == "mine_plan_show" {
                &data["data"]["plan"]["pending_operations"]
            } else {
                &data["pending_operations"]
            };
            assert_eq!(*pending, summary);
        }

        // Filtering to one Plan/unfinished records must follow full validation,
        // not hide a malformed or foreign record later in the same catalog.
        let other_id = uuid::Uuid::new_v4().to_string();
        let other_path = journal_dir.join(format!("{other_id}.json"));
        let mut changed_catalog: Value = serde_json::from_slice(&catalog).unwrap();
        changed_catalog["operations"]
            .as_array_mut()
            .unwrap()
            .push(json!(other_id));
        changed_catalog["operations"]
            .as_array_mut()
            .unwrap()
            .sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        fs::write(&catalog_path, serde_json::to_vec(&changed_catalog).unwrap()).unwrap();
        let original: Value = serde_json::from_slice(
            &fs::read(journal_dir.join(format!(
                "{}.json",
                operation["operation_id"].as_str().unwrap()
            )))
            .unwrap(),
        )
        .unwrap();
        for failure in 0..3 {
            let mut foreign = original.clone();
            foreign["operation"]["operation_id"] = json!(other_id);
            // Even a terminal record for a different Plan must be validated
            // before either the pending or requested-Plan filter can run.
            foreign["operation"]["outcome"] = json!("refused");
            let other_plan = format!("{PLAN}-02");
            foreign["operation"]["plan"]["plan_id"] = json!(other_plan);
            foreign["operation"]["plan"]["plan_path"] =
                json!(format!(".mine/plan/{other_plan}-fixture.md"));
            foreign["operation"]["plan"]["lineage_id"] = json!(other_plan);
            foreign["operation"]["plan"]["branch"] = json!(format!("plan/{other_plan}-fixture"));
            foreign["operation"]["registered_worktree"] =
                json!(format!(".mine/worktrees/{other_plan}"));
            foreign["operation"]["review"]["report"]["path"] =
                json!(format!(".mine/plan/reports/{other_plan}-fixture-review.md"));
            let field = if failure == 1 {
                "repository_id"
            } else {
                "workspace_id"
            };
            foreign["operation"][field] = json!("foreign");
            foreign["operation"]["plan"][field] = json!("foreign");
            if rejected {
                foreign["closure"]["compensation"][field] = json!("foreign");
            }
            let bytes = if failure == 0 {
                b"{".to_vec()
            } else {
                serde_json::to_vec(&foreign).unwrap()
            };
            fs::write(&other_path, &bytes).unwrap();
            assert_ne!(cli(fixture.root(), &["graph", "status"]).0, 0);
            assert_ne!(cli(fixture.root(), &["plan", "show", "--id", PLAN]).0, 0);
            let result = client
                .peer()
                .call_tool(CallToolRequestParams::new("mine_graph_status"))
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true), "{result:?}");
            assert_eq!(fs::read(&other_path).unwrap(), bytes);
            assert_eq!(fixture.graph(), graph);
        }
        client.cancel().await.unwrap();
    }
}

#[test]
fn actual_cli_closure_begin_refuses_forged_missing_duplicate_and_stale_assertions_without_intent() {
    let fixture = Fixture::new(None, true);
    let target = fixture.implemented_subject();
    let graph = fixture.graph();
    for variant in 0..10 {
        let mut assertion = fixture.closure_assertion();
        match variant {
            0 => assertion["writer_owner"] = json!("foreign"),
            1 => assertion["writer_run_id"] = json!("stale-run"),
            2 => assertion["reviewer"] = json!("executor"),
            3 => assertion["independent_review"] = json!(false),
            4 => assertion["writer_stopped"] = json!(false),
            5 => assertion["plan"]["body_sha256"] = json!("0".repeat(64)),
            6 => assertion["stop_basis"] = json!(""),
            _ => {}
        }
        let mut fence = format!("```mine-review-closure\n{assertion}\n```\n");
        match variant {
            7 => fence = format!("{fence}\n{fence}"),
            8 => fence = fence.replace("mine-review-closure", "mine-review-closure extra"),
            9 => fence = "no closure attestation".into(),
            _ => {}
        }
        fixture.closure_input(&target, &fence, false);
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let journal = journal_snapshot(fixture.root());
        let (code, value) = fixture.begin_closure("3");
        assert_ne!(code, 0, "variant={variant}: {value}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(journal_snapshot(fixture.root()), journal);
    }
    let trace = fixture
        .root()
        .join(".mine/runtime/closure-stale-trace.jsonl");
    let (_, stale) = cli_observed(
        fixture.root(),
        &[
            "operation",
            "begin",
            "--expected-revision",
            "2",
            "--input",
            "missing.json",
        ],
        Some(&trace),
    );
    assert_eq!(stale["error"]["code"], "MINE_REVISION_CONFLICT");
    for line in fs::read_to_string(trace).unwrap().lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        if event["event"] == "start" {
            for forbidden in ["cat-file", "status", "merge-base", "for-each-ref"] {
                assert!(
                    !event["argv"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|arg| arg == forbidden),
                    "stale caller launched {event}"
                );
            }
        }
    }
}

#[test]
fn actual_cli_closure_begin_preserves_raw_body_identity_under_autocrlf_attributes() {
    for matching_raw_bytes in [false, true] {
        let fixture = Fixture::new(None, true);
        git(fixture.root(), &["config", "core.autocrlf", "true"]);
        let target = fixture.implemented_subject();
        if !matching_raw_bytes {
            // Canonical -text policy intentionally prevents automatic CRLF
            // drift. Construct a real, clean reviewed checkout with contrary
            // raw bytes after valid setup; do not remove the negative branch
            // or weaken the production byte-preservation policy to induce it.
            let body = fs::read_to_string(target.join(PLAN_PATH)).unwrap();
            assert!(!body.contains('\r'));
            fs::write(target.join(PLAN_PATH), body.replace('\n', "\r\n")).unwrap();
            git(&target, &["add", "--", PLAN_PATH]);
            git(
                &target,
                &["commit", "-m", "adversarial reviewed raw-body drift"],
            );
            assert!(git(&target, &["status", "--porcelain=v1", "-uall"]).is_empty());
        }
        let root_body = fs::read(fixture.root().join(PLAN_PATH)).unwrap();
        let checkout_body = fs::read(target.join(PLAN_PATH)).unwrap();
        assert_eq!(root_body == checkout_body, matching_raw_bytes);
        let fence = format!(
            "```mine-review-closure\n{}\n```",
            fixture.closure_assertion()
        );
        fixture.closure_input(&target, &fence, false);
        let graph = fixture.graph();
        let journal = journal_snapshot(fixture.root());
        let (code, value) = fixture.begin_closure("3");
        if matching_raw_bytes {
            assert_eq!(code, 0, "{value}");
        } else {
            assert_ne!(code, 0, "{value}");
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Plan body differs"),
                "{value}"
            );
            assert_eq!(journal_snapshot(fixture.root()), journal);
        }
        assert_eq!(fixture.graph(), graph);
        assert_eq!(fs::read(fixture.root().join(PLAN_PATH)).unwrap(), root_body);
        assert_eq!(fs::read(target.join(PLAN_PATH)).unwrap(), checkout_body);
    }
}

#[test]
fn actual_cli_closure_stage_admits_real_unknown_input_run_and_retains_it_through_cleanup() {
    use mine::infrastructure::verification_store::FileVerificationStore;
    let fixture = Fixture::new(None, true);
    let (integrated, context) = fixture.integrated_closure(true);
    let (receipt, claim, _) = fixture.gate_run(&integrated, &context);
    let claim_identity = retained_claim(fixture.root(), &claim);
    let mut verified = integrated.clone();
    verified["stage"] = json!({"kind":"acceptance","stage":"verified"});
    verified["stage_revision"] = json!(2);
    verified["verification_evidence"] = json!([receipt, claim_identity]);
    let gate = json!({"checks":[{"check":"probe","receipt":receipt,"context":context,"fresh_observation":claim_identity}]});
    let trace = fixture.root().join(".mine/runtime/gate-trace.jsonl");
    let (code, admitted) = fixture.stage_closure_observed(&verified, 1, gate.clone(), Some(&trace));
    assert_eq!(code, 0, "{admitted}");
    assert_eq!(admitted["data"]["checks"][0]["admission"], "fresh_run");
    let starts = fs::read_to_string(&trace)
        .unwrap()
        .lines()
        .filter(|line| {
            let event: Value = serde_json::from_str(line).unwrap();
            event["event"] == "start"
        })
        .count();
    println!("verified_gate_git_starts={starts}; one_check_one_receipt_one_claim_one_log");
    let operation_id = integrated["operation_id"].as_str().unwrap();
    let journal = fixture
        .root()
        .join(format!(".mine/runtime/operations/{operation_id}.json"));
    let recorded = fs::read(&journal).unwrap();
    let graph = fixture.graph();
    let (code, replay) = fixture.stage_closure(&verified, 1, gate.clone());
    assert_eq!(code, 0, "{replay}");
    assert_eq!(
        replay["data"]["checks"][0]["admission"],
        "retained_gate_run"
    );
    assert_eq!(fs::read(&journal).unwrap(), recorded);
    assert_eq!(fixture.graph(), graph);
    let check = json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context,"independent_of":"executor"});
    let (code, conservative) = fixture.verification("check", &check, receipt["path"].as_str());
    assert_eq!(code, 0, "{conservative}");
    assert_eq!(
        conservative["data"]["decision"]["disposition"],
        "NON_REUSABLE"
    );
    let plan: PlanIdentity = serde_json::from_value(verified["plan"].clone()).unwrap();
    let store = FileVerificationStore::new(fixture.root(), fixture.root()).unwrap();
    assert!(store.cleanup_outputs(&plan).unwrap());
    let target = fixture.root().join(format!(".mine/worktrees/{PLAN}"));
    git(
        fixture.root(),
        &["worktree", "remove", target.to_str().unwrap()],
    );
    git(
        fixture.root(),
        &["branch", "-d", &format!("plan/{PLAN}-fixture")],
    );
    let mut cleaned = verified;
    cleaned["stage"] = json!({"kind":"acceptance","stage":"cleaned"});
    cleaned["stage_revision"] = json!(3);
    cleaned["cleanup"] = json!({"worktree":format!(".mine/worktrees/{PLAN}"),"branch":format!("plan/{PLAN}-fixture"),"worktree_removed":true,"branch_removed":true});
    let (code, result) = fixture.stage_closure(&cleaned, 2, gate);
    assert_eq!(code, 0, "{result}");
    assert_eq!(
        result["data"]["checks"][0]["admission"],
        "retained_gate_run"
    );
    assert_eq!(fixture.graph(), graph);
}

#[test]
fn actual_cli_closure_stage_rejects_false_fresh_claims_and_incomplete_coverage_without_advancing() {
    let fixture = Fixture::new(None, true);
    let (integrated, context) = fixture.integrated_closure(true);
    let (receipt, claim, _) = fixture.gate_run(&integrated, &context);
    let id = integrated["operation_id"].as_str().unwrap();
    let journal = fixture
        .root()
        .join(format!(".mine/runtime/operations/{id}.json"));
    let original_journal = fs::read(&journal).unwrap();
    let graph = fixture.graph();
    for variant in 0..14 {
        let mut changed = claim.clone();
        match variant {
            0 => changed["operation_id"] = json!(uuid::Uuid::new_v4().to_string()),
            1 => changed["target_stage"] = json!({"kind":"acceptance","stage":"cleaned"}),
            2 => changed["target_stage_revision"] = json!(1),
            3 => changed["expected_graph_revision"] = json!(2),
            4 => changed["check"] = json!("different"),
            5 => changed["attested_by"] = json!("executor"),
            6 => changed["capture"]["context"]["platform"] = json!("foreign-platform"),
            7 => changed["observation"]["outcome"] = json!("failed"),
            8 => changed["observation"]["warnings_observed"] = json!(true),
            9 => changed["attested_at"] = json!("2999-01-01T00:00:00Z"),
            10 => changed["capture"]["manifest"]["root_mode"] = json!(999),
            _ => {}
        }
        let identity = retained_claim(fixture.root(), &changed);
        let mut verified = integrated.clone();
        verified["stage"] = json!({"kind":"acceptance","stage":"verified"});
        verified["stage_revision"] = json!(2);
        verified["verification_evidence"] = json!([receipt, identity]);
        let mut gate = json!({"checks":[{"check":"probe","receipt":receipt,"context":context,"fresh_observation":identity}]});
        match variant {
            11 => gate["checks"][0]["fresh_observation"] = Value::Null,
            12 => gate["checks"] = json!([]),
            13 => gate["checks"][0]["context"]["platform"] = json!("wrong-native-platform"),
            _ => {}
        }
        let (code, result) = fixture.stage_closure(&verified, 1, gate);
        assert_ne!(code, 0, "variant={variant}: {result}");
        assert_eq!(fs::read(&journal).unwrap(), original_journal);
        assert_eq!(fixture.graph(), graph);
    }
    // These are explicit tampered negative fixtures, not assertions of real
    // successful executions. Keep capture/context consistent to exercise the
    // checks that generic NON_REUSABLE/unobservable_inputs returns before.
    let original: Value = serde_json::from_slice(
        &fs::read(fixture.root().join(receipt["path"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    for variant in 0..4 {
        let mut stored = original.clone();
        let receipt_id = uuid::Uuid::new_v4().to_string();
        stored["receipt"]["receipt_id"] = json!(receipt_id);
        let mut changed = claim.clone();
        let mut current_context = context.clone();
        match variant {
            0 => {
                stored["receipt"]["actor"] = json!("executor");
                changed["capture"]["actor"] = json!("executor");
                changed["attested_by"] = json!("executor");
            }
            1 => {
                stored["receipt"]["platform"] = json!("foreign-platform");
                changed["capture"]["context"]["platform"] = json!("foreign-platform");
                current_context["platform"] = json!("foreign-platform");
            }
            2 => stored["inputs_stable"] = json!(false),
            3 => {
                stored["receipt"]["outcome"] = json!("failed");
                stored["receipt"]["exit_code"] = json!(1);
                changed["observation"]["outcome"] = json!("failed");
                changed["observation"]["exit_code"] = json!(1);
            }
            _ => unreachable!(),
        }
        let relative = format!(".mine/plan/reports/verification/{PLAN}/{receipt_id}.json");
        fs::write(
            fixture.root().join(&relative),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
        git(fixture.root(), &["add", &relative]);
        git(
            fixture.root(),
            &["commit", "-m", "explicit invalid run fixture"],
        );
        let receipt = file_json(fixture.root(), &relative);
        changed["receipt"] = receipt.clone();
        let identity = retained_claim(fixture.root(), &changed);
        let mut verified = integrated.clone();
        verified["stage"] = json!({"kind":"acceptance","stage":"verified"});
        verified["stage_revision"] = json!(2);
        verified["verification_evidence"] = json!([receipt, identity]);
        let gate = json!({"checks":[{"check":"probe","receipt":receipt,"context":current_context,"fresh_observation":identity}]});
        let (code, result) = fixture.stage_closure(&verified, 1, gate);
        assert_ne!(code, 0, "invalid-run variant={variant}: {result}");
        assert_eq!(fs::read(&journal).unwrap(), original_journal);
        assert_eq!(fixture.graph(), graph);
    }
    // Contrary writer activity after integration cannot be hidden by ancestry.
    let target = fixture.root().join(format!(".mine/worktrees/{PLAN}"));
    fs::write(
        target.join("src/fixture/implementation.txt"),
        "contrary late writer\n",
    )
    .unwrap();
    let mut operation = integrated.clone();
    operation["stage_revision"] = json!(2);
    let (code, result) = fixture.stage_closure(&operation, 1, Value::Null);
    assert_ne!(code, 0, "{result}");
    assert_eq!(fs::read(&journal).unwrap(), original_journal);
    assert_eq!(fixture.graph(), graph);
}

#[test]
fn actual_cli_acceptance_finalize_proves_terminal_delta_and_same_identity_recovery_without_more_runs()
 {
    let mut fixture = Fixture::new(None, true);
    fixture.blocked_successor = true;
    let (cleaned, gate, receipt, context) = fixture.cleaned_closure(false);
    let root = fixture.root();
    let master = git(root, &["rev-parse", "master"]);
    let before_head = git(root, &["rev-parse", "HEAD"]);
    let id = cleaned["operation_id"].as_str().unwrap();
    let journal = root.join(format!(".mine/runtime/operations/{id}.json"));
    let cleaned_journal = fs::read(&journal).unwrap();
    let receipt_bytes = fs::read(root.join(receipt["path"].as_str().unwrap())).unwrap();
    let mut accepted = cleaned;
    accepted["stage"] = json!({"kind":"acceptance","stage":"accepted"});
    accepted["stage_revision"] = json!(4);
    accepted["outcome"] = json!("committed");
    let (code, first) = fixture.finalize_closure(&accepted, &gate, "3");
    assert_eq!(code, 0, "{first}");
    assert_eq!(first["revision_before"], 3);
    assert_eq!(first["revision_after"], 4);
    assert_eq!(first["data"]["replay"], false);
    let head = git(root, &["rev-parse", "HEAD"]);
    assert_ne!(head, before_head);
    assert_eq!(git(root, &["rev-parse", "HEAD^"]), before_head);
    assert_eq!(git(root, &["rev-parse", "master"]), master);
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    assert_eq!(workspace.revision, 4);
    let successor = workspace.get(&format!("{PLAN}-01")).unwrap();
    assert_eq!(successor.status, mine::domain::status::PlanStatus::Ready);
    assert_eq!(successor.hard_predecessors, vec![PLAN]);
    assert_eq!(
        workspace.get(PLAN).unwrap().status,
        mine::domain::status::PlanStatus::Accepted
    );
    assert_eq!(
        serde_json::to_value(workspace.get(PLAN).unwrap().acceptance.as_ref().unwrap()).unwrap(),
        accepted
    );
    let graph = fixture.graph();
    let final_journal = fs::read(&journal).unwrap();
    let (code, check) = fixture.verification("check",&json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context}),receipt["path"].as_str());
    assert_eq!(code, 0, "{check}");
    assert_eq!(check["data"]["decision"]["disposition"], "INVALIDATED");
    assert_eq!(check["data"]["decision"]["reason"], "inputs_changed");
    // Terminal replay is history, with no caller current gate or second RUN.
    let (code, replay) = fixture.finalize_closure(&accepted, &Value::Null, "4");
    assert_eq!(code, 0, "{replay}");
    assert_eq!(replay["data"]["replay"], true);
    assert_eq!(replay["data"]["checks"], json!([]));
    assert_eq!(fs::read(&journal).unwrap(), final_journal);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);

    // Isolated lost-response fixture: retain the real published checkpoint and
    // graph, restore only the saved pre-finalize outer journal. Production can
    // stop in precisely this window; no fake candidate/tip is introduced.
    fs::write(&journal, &cleaned_journal).unwrap();
    fs::write(
        root.join("ignored-late-input"),
        b"known drift after terminal graph publication",
    )
    .unwrap();
    let excludes = root.join(".git/info/exclude");
    let mut exclude_bytes = fs::read(&excludes).unwrap();
    exclude_bytes.extend_from_slice(b"\nignored-late-input\n");
    fs::write(excludes, exclude_bytes).unwrap();
    assert!(git(root, &["status", "--porcelain"]).is_empty());
    let (code, drift) = fixture.finalize_closure(&accepted, &gate, "4");
    assert_ne!(code, 0, "{drift}");
    assert_eq!(fs::read(&journal).unwrap(), cleaned_journal);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    fs::remove_file(root.join("ignored-late-input")).unwrap();
    let checkpoint_path = root.join(format!(
        ".mine/runtime/operations/{}.json",
        first["data"]["checkpoint_operation_id"].as_str().unwrap()
    ));
    let checkpoint_bytes = fs::read(&checkpoint_path).unwrap();
    let mut altered: Value = serde_json::from_slice(&checkpoint_bytes).unwrap();
    let mut unauthorized: PlanWorkspace =
        toml::from_str(altered["checkpoint"]["source_after"].as_str().unwrap()).unwrap();
    unauthorized.get_mut(PLAN).unwrap().title = "unauthorized extra graph change".into();
    let source_after = toml::to_string(&unauthorized).unwrap();
    let view_after = render_markdown(&unauthorized).unwrap();
    altered["operation"]["checkpoint"]["source_sha256"] = json!(digest(source_after.as_bytes()));
    altered["operation"]["checkpoint"]["view_sha256"] = json!(digest(view_after.as_bytes()));
    altered["checkpoint"]["source_after"] = json!(source_after);
    altered["checkpoint"]["view_after"] = json!(view_after);
    let altered_bytes = serde_json::to_vec(&altered).unwrap();
    fs::write(&checkpoint_path, &altered_bytes).unwrap();
    let (code, broader) = fixture.finalize_closure(&accepted, &gate, "4");
    assert_ne!(code, 0, "{broader}");
    assert!(
        broader["error"]["message"]
            .as_str()
            .unwrap()
            .contains("terminal graph delta"),
        "{broader}"
    );
    assert_eq!(fs::read(&checkpoint_path).unwrap(), altered_bytes);
    assert_eq!(fs::read(&journal).unwrap(), cleaned_journal);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    fs::write(&checkpoint_path, &checkpoint_bytes).unwrap();
    let (code, recovered) = fixture.finalize_closure(&accepted, &gate, "4");
    assert_eq!(code, 0, "{recovered}");
    assert_eq!(
        recovered["data"]["checkpoint_operation_id"],
        first["data"]["checkpoint_operation_id"]
    );
    assert_eq!(
        recovered["data"]["checks"][0]["admission"],
        "retained_gate_run"
    );
    assert_eq!(fs::read(&journal).unwrap(), final_journal);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(root.join(receipt["path"].as_str().unwrap())).unwrap(),
        receipt_bytes
    );
}

#[test]
fn actual_cli_verification_unknown_inputs_never_hide_known_source_context_lock_or_artifact_drift() {
    let fixture = Fixture::new(None, true);
    assert_eq!(fixture.register("0").0, 0);
    let mut context = fixture.verification_policy();
    let root = fixture.root();
    let policy_path = root.join(".mine/verification-policy.json");
    let mut policy: Value = serde_json::from_slice(&fs::read(&policy_path).unwrap()).unwrap();
    policy["checks"][0]["environment"] =
        json!([{"name":"external_data","observable":false,"secret_dependent":false}]);
    policy["checks"][0]["dependency_locks"] = json!(["probe.lock"]);
    context["environment"] =
        json!([{"name":"external_data","sha256":null,"observable":false,"secret_dependent":false}]);
    fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
    fs::write(root.join("probe.lock"), b"original dependency lock").unwrap();
    git(
        root,
        &["add", ".mine/verification-policy.json", "probe.lock"],
    );
    git(
        root,
        &[
            "commit",
            "-m",
            "explicit observable and unknown input fixture",
        ],
    );
    let (receipt, _, observation, _) = fixture.verification_run(&context);
    let check = |current: &Value| {
        let (code,value) = fixture.verification("check",&json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":current}),receipt["path"].as_str());
        assert_eq!(code, 0, "{value}");
        value["data"]["decision"].clone()
    };
    let unchanged = check(&context);
    assert_eq!(unchanged["disposition"], "NON_REUSABLE");
    assert_eq!(unchanged["reason"], "unobservable_inputs");
    fs::write(root.join("new-relevant-source"), b"known source change").unwrap();
    let changed = check(&context);
    assert_eq!(changed["disposition"], "INVALIDATED");
    assert_eq!(changed["reason"], "inputs_changed");
    fs::remove_file(root.join("new-relevant-source")).unwrap();
    let mut changed_context = context.clone();
    changed_context["toolchains"]["git"] = json!(digest(b"changed observed toolchain"));
    assert_eq!(check(&changed_context)["reason"], "context_changed");
    fs::write(root.join("probe.lock"), b"changed dependency lock").unwrap();
    assert_eq!(check(&context)["reason"], "inputs_changed");
    fs::write(root.join("probe.lock"), b"original dependency lock").unwrap();
    let artifact = root.join(observation["artifacts"][0]["path"].as_str().unwrap());
    let original_artifact = fs::read(&artifact).unwrap();
    fs::write(&artifact, b"changed retained artifact").unwrap();
    assert_eq!(check(&context)["reason"], "artifacts_changed");
    fs::write(&artifact, &original_artifact).unwrap();
    assert_eq!(check(&context)["disposition"], "NON_REUSABLE");
}

#[test]
fn actual_cli_acceptance_finalize_retains_unknown_gate_run_but_invalidates_known_post_tree_drift() {
    let fixture = Fixture::new(None, true);
    let (mut accepted, gate, receipt, context) = fixture.cleaned_closure(true);
    accepted["stage"] = json!({"kind":"acceptance","stage":"accepted"});
    accepted["stage_revision"] = json!(4);
    accepted["outcome"] = json!("committed");
    let (code, value) = fixture.finalize_closure(&accepted, &gate, "3");
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["checks"][0]["admission"], "retained_gate_run");
    let graph = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let (code, value) = fixture.verification("check",&json!({"schema_version":1,"plan_id":PLAN,"actor":"reviewer","check":"probe","context":context,"independent_of":"executor"}),receipt["path"].as_str());
    assert_eq!(code, 0, "{value}");
    // Unknown inputs never hide a known source-key change caused by the exact
    // terminal transition. This remains a standalone check, not acceptance.
    assert_eq!(
        value["data"]["decision"]["disposition"], "INVALIDATED",
        "{value}"
    );
    assert_eq!(value["data"]["decision"]["reason"], "inputs_changed");
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn compensation_rejected_tip_cannot_contain_a_plan_body_created_later_on_root() {
    // This is a real-Git topology counterexample, not a fabricated native
    // preparation/handoff. It models the two approved inputs before deciding
    // how their missing-body boundary is resolved.
    let fixture = Fixture::new(None, true);
    fixture.verification_policy();
    let target = fixture.implemented_subject();
    let fence = format!(
        "```mine-review-closure\n{}\n```",
        fixture.closure_assertion()
    );
    let request = fixture.closure_input(&target, &fence, true);
    let tip = request["intent"]["review"]["reviewed_tip"]
        .as_str()
        .unwrap();
    let path = request["intent"]["compensation"]["plan_path"]
        .as_str()
        .unwrap();
    let branch = request["intent"]["compensation"]["branch"]
        .as_str()
        .unwrap();
    git(&target, &["switch", "-c", branch, tip]);
    assert_eq!(git(&target, &["rev-parse", "HEAD"]), tip);
    assert_eq!(
        git(
            fixture.root(),
            &["rev-parse", &format!("plan/{PLAN}-fixture")]
        ),
        tip
    );
    assert_eq!(git(fixture.root(), &["branch", "--show-current"]), "dev");
    assert!(fixture.root().join(path).is_file());
    assert!(!target.join(path).exists());
    assert!(git(&target, &["ls-tree", "--full-tree", tip, "--", path]).is_empty());
    println!(
        "root_created_compensation_body_is_absent_at_exact_rejected_tip; Root remains dev and source branch preserved"
    );
}

#[tokio::test]
async fn actual_mcp_accept_and_cli_replay_share_native_finalization_without_report_only_bypass() {
    use rmcp::{
        model::CallToolRequestParams, service::serve_client,
        transport::child_process::TokioChildProcess,
    };
    let fixture = Fixture::new(None, true);
    let (mut accepted, gate, _, _) = fixture.cleaned_closure(false);
    accepted["stage"] = json!({"kind":"acceptance","stage":"accepted"});
    accepted["stage_revision"] = json!(4);
    accepted["outcome"] = json!("committed");
    fs::write(fixture.root().join(CLOSURE_INPUT),serde_json::to_vec(&json!({"schema_version":1,"expected_stage_revision":3,"operation":accepted,"verification":gate})).unwrap()).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
    command.current_dir(fixture.root()).args(["mcp", "serve"]);
    let client = serve_client((), TokioChildProcess::builder(command).spawn().unwrap().0)
        .await
        .unwrap();
    let inventory = client.peer().list_tools(Default::default()).await.unwrap();
    let tool = inventory
        .tools
        .iter()
        .find(|tool| tool.name == "mine_plan_accept")
        .unwrap();
    let schema = serde_json::to_value(&tool.input_schema).unwrap();
    for field in ["expected_revision", "id", "operation_id", "input"] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == field),
            "{schema}"
        );
    }
    assert!(schema["properties"].get("review").is_none());
    let arguments = json!({"expected_revision":3,"id":PLAN,"operation_id":accepted["operation_id"],"input":CLOSURE_INPUT});
    let graph = fixture.graph();
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    for (field, value) in [
        // mine-release-allow-plan-reference: isolated forged-request fixture, not repository Plan provenance
        ("id", json!("plan99")),
        ("operation_id", json!(uuid::Uuid::new_v4().to_string())),
        ("expected_revision", json!(2)),
    ] {
        let mut invalid = arguments.clone();
        invalid[field] = value;
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("mine_plan_accept")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true), "{result:?}");
        assert_eq!(fixture.graph(), graph);
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    }
    let result = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_plan_accept")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let value = result.structured_content.as_ref().unwrap();
    assert_eq!(value["revision_before"], 3);
    assert_eq!(value["revision_after"], 4);
    assert_eq!(value["data"]["operation"], accepted);
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let graph = fixture.graph();
    fs::write(fixture.root().join(CLOSURE_INPUT),serde_json::to_vec(&json!({"schema_version":1,"expected_stage_revision":3,"operation":accepted,"verification":null})).unwrap()).unwrap();
    let (code, replay) = cli(
        fixture.root(),
        &[
            "plan",
            "accept",
            "--id",
            PLAN,
            "--operation-id",
            accepted["operation_id"].as_str().unwrap(),
            "--input",
            CLOSURE_INPUT,
            "--expected-revision",
            "4",
        ],
    );
    assert_eq!(code, 0, "{replay}");
    assert_eq!(replay["data"]["operation"], accepted);
    assert_eq!(replay["revision_before"], 4);
    assert_eq!(replay["revision_after"], 4);
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    let (code, legacy) = cli(
        fixture.root(),
        &[
            "plan",
            "accept",
            "--id",
            PLAN,
            "--review",
            "report.md",
            "--expected-revision",
            "4",
        ],
    );
    assert_ne!(code, 0, "{legacy}");
    assert_eq!(fixture.graph(), graph);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn actual_mcp_policy_registration_requires_caller_revision_and_matches_cli_provenance() {
    use rmcp::{
        model::CallToolRequestParams, service::serve_client,
        transport::child_process::TokioChildProcess,
    };
    let fixture = Fixture::new(None, true);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
    command.current_dir(fixture.root()).args(["mcp", "serve"]);
    let transport = TokioChildProcess::builder(command).spawn().unwrap().0;
    let client = serve_client((), transport).await.unwrap();
    let arguments = json!({"actor":"fixture-registrar","id":PLAN,"path":PLAN_PATH,"title":"Fixture","design_references":[".mine/design/index.md"],"ownership_input":INPUT_PATH});
    let before = fixture.graph();
    let missing = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_plan_add")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(missing.is_error, Some(true));
    assert_eq!(fixture.graph(), before);
    let mut arguments = arguments;
    arguments["expected_revision"] = json!(0);
    let result = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_plan_add")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&fixture.graph()).unwrap()).unwrap();
    let provenance = workspace
        .get(PLAN)
        .unwrap()
        .ownership_policy
        .as_ref()
        .unwrap();
    assert_eq!(provenance.approval_actor, "owner");
    assert_eq!(provenance.admitted_graph_revision, 0);
    assert_eq!(provenance.adr_registry_revision, 3);
    assert_eq!(workspace.revision, 1);
    assert!(
        workspace
            .get(PLAN)
            .unwrap()
            .exclusive_write_paths
            .is_empty()
    );
    let shown = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_plan_show")
                .with_arguments(json!({"id":PLAN}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(shown.is_error, Some(true), "{shown:?}");
    let data = shown.structured_content.unwrap();
    assert_eq!(data["data"]["plan"]["status"], "DRAFT");
    assert_eq!(
        data["data"]["plan"]["effective_authority"]["policy"]["sha256"],
        json!(digest(&fixture.policy))
    );
    client.cancel().await.unwrap();
}
