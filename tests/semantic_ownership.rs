#![forbid(unsafe_code)]
use mine::domain::graph::*;
use mine::domain::ownership::*;

fn path(s: &str) -> RepoPath {
    RepoPath::new(s).unwrap()
}
fn ns(s: &str) -> Namespace {
    Namespace::new(s).unwrap()
}
fn identity(n: u8, body: Option<Sha256>) -> PlanIdentity {
    let id = format!("plan{n:02}");
    PlanIdentity::new(PlanIdentityFields {
        repository_id: "repository".into(),
        workspace_id: "workspace".into(),
        plan_id: id.clone(),
        plan_path: path(&format!("docs/plan/{id}-fixture.md")),
        body_sha256: body,
        lineage_id: id.clone(),
        branch: format!("plan/{id}-fixture"),
    })
    .unwrap()
}
fn generator() -> GeneratorTransaction {
    GeneratorTransaction {
        name: "payload".into(),
        inputs: vec![path("policy/source.json")],
        machine_source: path("policy/source.json"),
        outputs: vec![path("dist/payload"), path("dist/seal")],
        seals: vec![path("dist/seal")],
    }
}
fn document() -> OwnershipPolicyDocument {
    let entry = SharedDeclaration {
        path: path("src/lib.rs"),
        entry: "storage".into(),
        class: DeclarationClass::Export,
        integration_owner: "integrator".into(),
    };
    OwnershipPolicyDocument {
        schema_version: 1,
        clarification_actors: vec!["owner".into()],
        mappings: vec![ResponsibilityMapping {
            responsibility: "storage".into(),
            namespaces: vec![NamespaceClaim {
                subtree: ns("mine.storage"),
                excluded: vec![ns("mine.storage.foreign")],
            }],
            contracts: vec!["store".into()],
            physical_prefixes: vec![path("src/storage"), path("tests/storage")],
            shared_entries: vec![SharedEntryKey {
                path: entry.path.clone(),
                entry: entry.entry.clone(),
            }],
            generator_names: vec!["payload".into()],
        }],
        partitions: vec![],
        shared_declarations: vec![entry],
        generators: vec![generator()],
    }
}
fn approval(doc: &OwnershipPolicyDocument) -> (Vec<u8>, PolicyApproval) {
    let bytes = serde_json::to_vec(doc).unwrap();
    let approval = PolicyApproval {
        identity: PolicyIdentity::new(PolicyIdentityFields {
            design_references: vec![path("docs/design/governance/semantic-ownership.md")],
            sha256: hash_bytes(&bytes),
        })
        .unwrap(),
        actor: "owner".into(),
        observed_revision: 2,
    };
    (bytes, approval)
}
fn policy() -> ApprovedOwnershipPolicy {
    let (bytes, a) = approval(&document());
    ApprovedOwnershipPolicy::verify(&bytes, &a, 7).unwrap()
}
fn original(p: &ApprovedOwnershipPolicy) -> OwnershipContract {
    OwnershipContract::new(OwnershipFields {
        schema_version: 1,
        plan: identity(1, Some(hash_bytes(b"immutable plan"))),
        policy: p.identity().clone(),
        responsibility: "storage".into(),
        namespace_claims: document().mappings[0].namespaces.clone(),
        owned_contracts: vec!["store".into()],
        excluded_responsibilities: vec!["foreign".into()],
        excluded_contracts: vec!["foreign_contract".into()],
        physical_mapping: vec![path("src/storage")],
        shared_declarations: document().shared_declarations,
        generators: vec![generator()],
    })
    .unwrap()
}
fn resolve(o: &OwnershipContract, p: &ApprovedOwnershipPolicy) -> EffectiveAuthority {
    AuthorityResolver::resolve(
        o,
        b"immutable plan",
        &identity(1, Some(hash_bytes(b"immutable plan"))),
        p,
        7,
        &[],
    )
    .unwrap()
}
fn request(o: &OwnershipContract, p: &ApprovedOwnershipPolicy) -> ClarificationRequest {
    ClarificationRequest {
        plan: identity(1, Some(hash_bytes(b"immutable plan"))),
        policy: p.identity().clone(),
        expected_revision: 7,
        expected_tail: None,
        actor: "owner".into(),
        reason: "complete the declared test mapping".into(),
        kind: ClarificationKind::Mapping,
        physical_mapping: vec![path("src/storage"), path("tests/storage")],
        generators: o.fields().generators.clone(),
    }
}

