use mine_v1_contracts::*;
use serde_json::json;

const PROJECT: &str = "11111111-1111-4111-8111-111111111111";
const SNAPSHOT: &str = "22222222-2222-4222-8222-222222222222";

fn pair() -> (Selection, Discovery) {
    let selection = Selection {
        project_id: PROJECT.to_owned().try_into().unwrap(),
        snapshot_id: SNAPSHOT.to_owned().try_into().unwrap(),
        provider: Provider::Codex,
        model: RequestedValue::Exact("fixture-model".into()),
        effort: RequestedValue::Exact("fixture-high".into()),
        speed: RequestedValue::Exact("fixture-priority".into()),
    };
    let discovery = Discovery {
        project_id: selection.project_id.clone(),
        snapshot_id: selection.snapshot_id.clone(),
        provider: selection.provider,
        runtime_version: "fixture-only".into(),
        config_fingerprint: "a".repeat(64).try_into().unwrap(),
        models: vec![ModelCapability {
            model: "fixture-model".into(),
            effort: Choices::Supported(vec!["fixture-high".into()]),
            speed: Choices::Supported(vec!["fixture-priority".into()]),
        }],
    };
    (selection, discovery)
}

#[test]
fn canonical_ids_reject_nil_alias_and_non_uuid() {
    for id in [
        "00000000-0000-0000-0000-000000000000",
        "EXEC-ABC123",
        "11111111111141118111111111111111",
        "bad",
    ] {
        assert!(ProjectId::try_from(id.to_owned()).is_err());
    }
    assert!(ProjectId::try_from(PROJECT.to_owned()).is_ok());
}

#[test]
fn counters_preserve_large_values_and_refuse_overflow() {
    let revision = EntityRevision::new(i64::MAX as u64).unwrap();
    let wire = serde_json::to_string(&revision).unwrap();
    assert_eq!(wire, "\"9223372036854775807\"");
    assert_eq!(
        serde_json::from_str::<EntityRevision>(&wire).unwrap(),
        revision
    );
    assert_eq!(revision.checked_next(), Err(ErrorCode::InvalidInput));
    for bad in [
        "\"01\"",
        "\"+1\"",
        "\"-1\"",
        "1",
        "\"18446744073709551616\"",
        "\"9223372036854775808\"",
    ] {
        assert!(serde_json::from_str::<EntityRevision>(bad).is_err());
    }
}

#[test]
fn digest_is_exact_lowercase_sha256_shape() {
    assert!(Digest::try_from("a".repeat(64)).is_ok());
    assert!(Digest::try_from("A".repeat(64)).is_err());
    assert!(Digest::try_from("a".repeat(63)).is_err());
    assert!(Digest::try_from("z".repeat(64)).is_err());
}

#[test]
fn forged_actor_and_unknown_fields_refuse_at_boundary() {
    let mut wire = json!({"operation_id":PROJECT,"project_id":PROJECT,"expected_revision":"0","assignment_epoch":null,"input_digest":"a".repeat(64)});
    assert!(serde_json::from_value::<CommandMeta>(wire.clone()).is_ok());
    wire["actor"] = json!("owner");
    assert!(serde_json::from_value::<CommandMeta>(wire).is_err());
}

#[test]
fn concurrency_rejects_stale_entity_and_attempt_without_mutation() {
    let meta: CommandMeta = serde_json::from_value(json!({"operation_id":PROJECT,"project_id":PROJECT,"expected_revision":"3","assignment_epoch":"2","input_digest":"a".repeat(64)})).unwrap();
    assert_eq!(
        check_concurrency(
            &meta,
            EntityRevision::new(3).unwrap(),
            Some(AssignmentEpoch::new(2).unwrap())
        ),
        Ok(())
    );
    assert_eq!(
        check_concurrency(
            &meta,
            EntityRevision::new(4).unwrap(),
            Some(AssignmentEpoch::new(2).unwrap())
        ),
        Err(ErrorCode::RevisionConflict)
    );
    assert_eq!(
        check_concurrency(&meta, EntityRevision::new(3).unwrap(), None),
        Err(ErrorCode::StaleEpoch)
    );
}

#[test]
fn exact_provider_native_tuple_roundtrips() {
    let (selection, discovery) = pair();
    assert_eq!(validate_selection(&selection, &discovery), Ok(()));
    assert_eq!(
        serde_json::from_str::<Selection>(&serde_json::to_string(&selection).unwrap()).unwrap(),
        selection
    );
}

