use chrono::{DateTime, Utc};
use codifix_agency::{a2a::*, discovery::*, *};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}
fn profile() -> PublicCapabilityProfile {
    serde_json::from_str(include_str!("fixtures/public-profile.json")).expect("profile")
}
#[test]
fn public_projection_only_contains_reviewed_copy_and_selected_capabilities() {
    let expected = profile();
    let manifest = SpecialistManifest {
        schema_version: 1,
        instance_id: expected.instance_id,
        specialist_id: "research".into(),
        name: "Private customer research / secret".into(),
        capabilities: BTreeSet::from(["public.research".into(), "private.read".into()]),
    };
    let approval = PublicProfileApproval {
        display_name: expected.display_name.clone(),
        summary: expected.summary.clone(),
        capabilities: expected.capabilities.clone(),
        agent_card_url: expected.agent_card_url.clone(),
    };
    let actual =
        PublicCapabilityProfile::from_approved(&manifest, approval, now()).expect("projection");
    assert_eq!(actual, expected);
    let wire = serde_json::to_string(&actual).expect("json");
    assert!(!wire.contains("secret"));
    assert!(!wire.contains("private.read"));
    let mut json: Value = serde_json::from_str(&wire).expect("json");
    json["credentials"] = json!("secret");
    assert!(serde_json::from_value::<PublicCapabilityProfile>(json).is_err());
}
#[test]
fn public_url_rejects_credentials_queries_and_internal_addresses() {
    for url in [
        "http://agents.example.com/card",
        "https://user:password@agents.example.com/card",
        "https://agents.example.com/card?token=secret",
        "https://127.0.0.1/card",
        "https://[::1]/card",
        "https://service.internal/card",
        "https://localhost/card",
    ] {
        let mut p = profile();
        p.agent_card_url = url.into();
        assert!(p.validate().is_err(), "{url}");
    }
}
#[test]
fn lexicon_fixture_matches_public_wire_fields() {
    let schema: Value = serde_json::from_str(CAPABILITY_LEXICON).expect("lexicon");
    assert_eq!(schema["id"], CAPABILITY_COLLECTION);
    assert_eq!(schema["lexicon"], 1);
    let wire = serde_json::to_value(profile()).expect("json");
    let properties = schema["defs"]["main"]["record"]["properties"]
        .as_object()
        .expect("properties");
    for key in wire.as_object().expect("object").keys() {
        assert!(key == "$type" || properties.contains_key(key), "{key}");
    }
    for required in schema["defs"]["main"]["record"]["required"]
        .as_array()
        .expect("required")
    {
        assert!(wire.get(required.as_str().expect("key")).is_some());
    }
}
#[test]
fn a2a_one_zero_fixtures_round_trip_without_legacy_fields() {
    let request: SendMessageRequest =
        serde_json::from_str(include_str!("fixtures/a2a-request.json")).expect("request");
    let response: SendMessageResponse =
        serde_json::from_str(include_str!("fixtures/a2a-response.json")).expect("response");
    let mut card = AgentCard::from_profile(
        &profile(),
        "https://agents.example.com/a2a".into(),
        "1.0.0".into(),
        "HTTP+JSON",
    )
    .expect("card");
    card.require_bearer();
    request
        .validate(&card.supported_interfaces[0], None)
        .expect("request policy");
    let wire = serde_json::to_value(card).expect("json");
    assert_eq!(wire["supportedInterfaces"][0]["protocolVersion"], "1.0");
    assert!(wire.get("protocolVersion").is_none());
    assert!(wire.get("url").is_none());
    assert_eq!(
        wire["securitySchemes"]["bearer"]["httpAuthSecurityScheme"]["scheme"],
        "Bearer"
    );
    assert_eq!(
        wire["securityRequirements"][0]["schemes"]["bearer"],
        json!({"list":[]})
    );
    let wire = serde_json::to_value(response).expect("json");
    assert_eq!(wire["task"]["status"]["state"], "TASK_STATE_SUBMITTED");
    assert!(wire["task"].get("kind").is_none());
    assert!(serde_json::from_value::<Part>(json!({"text":"a","data":{}})).is_err());
}
#[test]
fn a2a_context_and_tenant_bindings_are_checked() {
    let mut request: SendMessageRequest =
        serde_json::from_str(include_str!("fixtures/a2a-request.json")).expect("request");
    let mut card = AgentCard::from_profile(
        &profile(),
        "https://agents.example.com/a2a".into(),
        "1.0.0".into(),
        "HTTP+JSON",
    )
    .expect("card");
    card.supported_interfaces[0].tenant = Some("tenant-a".into());
    assert!(
        request
            .validate(&card.supported_interfaces[0], None)
            .is_err()
    );
    request.tenant = Some("tenant-a".into());
    let SendMessageResponse::Task(task) =
        serde_json::from_str(include_str!("fixtures/a2a-response.json")).expect("task")
    else {
        panic!("task expected")
    };
    request.message.task_id = Some(task.id.clone());
    request.message.context_id = Some("different-context".into());
    assert!(
        request
            .validate(&card.supported_interfaces[0], Some(&task))
            .is_err()
    );
    request.message.context_id = Some(task.context_id.clone());
    request
        .validate(&card.supported_interfaces[0], Some(&task))
        .expect("matched");
    assert!(
        request
            .validate(&card.supported_interfaces[0], None)
            .is_err()
    );
}

