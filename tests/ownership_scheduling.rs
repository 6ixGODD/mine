#![forbid(unsafe_code)]
use mine::domain::graph::*;
use mine::domain::ownership::*;
use mine::domain::scheduling::*;
use mine::domain::status::PlanStatus;
use serde_json::json;

const REPOSITORY_ID: &str = "c2925a22-523c-455e-8772-57cbcc615887";

fn path(s: &str) -> RepoPath {
    RepoPath::new(s).unwrap()
}
fn ns(s: &str) -> Namespace {
    Namespace::new(s).unwrap()
}
fn id(n: usize) -> String {
    format!("plan{n:02}")
}
fn identity(value: &str) -> PlanIdentity {
    let lineage = mine::domain::plan_id::CanonicalPlanId::parse(value)
        .unwrap()
        .lineage_id();
    PlanIdentity::new(PlanIdentityFields {
        repository_id: REPOSITORY_ID.into(),
        workspace_id: "workspace".into(),
        plan_id: value.into(),
        plan_path: path(&format!("docs/plan/{value}-fixture.md")),
        body_sha256: Some(hash_bytes(b"immutable")),
        lineage_id: lineage,
        branch: format!("plan/{value}-fixture"),
    })
    .unwrap()
}
fn generator() -> GeneratorTransaction {
    GeneratorTransaction {
        name: "payload".into(),
        inputs: vec![path("policy/source.json")],
        machine_source: path("policy/source.json"),
        outputs: vec![path("dist/a"), path("dist/seal")],
        seals: vec![path("dist/seal")],
    }
}
fn document(n: usize) -> OwnershipPolicyDocument {
    let shared: Vec<_> = (1..=n)
        .map(|i| SharedDeclaration {
            path: path("src/exports.rs"),
            entry: format!("export-{i}"),
            class: DeclarationClass::Export,
            integration_owner: "integrator".into(),
        })
        .collect();
    OwnershipPolicyDocument {
        schema_version: 1,
        clarification_actors: vec!["owner".into()],
        mappings: (1..=n)
            .map(|i| ResponsibilityMapping {
                responsibility: format!("responsibility-{i}"),
                namespaces: vec![NamespaceClaim {
                    subtree: ns("app"),
                    excluded: vec![],
                }],
                contracts: vec![format!("contract-{i}"), "shared-contract".into()],
                physical_prefixes: vec![path("src")],
                shared_entries: vec![SharedEntryKey {
                    path: shared[i - 1].path.clone(),
                    entry: shared[i - 1].entry.clone(),
                }],
                generator_names: vec!["payload".into()],
            })
            .collect(),
        partitions: vec![],
        shared_declarations: shared,
        generators: vec![generator()],
    }
}
fn approve(doc: &OwnershipPolicyDocument) -> ApprovedOwnershipPolicy {
    let bytes = serde_json::to_vec(doc).unwrap();
    ApprovedOwnershipPolicy::verify(
        &bytes,
        &PolicyApproval {
            identity: PolicyIdentity::new(PolicyIdentityFields {
                design_references: vec![path("docs/design/governance/semantic-ownership.md")],
                sha256: hash_bytes(&bytes),
            })
            .unwrap(),
            actor: "owner".into(),
            observed_revision: 1,
        },
        7,
    )
    .unwrap()
}
fn authority(p: &ApprovedOwnershipPolicy, n: usize) -> EffectiveAuthority {
    let contract = OwnershipContract::new(OwnershipFields {
        schema_version: 1,
        plan: identity(&id(n)),
        policy: p.identity().clone(),
        responsibility: format!("responsibility-{n}"),
        namespace_claims: vec![NamespaceClaim {
            subtree: ns(&format!("app.area{n}")),
            excluded: vec![],
        }],
        owned_contracts: vec![format!("contract-{n}")],
        excluded_responsibilities: vec![],
        excluded_contracts: vec![],
        physical_mapping: vec![path(&format!("src/area{n}"))],
        shared_declarations: vec![],
        generators: vec![],
    })
    .unwrap();
    resolve(contract, p)
}
fn resolve(c: OwnershipContract, p: &ApprovedOwnershipPolicy) -> EffectiveAuthority {
    AuthorityResolver::resolve(&c, b"immutable", &c.fields().plan, p, 7, &[]).unwrap()
}
fn alter(
    a: &EffectiveAuthority,
    p: &ApprovedOwnershipPolicy,
    change: impl FnOnce(&mut OwnershipFields),
) -> EffectiveAuthority {
    let mut f = a.semantic().unwrap().fields().clone();
    change(&mut f);
    resolve(OwnershipContract::new(f).unwrap(), p)
}
fn nodes(p: &ApprovedOwnershipPolicy, n: usize) -> Vec<NodeOwnership> {
    (1..=n)
        .map(|i| NodeOwnership {
            authority: authority(p, i),
            worktree: path(&format!(".mine/worktrees/{}", id(i))),
            mutable_outputs: vec![path(&format!(".mine/build/{}/cargo-target", id(i)))],
            closure: ClosureFacts::Settled,
        })
        .collect()
}
fn graph(nodes: &[NodeOwnership], statuses: &[PlanStatus]) -> PlanWorkspace {
    let plans:Vec<_>=nodes.iter().zip(statuses).map(|(n,status)| {
        let p=n.authority.plan().fields();
        json!({"id":p.plan_id,"path":p.plan_path,"title":p.plan_id,"branch":p.branch,"status":status,
            "hard_predecessors":[],"design_references":["docs/design/principles.md"],"lineage_id":p.lineage_id,"worktree_path":n.worktree,
            "owner":"executor","run_id":"run","started_at":"","updated_at":"","rejection_reason":"","compensating_plan":"",
            "implementation_report":"","review_report":"","implementation_commits":[]})
    }).collect();
    serde_json::from_value(json!({"schema_version":"mine-graph-v4","revision":7,"project_id":"repository","workspace_id":"workspace",
        "stable_branch":"master","integration_branch":"dev","stable_baseline_commit":"","design_root":"docs/design/index.md",
        "ephemeral_workspace":true,"purge_before_stable_release":true,"plans":plans})).unwrap()
}
fn kernel<'a>(
    g: &'a PlanWorkspace,
    p: &'a ApprovedOwnershipPolicy,
    n: Vec<NodeOwnership>,
) -> SchedulingKernel<'a> {
    SchedulingKernel::new(
        REPOSITORY_ID,
        g,
        hash_bytes(b"observed graph bytes"),
        Some(p),
        n,
        None,
    )
    .unwrap()
}
fn has(blockers: &[Blocker], reason: BlockingReason) -> bool {
    blockers.iter().any(|b| b.reason == reason)
}