#[test]
fn segment_case_exclusion_and_internal_layout_are_real_authority() {
    let p = policy();
    let o = original(&p);
    let effective = resolve(&o, &p);
    for name in [
        "mine.storage",
        "mine.storage.cache",
        "mine.storage.foreignish",
    ] {
        assert!(effective.owns_namespace(&ns(name)));
    }
    for name in [
        "mine.storage2",
        "Mine.storage",
        "mine.storage.foreign",
        "mine.storage.foreign.deep",
    ] {
        assert!(!effective.owns_namespace(&ns(name)));
    }
    effective
        .authorize_internal(
            &ns("mine.storage.cache"),
            "store",
            &path("src/storage/new/split.rs"),
        )
        .unwrap();
    assert!(
        effective
            .authorize_internal(
                &ns("mine.storage.cache"),
                "foreign_contract",
                &path("src/storage/a.rs")
            )
            .is_err()
    );
    assert!(
        effective
            .authorize_internal(
                &ns("mine.storage"),
                "store",
                &path("src/storage-other/a.rs")
            )
            .is_err()
    );
    assert!(
        effective
            .authorize_internal(
                &ns("mine.storage.foreign"),
                "store",
                &path("src/storage/a.rs")
            )
            .is_err()
    );
}

#[test]
fn contradictory_owned_excluded_scopes_are_errors() {
    let p = policy();
    let base = original(&p).fields().clone();
    let mut f = base.clone();
    f.namespace_claims.push(NamespaceClaim {
        subtree: ns("mine.storage.foreign.reclaimed"),
        excluded: vec![],
    });
    assert!(OwnershipContract::new(f).is_err());
    let mut f = base.clone();
    f.excluded_contracts.push("store".into());
    assert!(OwnershipContract::new(f).is_err());
    let mut f = base.clone();
    f.excluded_responsibilities.push("storage".into());
    assert!(OwnershipContract::new(f).is_err());
    let mut f = base.clone();
    f.physical_mapping.push(f.physical_mapping[0].clone());
    assert!(OwnershipContract::new(f).is_err());
    let mut f = base;
    f.generators[0].inputs.clear();
    assert!(OwnershipContract::new(f).is_err());
    for invalid in [
        "",
        "mine..storage",
        "mine/storage",
        ".mine",
        "mine.",
        "mine storage",
    ] {
        assert!(Namespace::new(invalid).is_err());
    }
    for invalid in [
        "../escape",
        "src/../escape",
        "/absolute",
        "src\\storage",
        "src/*",
    ] {
        assert!(RepoPath::new(invalid).is_err());
    }
}

#[test]
fn actual_policy_bytes_approval_revision_and_structure_fail_closed() {
    let (bytes, a) = approval(&document());
    assert!(ApprovedOwnershipPolicy::verify(b"{}", &a, 7).is_err());
    assert!(ApprovedOwnershipPolicy::verify(&bytes, &a, 1).is_err());
    for variant in 0..6 {
        let mut doc = document();
        match variant {
            0 => doc.schema_version = 99,
            1 => doc.mappings.clear(),
            2 => doc.clarification_actors.clear(),
            3 => doc.mappings[0].generator_names.push("missing".into()),
            4 => doc.generators[0].seals.push(path("missing/seal")),
            _ => doc.mappings[0].shared_entries[0].entry = "undeclared".into(),
        }
        let (bytes, a) = approval(&doc);
        assert!(
            ApprovedOwnershipPolicy::verify(&bytes, &a, 7).is_err(),
            "{variant}"
        );
    }
}

