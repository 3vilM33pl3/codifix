// SPDX-License-Identifier: AGPL-3.0-or-later
//! Component rehearsal, not the unfinished Web/CLI/MCP service workflow.
use super::*;
use mem_record::{Permission, PermissionSet};

struct Origin {
    db: mem_test_support::IsolatedDatabase,
    worker: LocalProjectAccess,
    draft: CollectionDraft,
    publication: PublicationIntent,
}
impl Origin {
    async fn prepare(
        owner: InstanceId,
        requester: InstanceId,
        state: &PublicationState,
        label: &str,
    ) -> Self {
        let db = mem_test_support::IsolatedDatabase::new().await.unwrap();
        let project = Uuid::new_v4();
        let reviewer = Uuid::new_v4();
        let worker = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/SECRET_LOCAL_ROOT')",
        )
        .bind(project)
        .bind(label)
        .execute(&db.pool)
        .await
        .unwrap();
        for (principal, kind) in [(reviewer, "human_oidc"), (worker, "internal")] {
            sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,$2,'disposable F1 identity')")
                .bind(principal).bind(kind).execute(&db.pool).await.unwrap();
        }
        let review_access = LocalProjectAccess {
            project,
            principal: reviewer,
            read_only: false,
            permissions: PermissionSet::from_permissions(&[
                Permission::MemoryRead,
                Permission::MemoryCurate,
                Permission::BundleExport,
                Permission::AuthManage,
            ]),
        };
        let worker_access = LocalProjectAccess {
            project,
            principal: worker,
            read_only: false,
            permissions: PermissionSet::from_permissions(&[
                Permission::MemoryRead,
                Permission::BundleExport,
            ]),
        };
        let mut tx = db.pool.begin().await.unwrap();
        for principal in [reviewer, worker] {
            set_local_grant(
                &mut tx,
                &review_access,
                &LocalSpaceGrant {
                    project,
                    principal,
                    space: state.space(),
                    instance: owner,
                    capabilities: if principal == reviewer {
                        vec![
                            SpaceCapability::Pull,
                            SpaceCapability::Publish,
                            SpaceCapability::AnswerExpertise,
                        ]
                    } else {
                        vec![SpaceCapability::AnswerExpertise]
                    },
                },
                true,
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        let canonical = RecordId::new(Uuid::new_v4()).unwrap();
        for (id, text) in [
            (
                canonical,
                format!("{label} recovery requires verified offline backups"),
            ),
            (
                RecordId::new(Uuid::new_v4()).unwrap(),
                "recovery SECRET_UNSELECTED_MEMORY".into(),
            ),
        ] {
            insert_source(&db.pool, project, id, 1, &text).await;
        }
        let other_project = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,'other-project','other-project','/SECRET_OTHER_PROJECT_ROOT')",
        )
        .bind(other_project)
        .execute(&db.pool)
        .await
        .unwrap();
        insert_source(
            &db.pool,
            other_project,
            RecordId::new(Uuid::new_v4()).unwrap(),
            1,
            "recovery SECRET_OTHER_PROJECT",
        )
        .await;
        let evidence = EvidenceSnapshot {
            space: state.space(),
            stream: StreamId {
                origin: owner,
                canonical,
            },
            public_source_label: "Reviewed synthetic backup test".into(),
            selected_text: "A restore was verified using disposable records.".into(),
            observed_at: chrono::Utc::now(),
        };
        let blueprint = CollectionBlueprint {
            collection: RecordId::new(Uuid::new_v4()).unwrap(),
            name: label.into(),
            description: "Explicitly reviewed recovery notes".into(),
            topics: vec!["recovery".into()],
            modes: vec![AnswerMode::Deterministic],
            requesters: vec![requester],
            model_destinations: vec![],
            output_destinations: vec!["local_human".into()],
            entries: vec![CollectionSelection {
                canonical,
                evidence: vec![EvidenceSelection {
                    snapshot_digest: CanonicalJsonCodec::digest(&evidence).unwrap(),
                    snapshot: evidence,
                }],
            }],
        };
        let draft = preview_collection(&db.pool, &review_access, state.space(), owner, &blueprint)
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        let publication = activate_collection(&mut tx, state, &review_access, owner, &draft, true)
            .await
            .unwrap();
        set_collection_grant(
            &mut tx,
            &review_access,
            state.space(),
            owner,
            blueprint.collection,
            worker,
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        Self {
            db,
            worker: worker_access,
            draft,
            publication,
        }
    }
    fn question(&self, requester: InstanceId) -> ExpertiseQuestion {
        let ad = &self.draft.advertisement;
        let created_at = chrono::Utc::now();
        ExpertiseQuestion {
            request_id: Uuid::new_v4(),
            space: self.draft.snapshot.space,
            requester,
            principal: Uuid::new_v4(),
            capability: ad.key(),
            revision: ad.revision,
            collection_digest: ad.collection_digest.clone(),
            question: "How does recovery work?".into(),
            mode: AnswerMode::Deterministic,
            model_destination: None,
            output_destination: "local_human".into(),
            created_at,
            expires_at: created_at + chrono::Duration::seconds(120),
        }
    }
    async fn answer(&self, q: &ExpertiseQuestion) -> ExpertiseAnswer {
        let mut tx = self.db.pool.begin().await.unwrap();
        let (eligible, ad) =
            eligible_question_collection(&mut tx, &self.worker, self.draft.snapshot.owner, q)
                .await
                .unwrap();
        let answer = deterministic_expertise_answer(q, &ad, &eligible).unwrap();
        // Component boundary only: production must durably save the result and
        // gate every later transmission on fresh source/consent eligibility.
        tx.commit().await.unwrap();
        answer
    }
}
async fn insert_source(
    pool: &sqlx::PgPool,
    project: Uuid,
    canonical: RecordId,
    version: i32,
    text: &str,
) {
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,$4,FALSE,$5,'Recovery notes','decision','project',3,0.8,now(),now(),to_tsvector('recovery'))")
        .bind(Uuid::new_v4()).bind(project).bind(canonical.as_uuid()).bind(version).bind(text).execute(pool).await.unwrap();
}