#[test]
fn repository_uuid_is_explicit_and_project_label_and_graph_bytes_are_unchanged() {
    let policy = approve(&document(1));
    let ownership = nodes(&policy, 1);
    let mut workspace = graph(&ownership, &[PlanStatus::Ready]);
    workspace.project_id = "human-project-slug".into();
    let bytes = toml::to_string(&workspace).unwrap();
    let digest = hash_bytes(bytes.as_bytes());
    let wave = SchedulingKernel::new(
        REPOSITORY_ID,
        &workspace,
        digest.clone(),
        Some(&policy),
        ownership.clone(),
        None,
    )
    .unwrap()
    .wave()
    .unwrap();
    assert_eq!(wave.repository_id, REPOSITORY_ID);
    assert_eq!(wave.graph_sha256, digest);
    assert_eq!(toml::to_string(&workspace).unwrap(), bytes);
    assert_eq!(workspace.project_id, "human-project-slug");
    assert!(
        SchedulingKernel::new(
            "793a8985-b7b3-4197-a644-895d89d6e6aa",
            &workspace,
            digest,
            Some(&policy),
            ownership,
            None,
        )
        .is_err()
    );
}

fn conservative_nodes(count: usize) -> Vec<NodeOwnership> {
    (1..=count)
        .map(|i| NodeOwnership {
            authority: EffectiveAuthority::conservative(
                identity(&id(i)),
                vec![path(&format!("src/area{i}"))],
            )
            .unwrap(),
            worktree: path(&format!(".mine/worktrees/{}", id(i))),
            mutable_outputs: vec![path(&format!(".mine/build/{}", id(i)))],
            closure: ClosureFacts::Settled,
        })
        .collect()
}