#[test]
fn stale_discovery_and_cross_project_choices_refuse() {
    let (selection, mut discovery) = pair();
    discovery.project_id = SNAPSHOT.to_owned().try_into().unwrap();
    assert_eq!(
        validate_selection(&selection, &discovery),
        Err(ErrorCode::CapabilityStale)
    );
    discovery.project_id = selection.project_id.clone();
    discovery.snapshot_id = PROJECT.to_owned().try_into().unwrap();
    assert_eq!(
        validate_selection(&selection, &discovery),
        Err(ErrorCode::CapabilityStale)
    );
}

#[test]
fn unsupported_and_unknown_fast_never_silently_downgrade() {
    let (selection, mut discovery) = pair();
    for choices in [
        Choices::Unsupported,
        Choices::Unknown,
        Choices::Supported(vec![]),
    ] {
        discovery.models[0].speed = choices;
        assert_eq!(
            validate_selection(&selection, &discovery),
            Err(ErrorCode::SelectionUnsupported)
        );
    }
}

#[test]
fn unresolved_default_model_cannot_accept_specific_effort() {
    let (mut selection, discovery) = pair();
    selection.model = RequestedValue::RuntimeDefault;
    assert_eq!(
        validate_selection(&selection, &discovery),
        Err(ErrorCode::SelectionUnsupported)
    );
    selection.effort = RequestedValue::RuntimeDefault;
    selection.speed = RequestedValue::RuntimeDefault;
    assert_eq!(validate_selection(&selection, &discovery), Ok(()));
}

#[test]
fn old_provider_and_unknown_enum_values_refuse() {
    assert!(serde_json::from_value::<Provider>(json!("opencode")).is_err());
    assert!(serde_json::from_value::<WorkState>(json!("accepted")).is_err());
    assert!(
        serde_json::from_value::<ObservedValue>(json!({"kind":"unknown","value":"pretend"}))
            .is_err()
    );
}

#[test]
fn only_wait_states_may_wake_and_pause_always_wins() {
    for state in [
        WorkState::WaitingPeer,
        WorkState::WaitingDecision,
        WorkState::WaitingResource,
    ] {
        assert!(state.can_wake(false));
        assert!(!state.can_wake(true));
    }
    for state in [
        WorkState::Runnable,
        WorkState::Running,
        WorkState::Pausing,
        WorkState::Paused,
        WorkState::Recovering,
        WorkState::Failed,
        WorkState::Done,
    ] {
        assert!(!state.can_wake(false));
        assert!(!state.can_wake(true));
    }
}

#[test]
fn public_schema_forbids_extra_command_fields() {
    let schema = serde_json::to_value(schemars::schema_for!(ContractSchema)).unwrap();
    assert_eq!(
        schema["$defs"]["CommandMeta"]["additionalProperties"],
        false
    );
    assert_eq!(schema["$defs"]["EventCursor"]["type"], "string");
}

#[test]
fn ambiguous_discovery_is_rejected_before_selection() {
    let (selection, mut discovery) = pair();
    discovery.models.push(discovery.models[0].clone());
    assert_eq!(
        validate_selection(&selection, &discovery),
        Err(ErrorCode::InvalidInput)
    );
    discovery.models.pop();
    discovery.models[0].effort = Choices::Supported(vec!["same".into(), "same".into()]);
    assert_eq!(validate_discovery(&discovery), Err(ErrorCode::InvalidInput));
    discovery.models[0].effort = Choices::Supported(vec![" ".into()]);
    assert_eq!(validate_discovery(&discovery), Err(ErrorCode::InvalidInput));
}

#[test]
fn dashboard_fixture_uses_the_same_rust_boundary() {
    let snapshot: FrontierSnapshot =
        serde_json::from_str(include_str!("../fixtures/frontier.json")).unwrap();
    assert_eq!(snapshot.nodes.len(), 4);
    assert_eq!(snapshot.nodes[3].state, WorkState::Runnable);
    assert!(snapshot.nodes[2].waiting_on.is_some());
    let mut wire = serde_json::to_value(snapshot).unwrap();
    wire["cursor"] = json!("9223372036854775808");
    assert!(serde_json::from_value::<FrontierSnapshot>(wire).is_err());
}

#[test]
fn control_command_needs_no_fabricated_project() {
    let wire =
        json!({"operation_id":PROJECT,"expected_revision":"0","input_digest":"a".repeat(64)});
    assert!(serde_json::from_value::<ControlCommandMeta>(wire.clone()).is_ok());
    assert!(serde_json::from_value::<CommandMeta>(wire).is_err());
}
