// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_federation_store::{ActionRequest, FederationAction};

#[tokio::test]
async fn expertise_enrollment_actions_preserve_versions_and_exact_retries() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let instance = f.runtime.instance;
    let mut known = PublicationState::new(f.runtime.space, instance);
    let access = crate::auth::federation_local_access(&f.state, f.root_id, f.runtime.project)
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    mem_federation_store::set_local_grant(
        &mut tx,
        &access,
        &LocalSpaceGrant {
            principal: f.root_id,
            project: f.runtime.project,
            space: f.runtime.space,
            instance,
            capabilities: vec![SpaceCapability::Pull, SpaceCapability::ManageMembership],
        },
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let member = InstanceId::new(Uuid::new_v4()).unwrap();
    for (capability, version) in [
        (SpaceCapability::Pull, 2),
        (SpaceCapability::Publish, 2),
        (SpaceCapability::ManageMembership, 2),
        (SpaceCapability::AskExpertise, 3),
        (SpaceCapability::AnswerExpertise, 3),
    ] {
        for enabled in [true, false] {
            let request = ActionRequest {
                request_id: Uuid::new_v4(),
                action: FederationAction::Membership {
                    member,
                    capability,
                    enabled,
                },
            };
            let response =
                mem_federation_store::submit_action(&f.pool, &known, &access, instance, &request)
                    .await
                    .unwrap();
            let retry =
                mem_federation_store::submit_action(&f.pool, &known, &access, instance, &request)
                    .await
                    .unwrap();
            assert_eq!(response.record, retry.record);
            let key = response.record.unwrap();
            let envelope: sqlx::types::Json<PublicationEnvelope> = sqlx::query_scalar(
                "SELECT envelope FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND record_id=$3")
                .bind(f.runtime.space.as_uuid()).bind(instance.as_uuid()).bind(key.id.as_uuid())
                .fetch_one(&f.pool).await.unwrap();
            assert_eq!(envelope.schema_version, version);
            assert!(envelope.supported_schema());
            let delivery = PublicationDelivery::new(
                AuthoritySequence::new(known.sequence() + 1).unwrap(),
                RecordLocator::new(format!("fixture:{}", key.id.as_uuid())).unwrap(),
                envelope.0,
            )
            .unwrap();
            assert_eq!(known.apply(instance, delivery), PublicationOutcome::Applied);
            assert_eq!(known.has_capability(member, capability), enabled);
            let mut altered = request.clone();
            altered.action = FederationAction::Membership {
                member,
                capability,
                enabled: !enabled,
            };
            assert!(
                mem_federation_store::submit_action(&f.pool, &known, &access, instance, &altered)
                    .await
                    .is_err()
            );
        }
    }
    let wrong_authority = PublicationState::new(f.runtime.space, member);
    let denied = ActionRequest {
        request_id: Uuid::new_v4(),
        action: FederationAction::Membership {
            member,
            capability: SpaceCapability::AskExpertise,
            enabled: true,
        },
    };
    assert!(
        mem_federation_store::submit_action(&f.pool, &wrong_authority, &access, instance, &denied)
            .await
            .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_source_outbox WHERE space=$1")
            .bind(f.runtime.space.as_uuid())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 10);
}