#[test]
fn policy_free_conservative_wave_and_admission_use_complete_held_resources() {
    let base = conservative_nodes(2);
    let graph = graph(&base, &[PlanStatus::InProgress, PlanStatus::Ready]);
    let bytes = toml::to_string(&graph).unwrap();
    let kernel = SchedulingKernel::new(
        REPOSITORY_ID,
        &graph,
        hash_bytes(bytes.as_bytes()),
        None,
        base.clone(),
        None,
    )
    .unwrap();
    assert!(kernel.admit(&id(2), 7).unwrap().admissible());
    let wave = kernel.wave().unwrap();
    assert_eq!(wave.authority_mode, AuthorityMode::Conservative);
    assert!(wave.policy.is_none());
    assert!(
        wave.nodes
            .iter()
            .all(|n| n.ownership.authority.policy().is_none())
    );
    for variant in 0..3 {
        let mut nodes = base.clone();
        let expected = match variant {
            0 => {
                nodes[1].authority = EffectiveAuthority::conservative(
                    identity(&id(2)),
                    vec![path("src/area1/child")],
                )
                .unwrap();
                BlockingReason::PhysicalOverlap
            }
            1 => {
                nodes[1].worktree = nodes[0].worktree.clone();
                BlockingReason::SameWorktree
            }
            _ => {
                nodes[1].mutable_outputs = nodes[0].mutable_outputs.clone();
                BlockingReason::SharedOutput
            }
        };
        // Invalid registered worktree replacement remains a structural refusal.
        let result = SchedulingKernel::new(
            REPOSITORY_ID,
            &graph,
            hash_bytes(bytes.as_bytes()),
            None,
            nodes,
            None,
        );
        if variant == 1 {
            assert!(result.is_err());
        } else {
            assert!(has(
                &result.unwrap().admit(&id(2), 7).unwrap().blockers,
                expected
            ));
        }
    }
    assert_eq!(toml::to_string(&graph).unwrap(), bytes);
}

#[test]
fn missing_semantic_policy_is_an_error_not_conservative_fallback() {
    let policy = approve(&document(2));
    let nodes = nodes(&policy, 2);
    let graph = graph(&nodes, &[PlanStatus::InProgress, PlanStatus::Ready]);
    assert!(
        SchedulingKernel::new(
            REPOSITORY_ID,
            &graph,
            hash_bytes(b"graph"),
            None,
            nodes,
            None
        )
        .is_err()
    );
}

#[test]
fn explicit_first_activation_retains_known_body_and_requires_verified_policy_context() {
    let policy = approve(&document(1));
    let declared = authority(&policy, 1).semantic().unwrap().clone();
    let old = EffectiveAuthority::conservative(
        declared.fields().plan.clone(),
        declared.fields().physical_mapping.clone(),
    )
    .unwrap();
    let nodes = vec![NodeOwnership {
        authority: old,
        worktree: path(&format!(".mine/worktrees/{}", id(1))),
        mutable_outputs: vec![path(&format!(".mine/build/{}", id(1)))],
        closure: ClosureFacts::Settled,
    }];
    let graph = graph(&nodes, &[PlanStatus::Ready]);
    let effective = AuthorityResolver::append(
        &declared,
        b"immutable",
        &policy,
        7,
        &[],
        ClarificationRequest {
            plan: declared.fields().plan.clone(),
            policy: policy.identity().clone(),
            expected_revision: 7,
            expected_tail: None,
            actor: "owner".into(),
            reason: "first explicit activation".into(),
            kind: ClarificationKind::BaselineAttestation,
            physical_mapping: declared.fields().physical_mapping.clone(),
            generators: declared.fields().generators.clone(),
        },
    )
    .unwrap();
    let kernel = SchedulingKernel::new(
        REPOSITORY_ID,
        &graph,
        hash_bytes(b"graph"),
        Some(&policy),
        nodes.clone(),
        None,
    )
    .unwrap();
    assert!(
        kernel
            .admit_clarification(&id(1), effective.clone(), 7)
            .unwrap()
            .admissible()
    );
    assert_eq!(
        effective.plan().fields().body_sha256,
        declared.fields().plan.fields().body_sha256
    );
    let no_policy = SchedulingKernel::new(
        REPOSITORY_ID,
        &graph,
        hash_bytes(b"graph"),
        None,
        nodes,
        None,
    )
    .unwrap();
    assert!(no_policy.admit_clarification(&id(1), effective, 7).is_err());
}

