#![allow(clippy::unwrap_used)]
use chrono::{Duration, Utc};
use codifix_agency::{Budget, Mandate, MandateStatus, ResourceScope, Usage};
use codifix_inbox::{store::*, *};
use codifix_test_support::IsolatedDatabase;
use serde_json::json;
use uuid::Uuid;
struct Fixture {
    db: IsolatedDatabase,
    store: Store,
    human: Uuid,
    project: Uuid,
    grant: InvocationGrant,
}
impl Fixture {
    async fn new(membership: bool) -> Option<Self> {
        let db = IsolatedDatabase::new().await?;
        let home = Uuid::new_v4();
        let human = Uuid::new_v4();
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'human_oidc','Boundary human','admin')").bind(human).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Boundary project','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
        if membership {
            sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')").bind(Uuid::new_v4()).bind(human).bind(project).execute(&db.pool).await.unwrap();
        }
        let m = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: home,
            subject_instance_id: home,
            specialist_id: "planner".into(),
            capabilities: ["conversation".into(), "inspect".into()].into(),
            resources: [ResourceScope {
                kind: "project".into(),
                id: project.to_string(),
            }]
            .into(),
            destinations: Default::default(),
            expires_at: Utc::now() + Duration::hours(2),
            status: MandateStatus::Active,
            budget: Budget {
                micro_usd: 100000,
                calls: 100,
                turns: 100,
                delegation_depth: 2,
            },
        };
        sqlx::query(
            "INSERT INTO agency_mandates(id,project_id,subject_id,document) VALUES($1,$2,$3,$4)",
        )
        .bind(m.id)
        .bind(project)
        .bind(&m.specialist_id)
        .bind(json!(m))
        .execute(&db.pool)
        .await
        .unwrap();
        let request = json!({"request_id":"template","mandate_id":m.id,"specialist_id":"planner","objective":"Discuss plan","acceptance_criteria":["conversation_reply_valid"],"capability":"conversation","resource":{"kind":"project","id":project},"deadline":Utc::now()+Duration::hours(1),"budget":{"micro_usd":1000,"calls":16,"turns":8,"delegation_depth":1},"payload":{"kind":"conversation_turn"},"parent_id":null});
        let mut action = request.clone();
        action["capability"] = json!("inspect");
        action["payload"] = json!({"kind":"inspect"});
        let grant = InvocationGrant {
            id: Uuid::new_v4(),
            home_instance_id: home,
            principal_id: human,
            contact: Specialist {
                id: Uuid::new_v4(),
                instance_id: home,
                specialist_id: "planner".into(),
                display_name: "Planner".into(),
                summary: "Synthetic".into(),
                capabilities: vec!["conversation".into()],
                enrolled: true,
                available: true,
                grant_id: None,
                mandate_id: None,
                remote: false,
            },
            request,
            routing: json!({}),
            actions: vec![ActionTemplate {
                key: "inspect".into(),
                request: action,
            }],
            expires_at: Utc::now() + Duration::hours(1),
        };
        Some(Self {
            store: Store {
                pool: db.pool.clone(),
                home_instance_id: home,
            },
            db,
            human,
            project,
            grant,
        })
    }
    async fn create(&self) -> Detail {
        self.store.install_grant(&self.grant).await.unwrap();
        self.store
            .create(
                self.human,
                &CreateConversation {
                    client_request_id: Uuid::new_v4(),
                    title: "Boundaries".into(),
                    specialist_id: self.grant.contact.id,
                },
            )
            .await
            .unwrap()
    }
    fn msg(&self, body: &str, recipients: Vec<Uuid>) -> SendMessage {
        SendMessage {
            client_message_id: Uuid::new_v4(),
            body: body.into(),
            format: "text".into(),
            artifact: None,
            recipient_member_ids: recipients,
        }
    }
    async fn invite(&self, c: Uuid, summary: &str, selected: Vec<Uuid>) -> Uuid {
        let mut g = self.grant.clone();
        g.id = Uuid::new_v4();
        g.contact.id = Uuid::new_v4();
        self.store.install_grant(&g).await.unwrap();
        let member = Uuid::new_v4();
        self.store
            .add_member(
                self.human,
                c,
                &AddMember {
                    client_request_id: member,
                    specialist_id: g.contact.id,
                    summary: summary.into(),
                    selected_message_ids: selected,
                    confirmed: true,
                },
            )
            .await
            .unwrap();
        member
    }
    async fn tickets(&self) -> Vec<DispatchTicket> {
        pending_dispatches(&self.db.pool, self.store.home_instance_id)
            .await
            .unwrap()
    }
    async fn redeem(&self, ticket: &DispatchTicket) -> Uuid {
        let id = Uuid::new_v4();
        let mut tx = self.db.pool.begin().await.unwrap();
        admission(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO agency_tasks(id,project_id,mandate_id,requester,request_id,contract,payload,request_digest) VALUES($1,$2,$3,'boundary-test',$4,'{}'::jsonb,$5,$6)").bind(id).bind(self.project).bind(ticket.request["mandate_id"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(id.to_string()).bind(&ticket.request).bind(digest(&ticket.request).unwrap()).execute(&mut *tx).await.unwrap();
        redeem_dispatch(
            &mut tx,
            ticket.id,
            id,
            &digest(&ticket.request).unwrap(),
            self.store.home_instance_id,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }
    async fn reply(&self, task: Uuid, text: &str, peer: Option<Uuid>) {
        let peers = peer
            .map(|id| vec![json!({"specialist_id":id,"message":text})])
            .unwrap_or_default();
        record_task_result(
            &self.db.pool,
            task,
            "completed",
            None,
            Some(&json!({"conversation_reply":{"version":1,"reply":text,"peer_requests":peers}})),
        )
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn human_global_role_is_not_project_authority() {
    let Some(f) = Fixture::new(false).await else {
        return;
    };
    assert!(f.store.install_grant(&f.grant).await.is_err());
    sqlx::query(
        "UPDATE auth_principals SET global_permissions_json='[\"memory_read\"]'::jsonb WHERE id=$1",
    )
    .bind(f.human)
    .execute(&f.db.pool)
    .await
    .unwrap();
    f.store.install_grant(&f.grant).await.unwrap();
    sqlx::query("UPDATE auth_principals SET global_permissions_json='[]'::jsonb WHERE id=$1")
        .bind(f.human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(
        f.store
            .create(
                f.human,
                &CreateConversation {
                    client_request_id: Uuid::new_v4(),
                    title: "Denied".into(),
                    specialist_id: f.grant.contact.id
                }
            )
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn principal_and_membership_stay_locked_through_effect_admission() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(f.human, c.conversation.id, &f.msg("Inspect", vec![]))
        .await
        .unwrap();
    let ticket = f.tickets().await.remove(0);
    let task = f.redeem(&ticket).await;
    let mut tx = f.db.pool.begin().await.unwrap();
    admission(&mut tx).await.unwrap();
    authorize_task_origin(
        &mut tx,
        task,
        "conversation",
        &serde_json::from_value(ticket.request["resource"].clone()).unwrap(),
        Usage::default(),
    )
    .await
    .unwrap();
    for query in [
        "UPDATE auth_principals SET disabled_at=now() WHERE id=$1",
        "DELETE FROM auth_project_memberships WHERE principal_id=$1",
    ] {
        let mut other = f.db.pool.begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout='100ms'")
            .execute(&mut *other)
            .await
            .unwrap();
        let error = sqlx::query(query)
            .bind(f.human)
            .execute(&mut *other)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("55P03")
        );
        other.rollback().await.unwrap();
    }
    tx.commit().await.unwrap();
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn mismatched_contact_and_unsupported_action_templates_are_rejected() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let mut g = f.grant.clone();
    g.contact.specialist_id = "imposter".into();
    assert!(f.store.install_grant(&g).await.is_err());
    g = f.grant.clone();
    g.contact.instance_id = Uuid::new_v4();
    assert!(f.store.install_grant(&g).await.is_err());
    g = f.grant.clone();
    g.actions[0].request["payload"]["kind"] = json!("model_draft");
    assert!(f.store.install_grant(&g).await.is_err());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn late_reply_cannot_smuggle_unshared_history_into_new_member_context() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("EXCLUDED SECRET", vec![]),
        )
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    let new = f
        .invite(c.conversation.id, "Only a public summary", vec![])
        .await;
    f.reply(task, "The old detail was EXCLUDED SECRET", None)
        .await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Discuss public roadmap", vec![new]),
        )
        .await
        .unwrap();
    let ctx = f.tickets().await.remove(0).request["payload"]["turn"]["context"].to_string();
    assert!(!ctx.contains("EXCLUDED SECRET"));
    assert!(ctx.contains("Only a public summary"));
    assert!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .messages
            .iter()
            .any(|m| m.body.contains("old detail"))
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn peer_request_inherits_source_context_and_cannot_rephrase_excluded_history() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("EXCLUDED SECRET", vec![]),
        )
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "Acknowledged", None).await;
    let new = f.invite(c.conversation.id, "Public summary", vec![]).await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Coordinate with reviewer", vec![]),
        )
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "EXCLUDED SECRET reframed as a request", Some(new))
        .await;
    assert!(f.tickets().await.is_empty());
    assert!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .messages
            .iter()
            .any(|m| m.role == "system")
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn explicit_selected_reply_is_new_disclosure_and_compatible_peers_coordinate() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(f.human, c.conversation.id, &f.msg("OLD DETAIL", vec![]))
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "Reviewed excerpt OLD DETAIL", None).await;
    let reply_id = Uuid::new_v5(&task, b"conversation-reply");
    let new = f.invite(c.conversation.id, "", vec![reply_id]).await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Review selected excerpt", vec![new]),
        )
        .await
        .unwrap();
    let ticket = f.tickets().await.remove(0);
    assert!(
        ticket.request["payload"]["turn"]["context"]
            .to_string()
            .contains("Reviewed excerpt OLD DETAIL")
    );
    let task = f.redeem(&ticket).await;
    f.reply(task, "Discuss the approved excerpt", Some(c.members[0].id))
        .await;
    assert_eq!(f.tickets().await.len(), 1);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn configured_turn_limit_is_shared_across_peer_requests() {
    let Some(mut f) = Fixture::new(true).await else {
        return;
    };
    f.grant.request["budget"]["turns"] = json!(2);
    let c = f.create().await;
    let new = f.invite(c.conversation.id, "", vec![]).await;
    f.store
        .send(f.human, c.conversation.id, &f.msg("Coordinate", vec![]))
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "Ask peer", Some(new)).await;
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "Ask original", Some(c.members[0].id)).await;
    assert!(f.tickets().await.is_empty());
    let turns: i32 = sqlx::query_scalar("SELECT turns FROM inbox_roots")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(turns, 2);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn removed_member_rejoins_with_a_fresh_disclosure_boundary() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Old private message", vec![]),
        )
        .await
        .unwrap();
    f.store
        .remove_member(f.human, c.conversation.id, c.members[0].id)
        .await
        .unwrap();
    let new = Uuid::new_v4();
    f.store
        .add_member(
            f.human,
            c.conversation.id,
            &AddMember {
                client_request_id: new,
                specialist_id: f.grant.contact.id,
                summary: "Fresh context".into(),
                selected_message_ids: vec![],
                confirmed: true,
            },
        )
        .await
        .unwrap();
    f.store.lead(f.human, c.conversation.id, new).await.unwrap();
    f.store
        .send(f.human, c.conversation.id, &f.msg("New topic", vec![]))
        .await
        .unwrap();
    let t = f.tickets().await.remove(0);
    assert!(!t.request.to_string().contains("Old private message"));
    assert!(t.request.to_string().contains("Fresh context"));
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn pagination_and_unread_counts_do_not_count_owner_messages() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    for i in 0..3 {
        f.store
            .send(
                f.human,
                c.conversation.id,
                &f.msg(&format!("Message {i}"), vec![]),
            )
            .await
            .unwrap();
    }
    assert_eq!(f.store.list(f.human).await.unwrap()[0].unread_count, 0);
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    f.reply(task, "One unread reply", None).await;
    assert_eq!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .conversation
            .unread_count,
        1
    );
    let page = f
        .store
        .message_page(f.human, c.conversation.id, i64::MAX, 2)
        .await
        .unwrap();
    assert_eq!(page.messages.len(), 2);
    assert!(page.has_more);
    assert!(page.messages[0].sequence < page.messages[1].sequence);
    let older = f
        .store
        .message_page(f.human, c.conversation.id, page.messages[0].sequence, 2)
        .await
        .unwrap();
    assert_eq!(older.messages.len(), 2);
    assert!(!older.has_more);
    assert!(
        f.store
            .message_page(Uuid::new_v4(), c.conversation.id, i64::MAX, 2)
            .await
            .is_err()
    );
    f.store
        .read(f.human, c.conversation.id, page.messages[1].sequence)
        .await
        .unwrap();
    assert_eq!(f.store.list(f.human).await.unwrap()[0].unread_count, 0);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn combined_message_and_artifact_limit_is_checked_before_persistence() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    let mut req = f.msg(&"a".repeat(20000), vec![]);
    req.artifact = Some(json!({"content":"b".repeat(20000)}));
    assert!(
        f.store
            .send(f.human, c.conversation.id, &req)
            .await
            .is_err()
    );
    assert!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .messages
            .is_empty()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn cancellation_card_clears_after_command_completes_without_task_revision_change() {
    let Some(f) = Fixture::new(true).await else {
        return;
    };
    let c = f.create().await;
    f.store
        .send(f.human, c.conversation.id, &f.msg("Work", vec![]))
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    record_task_result(&f.db.pool, task, "working", None, None)
        .await
        .unwrap();
    f.store
        .cancel(f.human, c.conversation.id, task)
        .await
        .unwrap();
    record_task_result(&f.db.pool, task, "working", None, None)
        .await
        .unwrap();
    assert!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .tasks[0]
            .cancel_requested
    );
    sqlx::query("UPDATE inbox_task_commands SET done=true WHERE task_id=$1")
        .bind(task)
        .execute(&f.db.pool)
        .await
        .unwrap();
    record_task_result(&f.db.pool, task, "working", None, None)
        .await
        .unwrap();
    assert!(
        !f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .tasks[0]
            .cancel_requested
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn revoking_action_project_blocks_effect_while_chat_project_remains_accessible() {
    let Some(mut f) = Fixture::new(true).await else {
        return;
    };
    let action_project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Action project','/synthetic')",
    )
    .bind(action_project)
    .bind(action_project.to_string())
    .execute(&f.db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')").bind(Uuid::new_v4()).bind(f.human).bind(action_project).execute(&f.db.pool).await.unwrap();
    let mut mandate: Mandate = serde_json::from_value(
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT document FROM agency_mandates WHERE id=$1",
        )
        .bind(
            f.grant.request["mandate_id"]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        )
        .fetch_one(&f.db.pool)
        .await
        .unwrap(),
    )
    .unwrap();
    mandate.id = Uuid::new_v4();
    mandate.resources = [ResourceScope {
        kind: "project".into(),
        id: action_project.to_string(),
    }]
    .into();
    sqlx::query(
        "INSERT INTO agency_mandates(id,project_id,subject_id,document) VALUES($1,$2,$3,$4)",
    )
    .bind(mandate.id)
    .bind(action_project)
    .bind(&mandate.specialist_id)
    .bind(json!(mandate))
    .execute(&f.db.pool)
    .await
    .unwrap();
    f.grant.actions[0].request["mandate_id"] = json!(mandate.id);
    f.grant.actions[0].request["resource"] = json!({"kind":"project","id":action_project});
    let c = f.create().await;
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Inspect action project", vec![]),
        )
        .await
        .unwrap();
    let task = f.redeem(&f.tickets().await.remove(0)).await;
    record_task_result(&f.db.pool,task,"completed",None,Some(&json!({"conversation_reply":{"version":1,"reply":"Inspecting","actions":[{"key":"inspect","payload":{"kind":"inspect"}}]}}))).await.unwrap();
    let action_ticket = f.tickets().await.remove(0);
    let action_task = f.redeem(&action_ticket).await;
    sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2")
        .bind(f.human)
        .bind(action_project)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    admission(&mut tx).await.unwrap();
    assert!(
        authorize_task_origin(
            &mut tx,
            action_task,
            "inspect",
            &serde_json::from_value(action_ticket.request["resource"].clone()).unwrap(),
            Usage::default()
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    f.store
        .send(
            f.human,
            c.conversation.id,
            &f.msg("Chat remains authorized", vec![]),
        )
        .await
        .unwrap();
    f.db.cleanup().await.unwrap();
}
