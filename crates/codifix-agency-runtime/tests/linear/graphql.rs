use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(super) struct Graphql {
    pub(super) objects: BTreeMap<String, Value>,
    pub(super) reads: usize,
    pub(super) writes: Vec<Value>,
    pub(super) fail_next_write: bool,
}
pub(super) async fn graphql(
    State(state): State<Arc<Mutex<Graphql>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut state = state.lock().unwrap();
    let query = body["query"].as_str().unwrap();
    if query.starts_with("query") {
        state.reads += 1;
        if query.contains("AgencyScopedIssue") || query.contains("AgencyFindIssue") {
            let filter = &body["variables"]["filter"];
            let rows: Vec<_> = state
                .objects
                .values()
                .filter(|object| {
                    object["team"]["id"] == filter["team"]["id"]["eq"]
                        && object["project"]["id"] == filter["project"]["id"]["eq"]
                })
                .filter(|object| {
                    if query.contains("AgencyScopedIssue") {
                        object["id"] == filter["id"]["eq"]
                    } else {
                        object["title"].as_str().is_some_and(|v| {
                            v.eq_ignore_ascii_case(
                                filter["title"]["eqIgnoreCase"].as_str().unwrap(),
                            )
                        })
                    }
                })
                .take(2)
                .collect();
            return Json(json!({"data":{"issues":{"nodes":rows}}}));
        }
        let id = body["variables"]["id"].as_str().unwrap();
        return Json(match state.objects.get(id) {
            Some(object) => json!({"data":{"object":object}}),
            None => json!({"errors":[{"extensions":{"code":"ENTITY_NOT_FOUND"}}]}),
        });
    }
    state.writes.push(body.clone());
    if std::mem::take(&mut state.fail_next_write) {
        return Json(json!({"errors":[{"extensions":{"code":"PROVIDER_UNAVAILABLE"}}]}));
    }
    let n = state.writes.len();
    let input = &body["variables"]["input"];
    let create = query.contains("CreateInput");
    let id = if create {
        input["id"].as_str().unwrap()
    } else {
        body["variables"]["id"].as_str().unwrap()
    };
    if create && state.objects.contains_key(id) {
        return Json(json!({"errors":[{"extensions":{"code":"DUPLICATE"}}]}));
    }
    let mut object = state
        .objects
        .get(id)
        .cloned()
        .unwrap_or_else(|| json!({"id":id}));
    object["updatedAt"] = json!(format!("2026-10-31T19:00:{n:02}Z"));
    for (key, value) in input.as_object().unwrap() {
        let relation = match key.as_str() {
            "projectId" => Some("project"),
            "projectMilestoneId" => Some("projectMilestone"),
            "teamId" => Some("team"),
            "leadId" => Some("lead"),
            "assigneeId" => Some("assignee"),
            "stateId" => Some("state"),
            "issueId" => Some("issue"),
            "relatedIssueId" => Some("relatedIssue"),
            _ => None,
        };
        if let Some(name) = relation {
            object[name] = if value.is_null() {
                Value::Null
            } else {
                json!({"id":value})
            };
        } else if key == "teamIds" {
            object["teams"] = json!({"nodes":value.as_array().unwrap().iter().map(|id|json!({"id":id})).collect::<Vec<_>>(),"pageInfo":{"hasNextPage":false}});
        } else {
            object[key] = value.clone();
        }
    }
    if query.contains("issueCreate(") {
        let fields = object.as_object_mut().unwrap();
        fields.entry("projectMilestone").or_insert(Value::Null);
        fields.entry("assignee").or_insert(Value::Null);
        fields.insert(
            "url".into(),
            json!(format!("https://linear.app/fixture/issue/{id}")),
        );
    }
    state.objects.insert(id.into(), object);
    let field = if query.contains("ProjectMilestone") {
        "projectMilestone"
    } else if query.contains("IssueRelation") {
        "issueRelation"
    } else if query.contains("Project") {
        "project"
    } else {
        "issue"
    };
    Json(json!({"data":{"effect":{"success":true,field:{"id":id}}}}))
}