#[test]
fn stable_greedy_wave_checks_selected_as_well_as_held_and_start_uses_same_evaluator() {
    let p = approve(&document(3));
    let mut n = nodes(&p, 3);
    n[1].authority = alter(&n[1].authority, &p, |f| {
        f.namespace_claims[0].subtree = ns("app.area1.child")
    });
    let g = graph(&n, &[PlanStatus::Ready; 3]);
    let k = kernel(&g, &p, n.clone());
    let w = k.wave().unwrap();
    assert_eq!(w.selected, vec![id(1), id(3)]);
    assert_eq!(w.eligible, vec![id(1), id(2), id(3)]);
    let blocker = w.blockers.iter().find(|b| b.candidate == id(2)).unwrap();
    assert_eq!(blocker.owner, id(1));
    assert_eq!(blocker.reason, BlockingReason::NamespaceOverlap);
    assert_eq!(blocker.resource, Resource::Namespace(ns("app.area1.child")));
    assert!(
        k.admit(&id(2), 7).unwrap().admissible(),
        "unselected READY candidate is not already a hold"
    );
    let mut started = g.clone();
    started.plans[0].status = PlanStatus::InProgress;
    let k = kernel(&started, &p, n);
    let start = k.admit(&id(2), 7).unwrap();
    assert_eq!(start.blockers, k.compatibility(&id(2), &id(1)).unwrap());
    assert!(!start.admissible());
    assert!(k.admit(&id(2), 6).is_err());
    assert!(k.admit("unknown", 7).is_err());
}

#[test]
fn declaration_order_not_id_sort_is_deterministic() {
    let p = approve(&document(3));
    let mut n = nodes(&p, 3);
    n[1].authority = alter(&n[1].authority, &p, |f| {
        f.namespace_claims[0].subtree = ns("app.area1")
    });
    n.reverse();
    let g = graph(&n, &[PlanStatus::Ready; 3]);
    let k = kernel(&g, &p, n);
    for _ in 0..10 {
        assert_eq!(k.wave().unwrap().selected, vec![id(3), id(2)]);
    }
}

#[test]
fn every_active_implemented_and_unfinished_review_owner_holds_without_ttl() {
    let p = approve(&document(2));
    for status in [
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Accepted,
    ] {
        let mut n = nodes(&p, 2);
        n[1].mutable_outputs = n[0].mutable_outputs.clone();
        if status == PlanStatus::Accepted {
            n[0].closure = ClosureFacts::ReviewPending;
        }
        let mut g = graph(&n, &[status, PlanStatus::Ready]);
        g.plans[0].started_at = "1990-01-01T00:00:00Z".into();
        let k = kernel(&g, &p, n);
        assert!(!k.admit(&id(2), 7).unwrap().admissible());
        assert_eq!(k.wave().unwrap().held, vec![id(1)]);
        assert!(has(
            &k.wave().unwrap().blockers,
            BlockingReason::SharedOutput
        ));
    }
}

