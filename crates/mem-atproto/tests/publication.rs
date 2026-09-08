// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use mem_atproto::publication::*;
use mem_federation::{CanonicalJsonCodec, publication::*};
use serde_json::{Value, json};
use uuid::Uuid;

fn samples() -> Vec<PublicationDelivery> {
    let id = Uuid::from_u128(1);
    let other = Uuid::from_u128(2);
    let stream = json!({"origin":id,"canonical":other});
    let key = json!({"publisher":id,"id":other});
    let source = json!({"record_id":other,"version":27});
    let content = json!({"text":"timeout 30","summary":"reviewed","memory_type":"decision",
        "confidence":800001,"importance":3,"tags":["shared"]});
    let snapshot = json!({"space":id,"stream":stream,"public_source_label":"reviewed extract",
        "selected_text":"selected bytes","observed_at":"2026-09-08T00:00:00Z"});
    let payloads = vec![
        ("membership", json!({"member":other,"capability":"publish"})),
        ("revocation", json!({"member":other,"capability":null})),
        (
            "evidence",
            json!({"snapshot":snapshot,"review":{
            "snapshot_digest":CanonicalJsonCodec::digest(&snapshot).unwrap(),
            "reviewer":"expert","reviewed_at":"2026-09-08T00:00:00Z"}}),
        ),
        (
            "memory_version",
            json!({"stream":stream,"publication":{"version":1,
            "source":source,"content":content,"evidence":[key]}}),
        ),
        (
            "canonical_state",
            json!({"stream":stream,"state":{"revision":2,
            "content_version":1,"status":"active","confidence":800001,"tombstone":true}}),
        ),
        (
            "relation",
            json!({"id":other,"source":stream,"target":stream,
            "relation_type":"related_to","revision":2,"active":false}),
        ),
        (
            "proposal",
            json!({"target":stream,"base_version":1,"content":content,"evidence":[key]}),
        ),
        ("resolve_proposal", json!({"proposal":key,"source":source})),
    ];
    payloads.into_iter().enumerate().map(|(index,(kind,payload))| {
        let envelope = serde_json::from_value(json!({"schema_version":2,"space":id,
            "publisher":id,"record_id":Uuid::from_u128(index as u128+10),"author":"original-expert",
            "authored_at":"2026-09-08T00:00:00Z","causation":key,"record":{"kind":kind,"payload":payload}})).unwrap();
        PublicationDelivery::new(AuthoritySequence::new(index as u32+1).unwrap(),
            mem_federation::RecordLocator::new(format!("record:{index}")).unwrap(),envelope).unwrap()
    }).collect()
}

// Extend every object, including nested evidence references and review digests.
fn extend(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for value in map.values_mut() {
                extend(value);
            }
            map.insert("future_extension".into(), json!({"retained":"exactly"}));
        }
        Value::Array(values) => {
            for value in values {
                extend(value);
            }
        }
        _ => {}
    }
}

#[test]
fn all_eight_variants_retain_nested_extensions_and_exact_transport_identity() {
    for delivery in samples() {
        let wire = PublicationWire::encode(delivery.clone()).unwrap();
        let mut raw = serde_json::to_value(wire.raw()).unwrap();
        extend(&mut raw);
        let decoded =
            PublicationWire::decode(serde_json::from_value(raw.clone()).unwrap()).unwrap();
        assert_eq!(decoded.delivery(), &delivery);
        assert_eq!(serde_json::to_value(decoded.raw()).unwrap(), raw);
        assert_ne!(wire.cid().unwrap(), decoded.cid().unwrap()); // extensions affect CID, not neutral digest
        let repo = "did:web:space.federation.example.com";
        let request = create_request(&decoded, repo).unwrap();
        assert_eq!(request["record"], raw);
        assert_eq!(request["collection"], COLLECTION);
        assert_eq!(request["validate"], false);
        assert!(request.get("swapCommit").is_none());
        Uuid::parse_str(request["rkey"].as_str().unwrap()).unwrap();
        let mut readback = json!({"uri":decoded.uri(&repo.parse().unwrap()).unwrap(),
            "cid":decoded.cid().unwrap(),"value":raw});
        assert!(verify_readback(&decoded, repo, &readback).unwrap());
        readback["cid"] = json!(wire.cid().unwrap());
        assert!(!verify_readback(&decoded, repo, &readback).unwrap());
    }
}

#[test]
fn unknown_semantics_and_modified_known_bytes_are_not_extensions() {
    let wire = PublicationWire::encode(samples().remove(3)).unwrap();
    for (pointer, value) in [
        ("/delivery/envelope/schema_version", json!(3)),
        (
            "/delivery/envelope/record/kind",
            json!("execute_remote_code"),
        ),
        (
            "/delivery/envelope/record/payload/publication/content/text",
            json!("forged"),
        ),
        (
            "/delivery/envelope/record/payload/publication/content/confidence",
            json!(0.8),
        ),
        ("/delivery/envelope/publisher", json!(Uuid::nil())),
    ] {
        let mut raw = serde_json::to_value(wire.raw()).unwrap();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value(raw)
                .ok()
                .and_then(|raw| PublicationWire::decode(raw).ok())
                .is_none()
        );
    }
}

#[test]
fn deterministic_keys_are_scoped_to_origin_and_space() {
    let delivery = samples().remove(0);
    let wire = PublicationWire::encode(delivery.clone()).unwrap();
    assert_eq!(
        wire.record().unwrap().rkey,
        PublicationWire::encode(delivery.clone())
            .unwrap()
            .record()
            .unwrap()
            .rkey
    );
    for change_space in [false, true] {
        let mut envelope = delivery.envelope.clone();
        if change_space {
            envelope.space = SpaceId::new(Uuid::from_u128(77)).unwrap();
        } else {
            envelope.publisher = InstanceId::new(Uuid::from_u128(77)).unwrap();
        }
        let changed = PublicationWire::encode(
            PublicationDelivery::new(delivery.sequence, delivery.locator.clone(), envelope)
                .unwrap(),
        )
        .unwrap();
        assert_ne!(wire.record().unwrap().rkey, changed.record().unwrap().rkey);
    }
}
