use mine_v1_contracts::ErrorCode;
use mine_v1_runtime_contracts::*;
use serde_json::{Value, json};

const ID: &str = "00000000-0000-0000-0000-000000000001";
const OTHER: &str = "00000000-0000-0000-0000-000000000002";

fn context() -> Value {
    json!({
        "provider":"codex", "runtime_profile_ref":"profile1",
        "credential_profile_ref":"credential1", "trusted_config_root":"D:\\root",
        // mine-release-allow-plan-reference: canonical worktree fixture, not release provenance
        "execution_cwd":"D:\\root\\.mine\\worktrees\\plan01",
        "config_fingerprint":"a".repeat(64), "policy_revision":"1"
    })
}
fn scope() -> Value {
    json!({"kind":"attempt", "principal_id":ID, "assignment_id":ID,
        "assignment_revision":"1", "attempt_id":ID, "generation":"1", "fencing_epoch":"1"})
}
fn binding() -> Value {
    json!({"provider":"codex", "native_session_id":"opaque-session",
        "native_thread_id":null,"native_storage_ref":null,"attempt_id":ID,"generation":"1"})
}
fn selection() -> Value {
    json!({"project_id":ID,"snapshot_id":ID,"provider":"codex",
        "model":{"kind":"exact","value":"runtime-model"},
        "effort":{"kind":"exact","value":"high"},"speed":{"kind":"runtime_default"}})
}
fn snapshot() -> RuntimeCapabilitySnapshot {
    serde_json::from_value(json!({
        "discovery":{"project_id":ID,"snapshot_id":ID,"provider":"codex",
            "runtime_version":"test-only","config_fingerprint":"a".repeat(64),
            "models":[{"model":"runtime-model","effort":{"kind":"supported","values":["high"]},
                "speed":{"kind":"unknown"}}]},
        "adapter_version":"test-only","protocol_schema_digest":"b".repeat(64),
        "credential_profile_ref":"credential1","observed_at":"10","expires_at":"20",
        "methods":[{"method":"create","state":"supported","reason":null},
            {"method":"resume","state":"supported","reason":null}],
        "model_details":[],"diagnostics":[]
    }))
    .unwrap()
}
fn request(method: &str, payload: Value) -> Value {
    json!({"protocol":PROTOCOL,"request_id":ID,"operation_id":ID,"project_id":ID,
        "scope":scope(),"deadline_ms":1000,"call":{"method":method,"payload":payload}})
}
fn create() -> Value {
    request(
        "create",
        json!({"context":context(),"selection":selection(),"display_name":"EXEC-A"}),
    )
}
fn now() -> UtcMillis {
    serde_json::from_value(json!("15")).unwrap()
}
fn valid(value: Value) -> Result<(), ErrorCode> {
    let request: Request = serde_json::from_value(value).unwrap();
    request.validate(Some(&snapshot()), &now())
}

#[test]
fn exact_selection_composes_common_types() {
    assert_eq!(valid(create()), Ok(()));
    let encoded =
        serde_json::to_value(serde_json::from_value::<Request>(create()).unwrap()).unwrap();
    assert_eq!(
        encoded["call"]["payload"]["selection"]["model"]["kind"],
        "exact"
    );
    let mut old = create();
    old["call"]["payload"]["selection"]["model"]["kind"] = json!("value");
    assert!(serde_json::from_value::<Request>(old).is_err());
}

#[test]
fn unknown_fields_methods_and_duplicate_keys_fail() {
    let mut extra = create();
    extra["untrusted"] = json!(true);
    assert!(serde_json::from_value::<Request>(extra).is_err());
    let mut extra = create();
    extra["call"]["payload"]["provider_override"] = json!("other");
    assert!(serde_json::from_value::<Request>(extra).is_err());
    let mut unknown = create();
    unknown["call"]["method"] = json!("shell");
    assert!(serde_json::from_value::<Request>(unknown).is_err());
    let duplicate = format!(
        "{{\"protocol\":\"{PROTOCOL}\",{}",
        serde_json::to_string(&create())
            .unwrap()
            .trim_start_matches('{')
    );
    assert!(serde_json::from_str::<Request>(&duplicate).is_err());
}