#[test]
fn tampered_body_cannot_be_reauthorized_by_recomputing_request_hash() {
    let p = policy();
    let o = original(&p);
    let forged = identity(1, Some(hash_bytes(b"changed acceptance")));
    assert!(AuthorityResolver::resolve(&o, b"changed acceptance", &forged, &p, 7, &[]).is_err());
    assert!(
        AuthorityResolver::resolve(
            &o,
            b"immutable plan",
            &identity(2, Some(hash_bytes(b"immutable plan"))),
            &p,
            7,
            &[]
        )
        .is_err()
    );
    let mut f = identity(1, Some(hash_bytes(b"immutable plan")))
        .fields()
        .clone();
    f.workspace_id = "other".into();
    assert!(
        AuthorityResolver::resolve(
            &o,
            b"immutable plan",
            &PlanIdentity::new(f).unwrap(),
            &p,
            7,
            &[]
        )
        .is_err()
    );
}

#[test]
fn clarification_keeps_original_append_only_and_effective_scope_visible() {
    let p = policy();
    let o = original(&p);
    let bytes = serde_json::to_vec(&o).unwrap();
    let first =
        AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], request(&o, &p)).unwrap();
    assert_eq!(serde_json::to_vec(&o).unwrap(), bytes);
    assert_eq!(o.fields().physical_mapping.len(), 1);
    assert_eq!(first.physical_mapping().len(), 2);
    assert_eq!(
        first.clarifications()[0].original_body,
        o.fields().plan.fields().body_sha256
    );
    let mut next = request(&o, &p);
    next.expected_revision = 8;
    next.expected_tail = Some(first.clarifications()[0].digest());
    next.reason = "narrow tests mapping".into();
    next.physical_mapping = vec![path("tests/storage")];
    let second =
        AuthorityResolver::append(&o, b"immutable plan", &p, 8, first.clarifications(), next)
            .unwrap();
    assert!(second.clarifications().starts_with(first.clarifications()));
    assert_eq!(second.clarifications().len(), 2);
    second
        .authorize_internal(&ns("mine.storage"), "store", &path("tests/storage/new.rs"))
        .unwrap();
    assert!(
        second
            .authorize_internal(&ns("mine.storage"), "store", &path("src/storage/old.rs"))
            .is_err()
    );
    let wire = serde_json::to_value(&second).unwrap();
    assert!(wire.get("original").is_some());
    assert!(wire.get("effective").is_some());
}

#[test]
fn forged_stale_unauthorized_expanded_and_reordered_records_refuse() {
    let p = policy();
    let o = original(&p);
    for variant in 0..7 {
        let mut r = request(&o, &p);
        match variant {
            0 => r.expected_revision = 6,
            1 => r.expected_tail = Some(hash_bytes(b"nonexistent")),
            2 => r.actor = "executor".into(),
            3 => r.reason.clear(),
            4 => r.physical_mapping.push(path("src/foreign")),
            5 => r.generators[0].outputs.pop().map(|_| ()).unwrap(),
            _ => r.plan = identity(2, Some(hash_bytes(b"immutable plan"))),
        }
        assert!(
            AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], r).is_err(),
            "{variant}"
        );
    }
    let good =
        AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], request(&o, &p)).unwrap();
    for variant in 0..6 {
        let mut history = good.clarifications().to_vec();
        match variant {
            0 => history[0].sequence = 2,
            1 => history[0].original_body = None,
            2 => history[0].observed_revision = 8,
            3 => history[0].actor = "forged".into(),
            4 => history[0].previous_record = Some(hash_bytes(b"unknown")),
            _ => history.push(history[0].clone()),
        }
        assert!(
            AuthorityResolver::resolve(&o, b"immutable plan", &o.fields().plan, &p, 7, &history)
                .is_err(),
            "{variant}"
        );
    }
}