#[test]
fn ordinary_disjoint_review_does_not_globally_block_grants_but_publication_does() {
    let p = approve(&document(2));
    let n = nodes(&p, 2);
    let g = graph(&n, &[PlanStatus::Implemented, PlanStatus::Ready]);
    assert_eq!(
        kernel(&g, &p, n.clone()).wave().unwrap().selected,
        vec![id(2)]
    );
    for stage in [
        PublicationStage::Graph,
        PublicationStage::Render,
        PublicationStage::Checkpoint,
    ] {
        let k = SchedulingKernel::new(
            REPOSITORY_ID,
            &g,
            hash_bytes(b"graph"),
            Some(&p),
            n.clone(),
            Some(PendingPublication {
                operation_id: "operation".into(),
                owner: "root-integrator".into(),
                stage,
            }),
        )
        .unwrap();
        let wave = k.wave().unwrap();
        assert!(wave.selected.is_empty());
        assert!(has(&wave.blockers, BlockingReason::PendingCheckpoint));
        let decision = k.admit(&id(2), 7).unwrap();
        assert!(!decision.admissible());
        assert_eq!(decision.blockers[0].owner, "root-integrator");
        assert_eq!(
            decision.blockers[0].resource,
            Resource::WorkspaceCheckpoint("operation".into())
        );
    }
}

#[test]
fn exclusions_segments_case_and_distinct_paths_do_not_false_conflict() {
    let p = approve(&document(2));
    let mut n = nodes(&p, 2);
    n[0].authority = alter(&n[0].authority, &p, |f| {
        f.namespace_claims = vec![NamespaceClaim {
            subtree: ns("app"),
            excluded: vec![ns("app.area2")],
        }]
    });
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    assert!(
        kernel(&g, &p, n.clone())
            .admit(&id(2), 7)
            .unwrap()
            .admissible()
    );
    n[0].authority = alter(&n[0].authority, &p, |f| {
        f.namespace_claims = vec![NamespaceClaim {
            subtree: ns("app.area20"),
            excluded: vec![],
        }];
        f.physical_mapping = vec![path("src/area20")];
    });
    assert!(
        kernel(&g, &p, n.clone())
            .compatibility(&id(2), &id(1))
            .unwrap()
            .is_empty()
    );
    n[0].authority = alter(&n[0].authority, &p, |f| {
        f.namespace_claims[0].subtree = ns("app.Area2")
    });
    assert!(kernel(&g, &p, n).admit(&id(2), 7).unwrap().admissible());
}

#[test]
fn exclusive_contract_physical_output_and_ancestry_reasons_are_exact() {
    let p = approve(&document(2));
    let mut n = nodes(&p, 2);
    for node in &mut n {
        node.authority = alter(&node.authority, &p, |f| {
            f.owned_contracts = vec!["shared-contract".into()]
        });
    }
    n[1].authority = alter(&n[1].authority, &p, |f| {
        f.physical_mapping = vec![path("src/area1/sub")]
    });
    n[1].mutable_outputs = vec![path(".mine/build")];
    let mut g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    g.plans[1].hard_predecessors = vec![id(1)];
    let k = kernel(&g, &p, n);
    let reasons = k.compatibility(&id(2), &id(1)).unwrap();
    for reason in [
        BlockingReason::ContractOverlap,
        BlockingReason::PhysicalOverlap,
        BlockingReason::SharedOutput,
        BlockingReason::Ancestor,
    ] {
        assert!(has(&reasons, reason));
    }
    assert!(has(
        &k.admit(&id(2), 7).unwrap().blockers,
        BlockingReason::PredecessorNotAccepted
    ));
    assert!(has(
        &k.admit(&id(1), 7).unwrap().blockers,
        BlockingReason::NotReady
    ));
}