#[test]
fn jsonl_has_lf_only_record_boundary_and_bounded_utf8() {
    let mut value = create();
    value["call"]["payload"]["display_name"] = json!("a\u{2028}b");
    let frame = format!("{}\n", serde_json::to_string(&value).unwrap());
    assert!(decode_request_frame(frame.as_bytes()).is_ok());
    assert!(decode_request_frame(frame.trim_end().as_bytes()).is_err());
    assert!(decode_request_frame(format!("{frame}{frame}").as_bytes()).is_err());
    assert!(decode_request_frame(b"\xff\n").is_err());
    assert!(decode_request_frame(format!("{}\r\n", frame.trim_end()).as_bytes()).is_err());
    assert!(decode_request_frame(&vec![b' '; MAX_FRAME_BYTES + 1]).is_err());
}

#[test]
fn discovery_requires_project_scope_and_context_is_structural_only() {
    let mut value = request("discover", json!({"context":context()}));
    assert_eq!(valid(value.clone()), Err(ErrorCode::InvalidInput));
    value["scope"] = json!({"kind":"project"});
    assert_eq!(valid(value.clone()), Ok(()));
    value["call"]["payload"]["context"]["execution_cwd"] = json!("relative");
    assert_eq!(valid(value), Err(ErrorCode::InvalidInput));
    let mut wrong = create();
    wrong["scope"] = json!({"kind":"project"});
    assert_eq!(valid(wrong), Err(ErrorCode::InvalidInput));
}