#[test]
fn absent_historical_baseline_requires_fresh_explicit_attestation() {
    let p = policy();
    let mut f = original(&p).fields().clone();
    f.plan = identity(1, None);
    let o = OwnershipContract::new(f).unwrap();
    let observed = identity(1, Some(hash_bytes(b"immutable plan")));
    assert!(AuthorityResolver::resolve(&o, b"immutable plan", &observed, &p, 7, &[]).is_err());
    assert!(AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], request(&o, &p)).is_err());
    let mut attestation = request(&o, &p);
    attestation.kind = ClarificationKind::BaselineAttestation;
    attestation.physical_mapping = o.fields().physical_mapping.clone();
    let effective =
        AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], attestation).unwrap();
    assert_eq!(effective.clarifications()[0].original_body, None);
    assert_eq!(o.fields().plan.fields().body_sha256, None);
    assert_eq!(
        effective.plan().fields().body_sha256,
        Some(hash_bytes(b"immutable plan"))
    );
    let mut request = request(&o, &p);
    request.expected_revision = 8;
    request.expected_tail = Some(effective.clarifications()[0].digest());
    assert!(
        AuthorityResolver::append(
            &o,
            b"immutable plan",
            &p,
            8,
            effective.clarifications(),
            request
        )
        .is_ok()
    );
}

#[test]
fn only_declared_entry_class_integrator_and_complete_generator_can_write() {
    let p = policy();
    let o = original(&p);
    let e = resolve(&o, &p);
    let shared = o.fields().shared_declarations[0].clone();
    e.authorize_shared(&shared).unwrap();
    for variant in 0..3 {
        let mut d = shared.clone();
        match variant {
            0 => d.entry = "foreign".into(),
            1 => d.class = DeclarationClass::Registration,
            _ => d.integration_owner = "other".into(),
        };
        assert!(e.authorize_shared(&d).is_err());
    }
    e.authorize_generation("payload", &generator().outputs)
        .unwrap();
    assert!(
        e.authorize_generation("payload", &[path("dist/payload")])
            .is_err()
    );
    assert!(
        e.authorize_generation("unknown", &generator().outputs)
            .is_err()
    );
    let mut f = o.fields().clone();
    f.generators[0].outputs.push(path("dist/rogue"));
    let malformed = OwnershipContract::new(f).unwrap();
    assert!(
        AuthorityResolver::resolve(
            &malformed,
            b"immutable plan",
            &malformed.fields().plan,
            &p,
            7,
            &[]
        )
        .is_err()
    );
    let mut value = serde_json::to_value(&shared).unwrap();
    value["class"] = serde_json::json!("algorithm");
    assert!(serde_json::from_value::<SharedDeclaration>(value).is_err());
}

#[test]
fn conservative_history_has_no_inferred_semantic_permissions() {
    let e = EffectiveAuthority::conservative(identity(1, None), vec![path("src/storage")]).unwrap();
    assert!(e.policy().is_none());
    assert!(e.semantic().is_none());
    assert!(!e.owns_namespace(&ns("mine.storage")));
    assert!(
        e.authorize_internal(&ns("mine.storage"), "store", &path("src/storage/a.rs"))
            .is_err()
    );
    assert!(EffectiveAuthority::conservative(identity(1, None), vec![]).is_err());
}

#[test]
fn conservative_observation_retains_original_absence_and_never_reauthorizes_known_drift() {
    let raw = b"# Raw Plan bytes\r\n";
    let mapping = vec![path("src/storage")];
    for known in [false, true] {
        let original = identity(1, known.then(|| hash_bytes(raw)));
        let result =
            EffectiveAuthority::observe_conservative(original.clone(), raw, mapping.clone())
                .unwrap();
        assert_eq!(
            result.original(),
            &OriginalScope::Conservative {
                plan: original.clone(),
                physical_mapping: mapping.clone()
            }
        );
        let mut effective = original.fields().clone();
        effective.body_sha256 = Some(hash_bytes(raw));
        assert_eq!(result.plan(), &PlanIdentity::new(effective).unwrap());
        assert_eq!(result.physical_mapping(), mapping);
        assert!(result.semantic().is_none());
        assert!(result.policy().is_none());
        assert!(result.clarifications().is_empty());
        assert!(
            result
                .authorize_internal(&ns("mine.storage"), "store", &path("src/storage/a.rs"))
                .is_err()
        );
        if known {
            assert!(
                EffectiveAuthority::observe_conservative(
                    original,
                    b"# Raw Plan bytes\n",
                    mapping.clone()
                )
                .is_err()
            );
        }
    }
}

