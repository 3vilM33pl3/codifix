#![allow(clippy::unwrap_used)]
use chrono::{Duration, Utc};
use codifix_agency::{Budget, Mandate, MandateStatus, ResourceScope, Usage};
use codifix_inbox::{store::*, *};
use codifix_test_support::IsolatedDatabase;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;
struct Fixture {
    db: IsolatedDatabase,
    store: Store,
    human: Uuid,
    project: Uuid,
    grant: InvocationGrant,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = IsolatedDatabase::new().await?;
        let home = Uuid::new_v4();
        let human = Uuid::new_v4();
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'human_oidc','Synthetic human','admin')").bind(human).execute(&db.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Inbox test','/synthetic')",
        )
        .bind(project)
        .bind(project.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')").bind(Uuid::new_v4()).bind(human).bind(project).execute(&db.pool).await.unwrap();
        let m = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: home,
            subject_instance_id: home,
            specialist_id: "planner".into(),
            capabilities: ["conversation_turn".into(), "inspect".into()].into(),
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
        let request = json!({"request_id":"template","mandate_id":m.id,"specialist_id":"planner","objective":"Discuss the plan","acceptance_criteria":["conversation_reply_valid"],"capability":"conversation_turn","resource":{"kind":"project","id":project},"deadline":Utc::now()+Duration::hours(1),"budget":{"micro_usd":1000,"calls":16,"turns":8,"delegation_depth":1},"payload":{"kind":"conversation_turn"},"parent_id":null});
        let mut action = request.clone();
        action["capability"] = json!("inspect");
        action["payload"] = json!({"kind":"inspect"});
        action["acceptance_criteria"] = json!(["status_observed"]);
        let grant = InvocationGrant {
            id: Uuid::new_v4(),
            home_instance_id: home,
            principal_id: human,
            contact: Specialist {
                id: Uuid::new_v4(),
                instance_id: home,
                specialist_id: "planner".into(),
                display_name: "Planner".into(),
                summary: "Synthetic specialist".into(),
                capabilities: vec!["conversation_turn".into()],
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
        let store = Store {
            pool: db.pool.clone(),
            home_instance_id: home,
        };
        store.install_grant(&grant).await.unwrap();
        Some(Self {
            db,
            store,
            human,
            project,
            grant,
        })
    }
    async fn conversation(&self) -> Detail {
        self.store
            .create(
                self.human,
                &CreateConversation {
                    client_request_id: Uuid::new_v4(),
                    title: "Roadmap".into(),
                    specialist_id: self.grant.contact.id,
                },
            )
            .await
            .unwrap()
    }
    fn msg(&self, body: &str) -> SendMessage {
        SendMessage {
            client_message_id: Uuid::new_v4(),
            body: body.into(),
            format: "text".into(),
            artifact: None,
            recipient_member_ids: vec![],
        }
    }
    async fn redeem(&self, t: &DispatchTicket) -> Uuid {
        let id = Uuid::new_v4();
        let mut tx = self.db.pool.begin().await.unwrap();
        admission(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO agency_tasks(id,project_id,mandate_id,requester,request_id,contract,payload,request_digest) VALUES($1,$2,$3,'inbox-test',$4,'{}'::jsonb,$5,$6)").bind(id).bind(self.project).bind(self.grant.request["mandate_id"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(id.to_string()).bind(&t.request).bind(digest(&t.request).unwrap()).execute(&mut *tx).await.unwrap();
        redeem_dispatch(
            &mut tx,
            t.id,
            id,
            &digest(&t.request).unwrap(),
            self.store.home_instance_id,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }
}
#[tokio::test]
async fn owner_home_and_idempotent_message_isolation() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let c = f.conversation().await;
    let msg = f.msg("Plan the milestone");
    f.store
        .send(f.human, c.conversation.id, &msg)
        .await
        .unwrap();
    let again = f
        .store
        .send(f.human, c.conversation.id, &msg)
        .await
        .unwrap();
    assert_eq!(again.messages.len(), 1);
    assert_eq!(
        pending_dispatches(&f.db.pool, f.store.home_instance_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let mut changed = msg.clone();
    changed.body = "Changed".into();
    assert!(
        f.store
            .send(f.human, c.conversation.id, &changed)
            .await
            .is_err()
    );
    assert!(
        f.store
            .detail(Uuid::new_v4(), c.conversation.id)
            .await
            .is_err()
    );
    let wrong = Store {
        pool: f.db.pool.clone(),
        home_instance_id: Uuid::new_v4(),
    };
    assert!(wrong.detail(f.human, c.conversation.id).await.is_err());
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(f.store.detail(f.human, c.conversation.id).await.is_err());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn joining_disclosure_excludes_unselected_history_and_removed_lead_pauses() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let c = f.conversation().await;
    let old = f.msg("PRIVATE EARLIER DETAIL");
    f.store
        .send(f.human, c.conversation.id, &old)
        .await
        .unwrap();
    let mut second = f.grant.clone();
    second.id = Uuid::new_v4();
    second.contact.id = Uuid::new_v4();
    second.contact.display_name = "Reviewer".into();
    f.store.install_grant(&second).await.unwrap();
    let mut add = AddMember {
        client_request_id: Uuid::new_v4(),
        specialist_id: second.contact.id,
        summary: "Public roadmap summary".into(),
        selected_message_ids: vec![],
        confirmed: false,
    };
    assert!(
        f.store
            .add_member(f.human, c.conversation.id, &add)
            .await
            .is_err()
    );
    add.confirmed = true;
    f.store
        .add_member(f.human, c.conversation.id, &add)
        .await
        .unwrap();
    let mut msg = f.msg("Review this");
    msg.recipient_member_ids = vec![add.client_request_id];
    f.store
        .send(f.human, c.conversation.id, &msg)
        .await
        .unwrap();
    let tickets = pending_dispatches(&f.db.pool, f.store.home_instance_id)
        .await
        .unwrap();
    let request = &tickets
        .iter()
        .find(|t| {
            t.request["payload"]["turn"]["turn_id"]
                == Uuid::new_v5(&msg.client_message_id, add.client_request_id.as_bytes())
                    .to_string()
        })
        .unwrap()
        .request;
    let ctx = request["payload"]["turn"]["context"].to_string();
    assert!(ctx.contains("Public roadmap summary"));
    assert!(!ctx.contains("PRIVATE EARLIER DETAIL"));
    f.store
        .remove_member(f.human, c.conversation.id, c.members[0].id)
        .await
        .unwrap();
    assert!(
        f.store
            .send(f.human, c.conversation.id, &f.msg("Unaddressed"))
            .await
            .is_err()
    );
    msg.client_message_id = Uuid::new_v4();
    f.store
        .send(f.human, c.conversation.id, &msg)
        .await
        .unwrap();
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn current_grant_project_and_effect_budget_are_enforced() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let c = f.conversation().await;
    f.store
        .send(f.human, c.conversation.id, &f.msg("Execute"))
        .await
        .unwrap();
    let t = pending_dispatches(&f.db.pool, f.store.home_instance_id)
        .await
        .unwrap()
        .remove(0);
    let task = f.redeem(&t).await;
    let resource: ResourceScope = serde_json::from_value(t.request["resource"].clone()).unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    admission(&mut tx).await.unwrap();
    assert!(
        authorize_task_origin(&mut tx, task, "inspect", &resource, Usage::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    admission(&mut tx).await.unwrap();
    authorize_task_origin(
        &mut tx,
        task,
        "conversation_turn",
        &resource,
        Usage {
            micro_usd: 900,
            calls: 1,
            turns: 1,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    admission(&mut tx).await.unwrap();
    assert!(
        authorize_task_origin(
            &mut tx,
            task,
            "conversation_turn",
            &resource,
            Usage {
                micro_usd: 101,
                calls: 1,
                turns: 1
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
        .bind(f.human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_principals SET global_permissions_json='[]'::jsonb WHERE id=$1")
        .bind(f.human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        authorize_task_origin(
            &mut tx,
            task,
            "conversation_turn",
            &resource,
            Usage::default()
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    f.store.revoke_grant(f.grant.id).await.unwrap();
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inbox_task_commands WHERE task_id=$1 AND kind='cancel'",
    )
    .bind(task)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(pending, 1);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn replies_admit_actions_once_and_clarification_keeps_root_budget() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let c = f.conversation().await;
    let msg = f.msg("Inspect project");
    f.store
        .send(f.human, c.conversation.id, &msg)
        .await
        .unwrap();
    let ticket = pending_dispatches(&f.db.pool, f.store.home_instance_id)
        .await
        .unwrap()
        .remove(0);
    let task = f.redeem(&ticket).await;
    let evidence = json!({"conversation_reply":{"version":1,"reply":"Inspecting under your grant","actions":[{"key":"inspect","payload":{"kind":"inspect"}}],"peer_requests":[]}});
    record_task_result(&f.db.pool, task, "completed", None, Some(&evidence))
        .await
        .unwrap();
    record_task_result(&f.db.pool, task, "completed", None, Some(&evidence))
        .await
        .unwrap();
    assert_eq!(
        pending_dispatches(&f.db.pool, f.store.home_instance_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        f.store
            .detail(f.human, c.conversation.id)
            .await
            .unwrap()
            .messages
            .len(),
        2
    );
    let action = pending_dispatches(&f.db.pool, f.store.home_instance_id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(action.request["capability"], "inspect");
    let action_task = f.redeem(&action).await;
    record_task_result(
        &f.db.pool,
        action_task,
        "input_required",
        Some("Which scope?"),
        None,
    )
    .await
    .unwrap();
    let answer = Uuid::new_v4();
    f.store
        .respond(
            f.human,
            c.conversation.id,
            action_task,
            answer,
            "Use the enrolled project".into(),
        )
        .await
        .unwrap();
    f.store
        .respond(
            f.human,
            c.conversation.id,
            action_task,
            answer,
            "Use the enrolled project".into(),
        )
        .await
        .unwrap();
    let rows = sqlx::query("SELECT id,turns FROM inbox_roots")
        .fetch_all(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].try_get::<Uuid, _>("id").unwrap(),
        msg.client_message_id
    );
    assert_eq!(rows[0].try_get::<i32, _>("turns").unwrap(), 2);
    assert!(
        f.store
            .respond(
                f.human,
                c.conversation.id,
                action_task,
                Uuid::new_v4(),
                "Another answer".into()
            )
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn eight_participant_turns_are_shared_across_response_tree() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let c = f.conversation().await;
    let mut second = f.grant.clone();
    second.id = Uuid::new_v4();
    second.contact.id = Uuid::new_v4();
    f.store.install_grant(&second).await.unwrap();
    let add = AddMember {
        client_request_id: Uuid::new_v4(),
        specialist_id: second.contact.id,
        summary: String::new(),
        selected_message_ids: vec![],
        confirmed: true,
    };
    f.store
        .add_member(f.human, c.conversation.id, &add)
        .await
        .unwrap();
    f.store
        .send(f.human, c.conversation.id, &f.msg("Coordinate"))
        .await
        .unwrap();
    for i in 0..8 {
        let ticket = pending_dispatches(&f.db.pool, f.store.home_instance_id)
            .await
            .unwrap()
            .remove(0);
        let task = f.redeem(&ticket).await;
        let peer = if i % 2 == 0 {
            add.client_request_id
        } else {
            c.members[0].id
        };
        record_task_result(&f.db.pool,task,"completed",None,Some(&json!({"conversation_reply":{"version":1,"reply":"Consulting participant","actions":[],"peer_requests":[{"specialist_id":peer,"message":"Review the current summary"}]}}))).await.unwrap();
    }
    assert!(
        pending_dispatches(&f.db.pool, f.store.home_instance_id)
            .await
            .unwrap()
            .is_empty()
    );
    let turns: i32 = sqlx::query_scalar("SELECT turns FROM inbox_roots")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(turns, 8);
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