#[test]
fn protocol_deadline_uuid_and_counter_validation() {
    let mut value = create();
    value["protocol"] = json!("mine-runtime/2");
    assert_eq!(valid(value), Err(ErrorCode::VersionUnsupported));
    let mut value = create();
    value["deadline_ms"] = json!(0);
    assert_eq!(valid(value), Err(ErrorCode::InvalidInput));
    for bad in ["not-id", "00000000-0000-0000-0000-000000000000"] {
        let mut value = create();
        value["request_id"] = json!(bad);
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
    for bad in [
        json!(1),
        json!("01"),
        json!("-1"),
        json!("9223372036854775808"),
    ] {
        let mut value = create();
        value["scope"]["generation"] = bad;
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
    let value: UtcMillis = serde_json::from_value(json!("9007199254740993")).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!("9007199254740993")
    );
}

#[test]
fn snapshot_project_config_credential_and_expiry_are_bound() {
    for path in ["project_id", "snapshot_id"] {
        let mut value = create();
        value["call"]["payload"]["selection"][path] = json!(OTHER);
        assert_eq!(valid(value), Err(ErrorCode::CapabilityStale));
    }
    for (field, mismatch) in [
        ("credential_profile_ref", json!("other")),
        ("config_fingerprint", json!("c".repeat(64))),
    ] {
        let mut value = create();
        value["call"]["payload"]["context"][field] = mismatch;
        assert_eq!(valid(value), Err(ErrorCode::CapabilityStale));
    }
    let request: Request = serde_json::from_value(create()).unwrap();
    for timestamp in ["9", "20", "21"] {
        assert_eq!(
            request.validate(
                Some(&snapshot()),
                &serde_json::from_value(json!(timestamp)).unwrap()
            ),
            Err(ErrorCode::CapabilityStale)
        );
    }
    assert_eq!(
        request.validate(None, &now()),
        Err(ErrorCode::CapabilityStale)
    );
}

#[test]
fn unsupported_unknown_and_unresolved_default_are_not_exact_choices() {
    let mut value = create();
    value["call"]["payload"]["selection"]["speed"] = json!({"kind":"exact","value":"fast"});
    assert_eq!(valid(value), Err(ErrorCode::SelectionUnsupported));
    let mut value = create();
    value["call"]["payload"]["selection"]["model"] = json!({"kind":"runtime_default"});
    assert_eq!(valid(value), Err(ErrorCode::SelectionUnsupported));
    let mut snap = snapshot();
    snap.methods[0].state = CapabilityState::Unknown;
    assert_eq!(snap.validate(&now()), Err(ErrorCode::InvalidInput));
    snap.methods[0].reason = Some("not observed".into());
    assert_eq!(snap.validate(&now()), Ok(()));
    snap.methods.push(snap.methods[0].clone());
    assert_eq!(snap.validate(&now()), Err(ErrorCode::InvalidInput));
}

#[test]
fn all_finite_calls_roundtrip_and_validate() {
    let calls = [
        (
            "resume",
            json!({"context":context(),"binding":binding(),"selection":selection()}),
        ),
        ("rename", json!({"binding":binding(),"display_name":"name"})),
        (
            "submit",
            json!({"binding":binding(),"delivery_id":ID,"mode":"start","expected_native_turn_id":null,"content":[{"type":"text","text":"hello"}]}),
        ),
        (
            "interrupt",
            json!({"binding":binding(),"expected_native_turn_id":null,"reason":"pause","discard_pending":true}),
        ),
        (
            "inspect",
            json!({"binding":binding(),"create_operation_id":null}),
        ),
        ("shutdown", json!({"binding":binding(),"reason":"pause"})),
    ];
    for (method, payload) in calls {
        let value = request(method, payload);
        assert_eq!(valid(value.clone()), Ok(()), "{method}");
        let parsed: Request = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
}

#[test]
fn stale_binding_empty_content_ambiguous_inspect_and_weak_pause_fail() {
    let mut stale = binding();
    stale["generation"] = json!("2");
    assert_eq!(
        valid(request(
            "rename",
            json!({"binding":stale,"display_name":"name"})
        )),
        Err(ErrorCode::StaleEpoch)
    );
    assert_eq!(
        valid(request(
            "submit",
            json!({"binding":binding(),"delivery_id":ID,"mode":"start","expected_native_turn_id":null,"content":[]})
        )),
        Err(ErrorCode::InvalidInput)
    );
    assert_eq!(
        valid(request(
            "inspect",
            json!({"binding":binding(),"create_operation_id":ID})
        )),
        Err(ErrorCode::InvalidInput)
    );
    assert_eq!(
        valid(request(
            "inspect",
            json!({"binding":null,"create_operation_id":null})
        )),
        Err(ErrorCode::InvalidInput)
    );
    assert_eq!(
        valid(request(
            "interrupt",
            json!({"binding":binding(),"expected_native_turn_id":null,"reason":"pause","discard_pending":false})
        )),
        Err(ErrorCode::InvalidInput)
    );
}

fn quiet() -> Value {
    json!({"observed_at":"15","turn_state":"terminal","pending_inputs":"empty",
        "automatic_continuation":"stopped","owned_processes":[],"external_effects":"settled",
        "evidence_refs":["verified-process-inventory"],"confidence":"confirmed"})
}

#[test]
fn terminal_parent_and_empty_process_list_are_not_enough() {
    let quiet: QuiescenceObservation = serde_json::from_value(quiet()).unwrap();
    assert_eq!(quiet.validate(), Ok(()));
    assert!(quiet.is_confirmed_quiescent());
    for (field, bad) in [
        ("pending_inputs", json!("unknown")),
        ("automatic_continuation", json!("possible")),
        ("external_effects", json!("pending")),
        ("turn_state", json!("running")),
        ("evidence_refs", json!([])),
        (
            "owned_processes",
            json!([{"pid":42,"launch_id":ID,"state":"running"}]),
        ),
    ] {
        let mut value = serde_json::to_value(&quiet).unwrap();
        value[field] = bad;
        let observed: QuiescenceObservation = serde_json::from_value(value).unwrap();
        assert_eq!(observed.validate(), Err(ErrorCode::InvalidInput), "{field}");
        assert!(!observed.is_confirmed_quiescent());
    }
}

#[test]
fn inspection_requires_native_binding_and_evidence_for_exact() {
    let scope: Scope = serde_json::from_value(scope()).unwrap();
    let Scope::Attempt(scope) = scope else {
        panic!("fixture scope")
    };
    let mut value = json!({"matched":"exact","binding":binding(),"selection":null,"observation":quiet(),"evidence_refs":["native-id-lookup"]});
    assert_eq!(
        serde_json::from_value::<Inspection>(value.clone())
            .unwrap()
            .validate(&scope),
        Ok(())
    );
    value["binding"] = Value::Null;
    assert_eq!(
        serde_json::from_value::<Inspection>(value.clone())
            .unwrap()
            .validate(&scope),
        Err(ErrorCode::InvalidInput)
    );
    value["matched"] = json!("absent");
    value["evidence_refs"] = json!([]);
    assert_eq!(
        serde_json::from_value::<Inspection>(value)
            .unwrap()
            .validate(&scope),
        Err(ErrorCode::InvalidInput)
    );
}

#[test]
fn failure_outcome_is_separate_from_failure_code() {
    let value = json!({"code":"PROCESS_EXITED","stage":"create","outcome":"unknown",
        "message":"lost response","provider_code":null,"retry_after_ms":null,"native_binding":null,"evidence_refs":[]});
    let failure: RuntimeFailure = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(failure.outcome, EffectOutcome::Unknown);
    assert_eq!(serde_json::to_value(failure).unwrap(), value);
}

#[test]
fn handshake_refuses_different_limits_or_protocol() {
    let mut hello: Handshake = serde_json::from_value(
        json!({"protocol":PROTOCOL,"kind":"hello","connection_id":ID,
        "adapter_version":"fixture","schema_digest":"a".repeat(64),"limits":Limits::default()}),
    )
    .unwrap();
    assert_eq!(hello.validate(), Ok(()));
    hello.limits.pending_requests += 1;
    assert_eq!(hello.validate(), Err(ErrorCode::VersionUnsupported));
}

#[test]
fn generated_schema_contains_common_selection_not_private_copy() {
    let schema = serde_json::to_value(schemars::schema_for!(RuntimeContractSchema)).unwrap();
    assert!(schema["$defs"].get("Selection").is_some());
    assert!(schema["$defs"].get("RequestedValue").is_some());
    assert_eq!(schema["$defs"]["Request"]["additionalProperties"], false);
}

fn effective() -> Value {
    let observed = json!({"reason":null,"evidence_refs":["runtime-init"],"resolved_identity":null,"availability_state":null});
    json!({"project_id":ID,"snapshot_id":ID,"provider":"codex",
        "model":{"kind":"observed","value":"runtime-model"},
        "effort":{"kind":"observed","value":"high"},"speed":{"kind":"unknown"},
        "details":{"model":observed,"effort":observed,
            "speed":{"reason":"not exposed","evidence_refs":[],"resolved_identity":null,"availability_state":null}}})
}
fn response(method: &str, payload: Value) -> Value {
    json!({"protocol":PROTOCOL,"request_id":ID,"operation_id":ID,"project_id":ID,"scope":scope(),
        "outcome":{"status":"ok","result":{"method":method,"payload":payload}}})
}
fn created() -> Value {
    response(
        "create",
        json!({"binding":binding(),"effective_selection":effective(),"admission":"ready","diagnostics":[]}),
    )
}

#[test]
fn response_correlates_operation_scope_and_method() {
    let request: Request = serde_json::from_value(create()).unwrap();
    assert_eq!(
        serde_json::from_value::<Response>(created())
            .unwrap()
            .validate_for(&request, &now()),
        Ok(())
    );
    for field in ["request_id", "operation_id", "project_id"] {
        let mut value = created();
        value[field] = json!(OTHER);
        assert_eq!(
            serde_json::from_value::<Response>(value)
                .unwrap()
                .validate_for(&request, &now()),
            Err(ErrorCode::InvalidInput)
        );
    }
    let wrong = response("rename", json!({"observed_name":"name"}));
    assert_eq!(
        serde_json::from_value::<Response>(wrong)
            .unwrap()
            .validate_for(&request, &now()),
        Err(ErrorCode::InvalidInput)
    );
}

#[test]
fn ready_requires_observed_exact_selection_blocked_requires_diagnostic() {
    let request: Request = serde_json::from_value(create()).unwrap();
    let mut value = created();
    value["outcome"]["result"]["payload"]["effective_selection"]["effort"]["value"] = json!("low");
    assert_eq!(
        serde_json::from_value::<Response>(value)
            .unwrap()
            .validate_for(&request, &now()),
        Err(ErrorCode::SelectionUnsupported)
    );
    let mut value = created();
    value["outcome"]["result"]["payload"]["admission"] = json!("blocked");
    assert_eq!(
        serde_json::from_value::<Response>(value.clone())
            .unwrap()
            .validate_for(&request, &now()),
        Err(ErrorCode::InvalidInput)
    );
    value["outcome"]["result"]["payload"]["diagnostics"] = json!(["parity not proven"]);
    assert_eq!(
        serde_json::from_value::<Response>(value)
            .unwrap()
            .validate_for(&request, &now()),
        Ok(())
    );
}

#[test]
fn resume_cannot_silently_substitute_native_session() {
    let request: Request = serde_json::from_value(request(
        "resume",
        json!({"binding":binding(),"context":context(),"selection":selection()}),
    ))
    .unwrap();
    let mut value = created();
    value["outcome"]["result"]["method"] = json!("resume");
    assert_eq!(
        serde_json::from_value::<Response>(value.clone())
            .unwrap()
            .validate_for(&request, &now()),
        Ok(())
    );
    value["outcome"]["result"]["payload"]["binding"]["native_session_id"] = json!("new-session");
    assert_eq!(
        serde_json::from_value::<Response>(value)
            .unwrap()
            .validate_for(&request, &now()),
        Err(ErrorCode::InvalidInput)
    );
}

#[test]
fn event_preserves_sequence_and_rejects_stale_generation_advancement() {
    let Scope::Attempt(active) = serde_json::from_value(scope()).unwrap() else {
        panic!("fixture scope")
    };
    let project = serde_json::from_value(json!(ID)).unwrap();
    let mut event_scope = scope();
    event_scope.as_object_mut().unwrap().remove("kind");
    let mut value = json!({"protocol":PROTOCOL,"project_id":ID,"scope":event_scope,"connection_id":ID,
        "event_seq":"9007199254740993","observed_at":"15","request_id":null,"operation_id":null,
        "native_session_id":null,"native_turn_id":null,
        "event":{"kind":"session_bound","payload":{"binding":binding(),"effective_selection":effective()}}});
    let event: Event = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(event.validate_for(&project, &active), Ok(()));
    assert_eq!(serde_json::to_value(event).unwrap(), value);
    value["scope"]["generation"] = json!("2");
    assert_eq!(
        serde_json::from_value::<Event>(value)
            .unwrap()
            .validate_for(&project, &active),
        Err(ErrorCode::StaleEpoch)
    );
}

#[test]
fn effective_unknown_and_observed_have_different_evidence_requirements() {
    assert_eq!(
        serde_json::from_value::<RuntimeEffectiveSelection>(effective())
            .unwrap()
            .validate(),
        Ok(())
    );
    let mut value = effective();
    value["details"]["speed"]["reason"] = Value::Null;
    assert_eq!(
        serde_json::from_value::<RuntimeEffectiveSelection>(value)
            .unwrap()
            .validate(),
        Err(ErrorCode::InvalidInput)
    );
    let mut value = effective();
    value["details"]["model"]["evidence_refs"] = json!([]);
    assert_eq!(
        serde_json::from_value::<RuntimeEffectiveSelection>(value)
            .unwrap()
            .validate(),
        Err(ErrorCode::InvalidInput)
    );
}

#[test]
fn method_unknown_is_not_supported_admission() {
    let mut snap = snapshot();
    snap.methods[0].state = CapabilityState::Unknown;
    snap.methods[0].reason = Some("not observed".into());
    let request: Request = serde_json::from_value(create()).unwrap();
    assert_eq!(
        request.validate(Some(&snap), &now()),
        Err(ErrorCode::VersionUnsupported)
    );
}

fn inspect_request(by_create_intent: bool) -> Request {
    serde_json::from_value(request(
        "inspect",
        if by_create_intent {
            json!({"binding":null,"create_operation_id":ID})
        } else {
            json!({"binding":binding(),"create_operation_id":null})
        },
    ))
    .unwrap()
}
fn inspected() -> Value {
    response(
        "inspect",
        json!({"inspection":{"matched":"exact","binding":binding(),
        "selection":effective(),"observation":quiet(),"evidence_refs":["native-id-lookup"]}}),
    )
}
fn check_inspect(value: Value, by_create_intent: bool) -> Result<(), ErrorCode> {
    serde_json::from_value::<Response>(value)
        .unwrap()
        .validate_for(&inspect_request(by_create_intent), &now())
}

#[test]
fn review_p1_inspect_accepts_original_binding() {
    assert_eq!(check_inspect(inspected(), false), Ok(()));
}

#[test]
fn review_p1_inspect_rejects_other_native_session() {
    let mut value = inspected();
    value["outcome"]["result"]["payload"]["inspection"]["binding"]["native_session_id"] =
        json!("native-B");
    assert_eq!(check_inspect(value, false), Err(ErrorCode::InvalidInput));
}

#[test]
fn review_p1_inspect_rejects_other_provider_even_with_matching_selection() {
    let mut value = inspected();
    value["outcome"]["result"]["payload"]["inspection"]["binding"]["provider"] = json!("pi");
    value["outcome"]["result"]["payload"]["inspection"]["selection"]["provider"] = json!("pi");
    assert_eq!(check_inspect(value, false), Err(ErrorCode::InvalidInput));
}

#[test]
fn review_p1_inspect_rejects_foreign_project_selection() {
    let mut value = inspected();
    value["outcome"]["result"]["payload"]["inspection"]["selection"]["project_id"] = json!(OTHER);
    assert_eq!(
        check_inspect(value.clone(), false),
        Err(ErrorCode::InvalidInput)
    );
    assert_eq!(check_inspect(value, true), Err(ErrorCode::InvalidInput));
}

#[test]
fn review_p1_create_intent_inspect_binds_selection_provider_and_requires_binding() {
    assert_eq!(check_inspect(inspected(), true), Ok(()));
    let mut value = inspected();
    value["outcome"]["result"]["payload"]["inspection"]["selection"]["provider"] = json!("pi");
    assert_eq!(check_inspect(value, true), Err(ErrorCode::InvalidInput));
    let mut value = inspected();
    value["outcome"]["result"]["payload"]["inspection"]["matched"] = json!("unknown");
    value["outcome"]["result"]["payload"]["inspection"]["binding"] = Value::Null;
    assert_eq!(check_inspect(value, true), Err(ErrorCode::InvalidInput));
}

#[test]
fn review_p1_snapshot_delegates_common_discovery_validation() {
    for values in [vec!["high".into(), "high".into()], vec![" ".into()]] {
        let mut snapshot = snapshot();
        snapshot.discovery.models[0].effort = mine_v1_contracts::Choices::Supported(values);
        assert_eq!(snapshot.validate(&now()), Err(ErrorCode::InvalidInput));
    }
}