#[test]
fn private_contract_fixture_projects_completed_outputs_as_artifacts() {
    let mut contract: TaskContract =
        serde_json::from_str(include_str!("fixtures/task-contract.json")).expect("contract");
    contract.validate().expect("contract policy");
    contract
        .transition(codifix_agency::TaskStatus::Working, now())
        .expect("working");
    contract
        .finish(
            TaskOutcome {
                summary: "Version 1.0".into(),
                artifact_refs: vec![],
                evidence_refs: vec!["https://a2a-protocol.org/v1.0.0/specification/".into()],
                acceptance: vec![AcceptanceResult {
                    criterion: "cite primary source".into(),
                    passed: true,
                    evidence_ref: "https://a2a-protocol.org/v1.0.0/specification/".into(),
                }],
            },
            now(),
        )
        .expect("finished");
    let task = Task::from_contract(&contract, "context-1".into()).expect("task");
    task.validate().expect("A2A policy");
    assert_eq!(task.status.state, TaskState::Completed);
    assert_eq!(task.artifacts.len(), 1);
    let wire = serde_json::to_value(task).expect("json");
    assert!(wire["artifacts"][0]["parts"][0].get("data").is_some());
    assert!(wire["status"].get("message").is_none());
}

#[test]
fn send_configuration_preserves_defaults_presence_and_unsupported_push_request() {
    let base: SendMessageRequest =
        serde_json::from_str(include_str!("fixtures/a2a-request.json")).expect("base request");
    assert!(base.configuration.is_none());
    let default: SendMessageConfiguration =
        serde_json::from_value(json!({})).expect("default configuration");
    assert!(!default.return_immediately);
    assert!(default.history_length.is_none());
    assert!(default.task_push_notification_config.is_none());
    let request: SendMessageRequest =
        serde_json::from_str(include_str!("fixtures/a2a-request-config.json"))
            .expect("configured request");
    let config = request.configuration.as_ref().expect("configuration");
    assert!(config.return_immediately);
    assert_eq!(config.history_length, Some(0));
    assert_eq!(config.accepted_output_modes, vec!["application/json"]);
    assert_eq!(
        config
            .task_push_notification_config
            .as_ref()
            .expect("preserved for runtime rejection")["url"],
        "https://notify.example.com/"
    );
    assert_eq!(
        request.metadata.as_ref().expect("request metadata")["trace"],
        "synthetic-contract-fixture"
    );
    let wire = serde_json::to_value(&request).expect("serialize request");
    assert_eq!(wire["configuration"]["historyLength"], 0);
    assert_eq!(
        wire["message"]["parts"][0],
        json!({"data":{"kind":"inspect"}})
    );
    assert!(
        serde_json::from_value::<SendMessageConfiguration>(json!({"historyLength":-1})).is_err()
    );
    let card = AgentCard::from_profile(
        &profile(),
        "https://agents.example.com/a2a".into(),
        "1.0.0".into(),
        "HTTP+JSON",
    )
    .expect("card");
    let mut too_large = request.clone();
    too_large
        .configuration
        .as_mut()
        .expect("config")
        .history_length = Some(u32::MAX);
    assert!(
        too_large
            .validate(&card.supported_interfaces[0], None)
            .is_err()
    );
    assert_eq!(card.default_input_modes, vec!["application/json"]);
    assert_eq!(card.default_output_modes, vec!["application/json"]);
}
#[test]
fn part_accepts_optional_wire_fields_but_rejects_ambiguous_or_nonobject_data() {
    let text:Part=serde_json::from_value(json!({"text":"hello","mediaType":"text/plain","metadata":{"source":"fixture"},"futureOptional":true})).expect("text metadata");
    assert_eq!(text, Part::Text("hello".into()));
    let data: Part = serde_json::from_value(
        json!({"data":{"kind":"inspect"},"mediaType":"application/json","metadata":{}}),
    )
    .expect("data metadata");
    assert_eq!(data, Part::Data(json!({"kind":"inspect"})));
    for value in [
        json!({"text":"a","data":{}}),
        json!({"text":"a","raw":"YQ=="}),
        json!({"data":{},"url":"https://example.com/"}),
        json!({"data":null}),
        json!({"data":3}),
        json!({"data":[]}),
        json!({"metadata":{}}),
        json!({"text":"a","metadata":[]}),
    ] {
        assert!(serde_json::from_value::<Part>(value).is_err());
    }
    assert!(serde_json::from_str::<Part>(r#"{"text":"a","text":"b"}"#).is_err());
    assert!(serde_json::from_str::<Part>(r#"{"data":{},"data":{}}"#).is_err());
    assert!(serde_json::to_value(Part::Data(json!(3))).is_err());
    let schema: Value = serde_json::from_str(CAPABILITY_LEXICON).expect("lexicon");
    assert_eq!(schema["defs"]["main"]["key"], "any");
}

#[test]
fn temporary_announcements_expire_without_removing_permanent_capabilities() {
    let mut p = profile();
    p.capabilities.insert("temporary".into());
    p.expirations.push(CapabilityExpiry {
        capability: "temporary".into(),
        expires_at: now(),
    });
    p.validate().expect("valid temporary capability profile");
    p.retain_available(now() - chrono::Duration::seconds(1));
    assert!(p.capabilities.contains("temporary"));
    p.retain_available(now());
    assert_eq!(p.capabilities, profile().capabilities);
    assert!(p.expirations.is_empty());
    p.capabilities.clear();
    p.validate().expect("valid empty capability profile");
    p.expirations.push(CapabilityExpiry {
        capability: "unknown".into(),
        expires_at: now(),
    });
    assert!(p.validate().is_err());
}
