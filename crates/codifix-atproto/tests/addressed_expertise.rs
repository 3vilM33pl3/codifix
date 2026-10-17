// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use codifix_atproto::addressed_expertise::*;
use codifix_federation::{CanonicalJsonCodec, expertise::*};
use serde_json::json;
use uuid::Uuid;

fn question() -> ExpertiseQuestion {
    let requester = Uuid::from_u128(11);
    let responder = Uuid::from_u128(12);
    serde_json::from_value(json!({
        "request_id":Uuid::from_u128(13),"space":Uuid::from_u128(14),
        "requester":requester,"principal":Uuid::from_u128(15),
        "capability":{"publisher":responder,"id":Uuid::from_u128(16)},
        "revision":1,"collection_digest":CanonicalJsonCodec::digest(&"reviewed").unwrap(),
        "question":"What changed?","mode":"deterministic","model_destination":null,
        "output_destination":"codex_openai",
        "created_at":"2026-10-04T10:00:00Z","expires_at":"2026-10-04T10:02:00Z"
    }))
    .unwrap()
}

#[test]
fn addressed_question_and_answer_have_distinct_deterministic_keys() {
    let q = question();
    let policy_digest = CanonicalJsonCodec::digest(&"agent policy").unwrap();
    let request = AddressedExpertiseRecord::question(q.clone(), 1, policy_digest.clone()).unwrap();
    let answer: ExpertiseAnswer = serde_json::from_value(json!({
        "request_digest":q.digest().unwrap(),"status":"refused","text":"",
        "citations":[],"insufficient_evidence":true,"reason":null,"untrusted":true
    }))
    .unwrap();
    let reply = AddressedExpertiseRecord::answer(
        &q,
        answer,
        1,
        policy_digest,
        "2026-10-04T10:01:00Z".parse().unwrap(),
    )
    .unwrap();
    let request_wire = AddressedExpertiseWire::encode(request.clone()).unwrap();
    let reply_wire = AddressedExpertiseWire::encode(reply.clone()).unwrap();
    assert_eq!(
        request_wire.record().unwrap().collection.as_ref(),
        QUESTION_COLLECTION
    );
    assert_eq!(
        reply_wire.record().unwrap().collection.as_ref(),
        ANSWER_COLLECTION
    );
    assert_ne!(
        request_wire.record().unwrap().rkey,
        reply_wire.record().unwrap().rkey
    );
    assert_eq!(
        AddressedExpertiseWire::encode(request).unwrap(),
        request_wire
    );
    for wire in [&request_wire, &reply_wire] {
        let repo = "did:web:space.federation.example.com";
        let body = create_request(wire, repo).unwrap();
        assert_eq!(body["record"], serde_json::to_value(wire.raw()).unwrap());
        assert_eq!(body["validate"], false);
        let readback = json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),
            "cid":wire.cid().unwrap(),"value":wire.raw()});
        assert!(verify_readback(wire, repo, &readback).unwrap());
    }
}

#[test]
fn addressed_records_reject_wrong_type_tampering_and_oversize() {
    let q = question();
    let record =
        AddressedExpertiseRecord::question(q, 1, CanonicalJsonCodec::digest(&"policy").unwrap())
            .unwrap();
    let wire = AddressedExpertiseWire::encode(record).unwrap();
    let mut extended = serde_json::to_value(wire.raw()).unwrap();
    extended["future_extension"] = json!({"retained":true});
    let decoded =
        AddressedExpertiseWire::decode(serde_json::from_value(extended.clone()).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(decoded.raw()).unwrap(), extended);
    for (pointer, replacement) in [
        ("/$type", json!(ANSWER_COLLECTION)),
        ("/addressed/payload_digest/hex", json!("a".repeat(64))),
        ("/addressed/recipient", json!(Uuid::from_u128(99))),
        ("/addressed/schema_version", json!(2)),
    ] {
        let mut changed = serde_json::to_value(wire.raw()).unwrap();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(AddressedExpertiseWire::decode(serde_json::from_value(changed).unwrap()).is_err());
    }
    let mut oversized = serde_json::to_value(wire.raw()).unwrap();
    oversized["padding"] = json!("x".repeat(MAX_RECORD_BYTES));
    assert!(AddressedExpertiseWire::decode(serde_json::from_value(oversized).unwrap()).is_err());
}

#[test]
fn unpublished_lexicons_have_distinct_ids() {
    for (name, value) in [
        (
            QUESTION_COLLECTION,
            include_str!("../publication-lexicons/dev.codifix.unspecced.expertiseQuestion.json"),
        ),
        (
            ANSWER_COLLECTION,
            include_str!("../publication-lexicons/dev.codifix.unspecced.expertiseAnswer.json"),
        ),
    ] {
        let lexicon: serde_json::Value = serde_json::from_str(value).unwrap();
        assert_eq!(lexicon["id"], name);
        assert_eq!(lexicon["defs"]["main"]["type"], "record");
    }
}