#[test]
fn explicitly_approved_directory_partition_and_independent_shared_entries_only() {
    let mut doc = document(2);
    doc.partitions = vec![DirectoryPartition {
        directory: path("src/common"),
        responsibilities: doc
            .mappings
            .iter()
            .map(|m| m.responsibility.clone())
            .collect(),
        integration_owner: "integrator".into(),
    }];
    let p = approve(&doc);
    let mut n = nodes(&p, 2);
    for (i, node) in n.iter_mut().enumerate() {
        node.authority = alter(&node.authority, &p, |f| {
            f.physical_mapping = vec![path("src/common")];
            f.shared_declarations = vec![doc.shared_declarations[i].clone()];
        });
    }
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    let k = kernel(&g, &p, n.clone());
    assert!(k.admit(&id(2), 7).unwrap().admissible());
    n[1].authority = alter(&n[1].authority, &p, |f| {
        f.physical_mapping = vec![path("src/common/algorithm.rs")]
    });
    assert!(has(
        &kernel(&g, &p, n).admit(&id(2), 7).unwrap().blockers,
        BlockingReason::PhysicalOverlap
    ));
    doc.partitions.clear();
    let p = approve(&doc);
    let mut n = nodes(&p, 2);
    for node in &mut n {
        node.authority = alter(&node.authority, &p, |f| {
            f.physical_mapping = vec![path("src/common")]
        });
    }
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    assert!(has(
        &kernel(&g, &p, n).wave().unwrap().blockers,
        BlockingReason::PhysicalOverlap
    ));
}

#[test]
fn namespace_partitions_never_allow_a_shared_generator_or_output_alias() {
    let p = approve(&document(2));
    let mut n = nodes(&p, 2);
    for node in &mut n {
        node.authority = alter(&node.authority, &p, |f| f.generators = vec![generator()]);
    }
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    assert!(has(
        &kernel(&g, &p, n.clone()).wave().unwrap().blockers,
        BlockingReason::GeneratorTransaction
    ));
    n[1].authority = alter(&n[1].authority, &p, |f| f.generators.clear());
    n[1].mutable_outputs = vec![path("dist")];
    assert!(has(
        &kernel(&g, &p, n).wave().unwrap().blockers,
        BlockingReason::SharedOutput
    ));
}

#[test]
fn conservative_legacy_paths_use_the_same_evaluator_without_semantic_inference() {
    let p = approve(&document(2));
    let mut n = nodes(&p, 2);
    n[0].authority =
        EffectiveAuthority::conservative(n[0].authority.plan().clone(), vec![path("src")]).unwrap();
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    let k = kernel(&g, &p, n);
    assert!(has(
        &k.admit(&id(2), 7).unwrap().blockers,
        BlockingReason::PhysicalOverlap
    ));
}

#[test]
fn rejected_holds_until_stopped_then_only_exact_declared_compensation_reuses() {
    let p = approve(&document(3));
    let mut n = nodes(&p, 3);
    let comp_id = format!("{}-C{:02}", id(1), 1);
    let comp = identity(&comp_id);
    n[1].authority = alter(&n[1].authority, &p, |f| f.plan = comp.clone());
    n[1].worktree = n[0].worktree.clone();
    n[0].closure = ClosureFacts::RejectionPending;
    let mut g = graph(
        &n,
        &[PlanStatus::Rejected, PlanStatus::Ready, PlanStatus::Ready],
    );
    g.plans[0].compensating_plan = comp_id.clone();
    let k = kernel(&g, &p, n.clone());
    let before = k.admit(&comp_id, 7).unwrap();
    assert!(!before.admissible());
    assert!(has(&before.blockers, BlockingReason::SameLineage));
    assert!(has(&before.blockers, BlockingReason::SameWorktree));
    let evidence = StoppedHandoff {
        rejected_plan: n[0].authority.plan().clone(),
        compensation: comp,
        report: FileIdentity {
            path: path(&format!("docs/plan/reports/{}-fixture-review.md", id(1))),
            bytes: 10,
            sha256: hash_bytes(b"report"),
        },
        stopped: true,
        observed_revision: 7,
    };
    n[0].closure = ClosureFacts::RejectedHandoff {
        evidence: Box::new(evidence.clone()),
    };
    let k = kernel(&g, &p, n.clone());
    assert!(k.admit(&comp_id, 7).unwrap().admissible());
    assert!(k.admit(&id(3), 7).unwrap().admissible());
    // A distinct lineage still cannot steal the reserved physical checkout.
    n[2].worktree = n[0].worktree.clone();
    g.plans[2].worktree_path = n[2].worktree.as_str().into();
    assert!(has(
        &kernel(&g, &p, n.clone()).admit(&id(3), 7).unwrap().blockers,
        BlockingReason::RejectedHandoff
    ));
    let mut forged = evidence;
    forged.stopped = false;
    n[0].closure = ClosureFacts::RejectedHandoff {
        evidence: Box::new(forged),
    };
    assert!(
        SchedulingKernel::new(REPOSITORY_ID, &g, hash_bytes(b"graph"), Some(&p), n, None).is_err()
    );
}