#[test]
fn generator_files_inside_a_source_mapping_cannot_be_written_as_ordinary_files() {
    let mut doc = document();
    doc.generators[0].outputs = vec![path("src/storage/generated.rs"), path("src/storage/seal")];
    doc.generators[0].seals = vec![path("src/storage/seal")];
    let (bytes, a) = approval(&doc);
    let p = ApprovedOwnershipPolicy::verify(&bytes, &a, 7).unwrap();
    let mut fields = original(&p).fields().clone();
    fields.generators = doc.generators;
    let original = OwnershipContract::new(fields).unwrap();
    let e = resolve(&original, &p);
    let err = e
        .authorize_internal(
            &ns("mine.storage"),
            "store",
            &path("src/storage/generated.rs"),
        )
        .unwrap_err();
    assert_eq!(err.field, "write.generator");
    e.authorize_generation("payload", &original.fields().generators[0].outputs)
        .unwrap();
}

#[test]
fn omitted_policy_shared_entries_and_generated_sets_are_not_internal_write_permissions() {
    let mut doc = document();
    doc.shared_declarations[0].path = path("src/storage/composition.rs");
    doc.mappings[0].shared_entries[0].path = doc.shared_declarations[0].path.clone();
    doc.generators[0].outputs = vec![path("src/storage/generated.rs"), path("src/storage/seal")];
    doc.generators[0].seals = vec![path("src/storage/seal")];
    let (bytes, approval) = approval(&doc);
    let policy = ApprovedOwnershipPolicy::verify(&bytes, &approval, 7).unwrap();
    let mut fields = original(&policy).fields().clone();
    fields.shared_declarations.clear();
    fields.generators.clear();
    let original = OwnershipContract::new(fields).unwrap();
    let effective = resolve(&original, &policy);
    for (file, expected) in [
        ("src/storage/composition.rs", "write.shared"),
        ("src/storage/generated.rs", "write.generator"),
        ("src/storage/seal", "write.generator"),
    ] {
        let error = effective
            .authorize_internal(&ns("mine.storage"), "store", &path(file))
            .unwrap_err();
        assert_eq!(error.field, expected);
    }
    assert!(
        effective
            .authorize_shared(&doc.shared_declarations[0])
            .is_err()
    );
    assert!(
        effective
            .authorize_generation("payload", &doc.generators[0].outputs)
            .is_err()
    );
    effective
        .authorize_internal(&ns("mine.storage"), "store", &path("src/storage/new.rs"))
        .unwrap();
}

#[test]
fn clarified_records_cannot_smuggle_behavior_fields_or_rewind_revision() {
    let p = policy();
    let o = original(&p);
    let first =
        AuthorityResolver::append(&o, b"immutable plan", &p, 7, &[], request(&o, &p)).unwrap();
    let mut value = serde_json::to_value(&first.clarifications()[0]).unwrap();
    value["owned_contracts"] = serde_json::json!(["foreign"]);
    assert!(serde_json::from_value::<ClarificationRecord>(value).is_err());
    let mut again = request(&o, &p);
    again.expected_tail = Some(first.clarifications()[0].digest());
    assert!(
        AuthorityResolver::append(&o, b"immutable plan", &p, 7, first.clarifications(), again)
            .is_err()
    );
    let mut fields = o.fields().clone();
    fields.namespace_claims.push(NamespaceClaim {
        subtree: ns("mine"),
        excluded: vec![],
    });
    assert!(
        OwnershipContract::new(fields).is_err(),
        "a broader duplicate claim must not bypass an exclusion"
    );
}