#[tokio::test]
async fn two_origin_databases_exchange_only_reviewed_cited_excerpts() {
    let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let store = GatewayStore::new(db.pool.clone());
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = instance();
    let memory = instance();
    let homelab = instance();
    let root_token = token();
    let memory_token = token();
    let homelab_token = token();
    let security = GatewaySecurity::initial(&[42; 32], SpacePolicy::default()).unwrap();
    let server = GatewayServer::new(
        store.clone(),
        space,
        authority,
        vec![
            (root_token, authority),
            (memory_token.clone(), memory),
            (homelab_token.clone(), homelab),
        ],
        security.clone(),
    )
    .await
    .unwrap()
    .enable_expertise_rehearsal(&[(authority, 3), (memory, 3), (homelab, 3)])
    .await
    .unwrap();
    let fence = store.initialize(space, authority).await.unwrap();
    for member in [memory, homelab] {
        for capability in [
            SpaceCapability::Pull,
            SpaceCapability::Publish,
            SpaceCapability::AskExpertise,
            SpaceCapability::AnswerExpertise,
        ] {
            publish(
                &store,
                fence,
                &security,
                &intent(
                    space,
                    authority,
                    PublicationRecord::Membership { member, capability },
                ),
            )
            .await;
        }
    }
    let mut known = PublicationState::new(space, authority);
    for d in store.pull(space, authority, 0, 32).await.unwrap() {
        assert_eq!(
            known.apply(d.envelope.publisher, d),
            PublicationOutcome::Applied
        );
    }
    let memory_origin = Origin::prepare(memory, homelab, &known, "memory").await;
    let homelab_origin = Origin::prepare(homelab, memory, &known, "homelab").await;
    for origin in [&memory_origin, &homelab_origin] {
        // The committed ordered outbox intent is the only thing published;
        // the private snapshot and evidence stay in the origin database.
        publish(&store, fence, &security, &origin.publication).await;
    }
    let (endpoint, task) = listen(&server).await;
    let memory_client = GatewayClient::new(
        endpoint.clone(),
        memory_token,
        space,
        authority,
        security.trust(),
    )
    .unwrap();
    let homelab_client =
        GatewayClient::new(endpoint, homelab_token, space, authority, security.trust()).unwrap();
    for (origin, requester, requester_client, responder_client) in [
        (&memory_origin, homelab, &homelab_client, &memory_client),
        (&homelab_origin, memory, &memory_client, &homelab_client),
    ] {
        let responder = origin.draft.snapshot.owner;
        let ExpertiseReply::Capabilities(ads) = requester_client
            .expertise(
                requester,
                ExpertiseOperation::Discover {
                    after: None,
                    limit: 32,
                },
            )
            .await
            .unwrap()
        else {
            panic!("discovery");
        };
        assert_eq!(ads, vec![origin.draft.advertisement.clone()]);
        let question = origin.question(requester);
        requester_client
            .expertise(
                requester,
                ExpertiseOperation::Submit {
                    question: question.clone(),
                },
            )
            .await
            .unwrap();
        let ExpertiseReply::Lease(Some(lease)) = responder_client
            .expertise(responder, ExpertiseOperation::Claim)
            .await
            .unwrap()
        else {
            panic!("claim");
        };
        let answer = origin.answer(&lease.question).await;
        assert_eq!(answer.status, ExpertiseStatus::Answered);
        let citation = &answer.citations[0];
        let entry = &origin.draft.snapshot.entries[0];
        assert_eq!(citation.author, entry.author);
        assert_eq!(citation.source, entry.source);
        assert_eq!(citation.responder, responder);
        assert_eq!(citation.collection, origin.draft.snapshot.collection);
        assert_eq!(citation.evidence, entry.evidence);
        assert_eq!(
            citation.content_digest,
            CanonicalJsonCodec::digest(&entry.content).unwrap()
        );
        let wire = serde_json::to_string(&answer).unwrap();
        for secret in [
            "SECRET_LOCAL_ROOT",
            "SECRET_UNSELECTED_MEMORY",
            "SECRET_OTHER_PROJECT",
            "at://",
        ] {
            assert!(!wire.contains(secret), "unexpected disclosure {secret}");
        }
        let mut fabricated = answer.clone();
        fabricated.citations[0].author = AuthorIdentity::new("forged-author").unwrap();
        assert!(fabricated.validate(&question, &answer.citations).is_err());
        responder_client
            .expertise(
                responder,
                ExpertiseOperation::Complete {
                    lease: *lease,
                    answer: answer.clone(),
                },
            )
            .await
            .unwrap();
        let ExpertiseReply::Question(result) = requester_client
            .expertise(
                requester,
                ExpertiseOperation::Result {
                    request_id: question.request_id,
                },
            )
            .await
            .unwrap()
        else {
            panic!("result");
        };
        assert_eq!(result.answer.unwrap(), answer);
        // Exact source updates do not silently become approved context.
        insert_source(
            &origin.db.pool,
            origin.worker.project,
            entry.canonical,
            2,
            "recovery CHANGED_UNREVIEWED_CONTENT",
        )
        .await;
        let new_answer = origin.answer(&origin.question(requester)).await;
        assert_eq!(new_answer.status, ExpertiseStatus::Refused);
        assert!(new_answer.citations.is_empty());
        assert!(
            !serde_json::to_string(&new_answer)
                .unwrap()
                .contains("CHANGED_UNREVIEWED_CONTENT")
        );
        let mut cross_project = origin.worker.clone();
        cross_project.project = Uuid::new_v4();
        let mut tx = origin.db.pool.begin().await.unwrap();
        assert!(
            eligible_question_collection(&mut tx, &cross_project, responder, &question)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    task.abort();
    memory_origin.db.cleanup().await.unwrap();
    homelab_origin.db.cleanup().await.unwrap();
    db.cleanup().await.unwrap();
}