#[test]
fn clarification_conflicting_with_an_active_owner_refuses_in_common_evaluator() {
    let p = approve(&document(2));
    let n = nodes(&p, 2);
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    let OriginalScope::Semantic { contract } = n[1].authority.original() else {
        panic!()
    };
    let clarified = AuthorityResolver::append(
        contract,
        b"immutable",
        &p,
        7,
        &[],
        ClarificationRequest {
            plan: contract.fields().plan.clone(),
            policy: p.identity().clone(),
            expected_revision: 7,
            expected_tail: None,
            actor: "owner".into(),
            reason: "correct physical mapping".into(),
            kind: ClarificationKind::Mapping,
            physical_mapping: vec![path("src/area1")],
            generators: vec![],
        },
    )
    .unwrap();
    let k = kernel(&g, &p, n.clone());
    let decision = k.admit_clarification(&id(2), clarified.clone(), 7).unwrap();
    assert!(has(&decision.blockers, BlockingReason::PhysicalOverlap));
    assert!(k.admit_clarification(&id(2), clarified.clone(), 6).is_err());
    let mut newer = g.clone();
    newer.revision = 8;
    assert!(
        kernel(&newer, &p, n)
            .admit_clarification(&id(2), clarified, 8)
            .is_err(),
        "caller cannot silently upgrade a clarification observed at the old revision"
    );
}

#[test]
fn incomplete_mismatched_cyclic_or_invalid_scope_is_error_never_compatible_false() {
    let p = approve(&document(2));
    let n = nodes(&p, 2);
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    for variant in 0..8 {
        let mut nodes = n.clone();
        let mut graph = g.clone();
        match variant {
            0 => {
                nodes.pop();
            }
            1 => nodes[1] = nodes[0].clone(),
            2 => graph.workspace_id = "other".into(),
            3 => graph.plans[0].owner.clear(),
            4 => nodes[1].mutable_outputs.clear(),
            5 => graph.plans[1].hard_predecessors = vec!["missing".into()],
            6 => {
                graph.plans[1].hard_predecessors = vec![id(1)];
                graph.plans[0].hard_predecessors = vec![id(2)];
            }
            _ => nodes[1].worktree = path(".mine/worktrees/unregistered"),
        }
        assert!(
            SchedulingKernel::new(
                REPOSITORY_ID,
                &graph,
                hash_bytes(b"graph"),
                Some(&p),
                nodes,
                None
            )
            .is_err(),
            "{variant}"
        );
    }
    let k = kernel(&g, &p, n);
    assert!(k.compatibility(&id(1), &id(1)).is_err());
    assert!(k.compatibility("missing", &id(2)).is_err());
}

#[test]
fn compatibility_property_table_is_symmetric_and_agrees_with_held_admission() {
    let p = approve(&document(2));
    for namespace_overlap in [false, true] {
        for physical_overlap in [false, true] {
            for output_overlap in [false, true] {
                let mut n = nodes(&p, 2);
                if namespace_overlap {
                    n[1].authority = alter(&n[1].authority, &p, |f| {
                        f.namespace_claims[0].subtree = ns("app.area1.child")
                    });
                }
                if physical_overlap {
                    n[1].authority = alter(&n[1].authority, &p, |f| {
                        f.physical_mapping = vec![path("src/area1/nested")]
                    });
                }
                if output_overlap {
                    n[1].mutable_outputs = n[0].mutable_outputs.clone();
                }
                let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
                let k = kernel(&g, &p, n);
                let forward = k.compatibility(&id(2), &id(1)).unwrap();
                let reverse = k.compatibility(&id(1), &id(2)).unwrap();
                assert_eq!(forward.is_empty(), reverse.is_empty());
                assert_eq!(k.admit(&id(2), 7).unwrap().blockers, forward);
                assert_eq!(
                    forward.is_empty(),
                    !namespace_overlap && !physical_overlap && !output_overlap
                );
                assert_eq!(
                    has(&forward, BlockingReason::NamespaceOverlap),
                    namespace_overlap
                );
                assert_eq!(
                    has(&forward, BlockingReason::PhysicalOverlap),
                    physical_overlap
                );
                assert_eq!(has(&forward, BlockingReason::SharedOutput), output_overlap);
            }
        }
    }
}

#[test]
fn fresh_legacy_attestation_uses_common_active_checks_and_preserves_absent_history() {
    let p = approve(&document(2));
    let mut n = nodes(&p, 2);
    let mut fields = n[1].authority.semantic().unwrap().fields().clone();
    let mut old_identity = fields.plan.fields().clone();
    old_identity.body_sha256 = None;
    fields.plan = PlanIdentity::new(old_identity).unwrap();
    // An explicitly approved semantic claim can reveal a collision that the
    // historical disjoint physical mapping could not infer.
    fields.namespace_claims[0].subtree = ns("app.area1");
    let declared = OwnershipContract::new(fields).unwrap();
    n[1].authority = EffectiveAuthority::conservative(
        declared.fields().plan.clone(),
        declared.fields().physical_mapping.clone(),
    )
    .unwrap();
    let g = graph(&n, &[PlanStatus::InProgress, PlanStatus::Ready]);
    let k = kernel(&g, &p, n);
    assert!(k.admit(&id(2), 7).unwrap().admissible());
    let effective = AuthorityResolver::append(
        &declared,
        b"immutable",
        &p,
        7,
        &[],
        ClarificationRequest {
            plan: identity(&id(2)),
            policy: p.identity().clone(),
            expected_revision: 7,
            expected_tail: None,
            actor: "owner".into(),
            reason: "fresh baseline and approved mapping attestation".into(),
            kind: ClarificationKind::BaselineAttestation,
            physical_mapping: declared.fields().physical_mapping.clone(),
            generators: vec![],
        },
    )
    .unwrap();
    assert_eq!(effective.clarifications()[0].original_body, None);
    assert!(has(
        &k.admit_clarification(&id(2), effective, 7)
            .unwrap()
            .blockers,
        BlockingReason::NamespaceOverlap
    ));
}

#[test]
fn first_activation_cannot_smuggle_shared_or_generated_outputs_outside_old_mapping() {
    let policy = approve(&document(1));
    for generated in [false, true] {
        let mut ownership = nodes(&policy, 1);
        let mut fields = ownership[0].authority.semantic().unwrap().fields().clone();
        if generated {
            fields.generators = vec![generator()];
        } else {
            fields.shared_declarations = policy.document().shared_declarations.clone();
        }
        let declared = OwnershipContract::new(fields).unwrap();
        ownership[0].authority = EffectiveAuthority::conservative(
            declared.fields().plan.clone(),
            declared.fields().physical_mapping.clone(),
        )
        .unwrap();
        let workspace = graph(&ownership, &[PlanStatus::Ready]);
        let evaluator = kernel(&workspace, &policy, ownership);
        let effective = AuthorityResolver::append(
            &declared,
            b"immutable",
            &policy,
            7,
            &[],
            ClarificationRequest {
                plan: declared.fields().plan.clone(),
                policy: policy.identity().clone(),
                expected_revision: 7,
                expected_tail: None,
                actor: "owner".into(),
                reason: "invalid activation expansion".into(),
                kind: ClarificationKind::BaselineAttestation,
                physical_mapping: declared.fields().physical_mapping.clone(),
                generators: declared.fields().generators.clone(),
            },
        )
        .unwrap();
        assert!(evaluator.admit_clarification(&id(1), effective, 7).is_err());
    }
}
