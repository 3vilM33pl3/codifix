#![allow(clippy::unwrap_used)]
//! Static response checks plus opt-in real browser coverage of private controls.
//! No database, live token, external service or production state is involved.
#[path = "../src/operator.rs"]
mod operator;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

#[tokio::test]
async fn public_shell_has_no_credentials_and_enforces_hash_bound_same_origin_policy() {
    let response = Router::new()
        .route("/operator", get(operator::shell))
        .oneshot(
            Request::builder()
                .uri("/operator")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers["content-type"], "text/html; charset=utf-8");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["referrer-policy"], "no-referrer");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert!(!headers.contains_key("set-cookie"));
    let policy = headers["content-security-policy"].to_str().unwrap();
    for restriction in [
        "default-src 'none'",
        "connect-src 'self'",
        "frame-ancestors 'none'",
        "form-action 'none'",
        "base-uri 'none'",
        "object-src 'none'",
    ] {
        assert!(policy.contains(restriction));
    }
    for bypass in ["unsafe-inline", "unsafe-eval", "https:", "http:", "*"] {
        assert!(!policy.contains(bypass));
    }
    let html = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    // Verify the hashes against the actual served bytes, so an edited script or
    // style cannot silently ship with a policy that blocks the operator flow.
    for (tag, directive) in [("script", "script-src"), ("style", "style-src")] {
        assert_eq!(html.matches(&format!("<{tag}>")).count(), 1);
        let source = html
            .split_once(&format!("<{tag}>"))
            .unwrap()
            .1
            .split_once(&format!("</{tag}>"))
            .unwrap()
            .0;
        let hash = STANDARD.encode(Sha256::digest(source.as_bytes()));
        assert!(policy.contains(&format!("{directive} 'sha256-{hash}'")));
    }
    assert!(!html.contains("<script src="));
    assert!(html.contains("Disconnected. No private data loaded."));
    assert!(html.contains("type=\"password\" autocomplete=\"off\""));
}

/// Run with CODIFIX_OPERATOR_PLAYWRIGHT_MODULE set to an installed Playwright
/// module entry path; optionally set CODIFIX_OPERATOR_CHROME. This executes the
/// actual HTML in Chrome, including CSP, injection attempts and confirmation.
#[tokio::test]
#[ignore = "requires local Node, Chrome and CODIFIX_OPERATOR_PLAYWRIGHT_MODULE"]
async fn browser_private_session_renders_safely_and_controls_only_authorized_same_origin_paths() {
    use axum::{
        Json,
        extract::{Path, State},
        http::HeaderMap,
        routing::post,
    };
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    const TOKEN: &str = "synthetic-operator-browser-token-no-live-authority";
    const TASK: &str = "11111111-1111-4111-8111-111111111111";
    const MANDATE: &str = "22222222-2222-4222-8222-222222222222";
    #[derive(Default)]
    struct Trace {
        actions: Vec<String>,
        lists: usize,
        canceled: bool,
    }
    fn allowed(headers: &HeaderMap) -> bool {
        headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some("Bearer synthetic-operator-browser-token-no-live-authority")
    }
    async fn list(
        State(trace): State<Arc<Mutex<Trace>>>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<Value>) {
        if !allowed(&headers) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"unauthorized"})),
            );
        }
        let mut trace = trace.lock().unwrap();
        trace.lists += 1;
        let first = json!({"contract":{"id":TASK,"mandate_id":MANDATE,"specialist_id":"synthetic-operations","objective":"Review <img src=x onerror=\"window.__operatorXss=true\">","status":if trace.canceled {"canceled"}else{"input_required"},"clarification":{"question":"<script>window.__operatorXss=true</script> Which scope?","answer":null},"outcome":null},"request":{"deadline":"2099-01-01T00:00:00Z","budget":{"micro_usd":100,"calls":5,"turns":10},"payload":{"private":"hidden-synthetic-payload-marker"}},"used_micro_usd":50,"used_calls":1,"used_turns":2});
        let mut finished = first.clone();
        finished["contract"]["id"] = json!("33333333-3333-4333-8333-333333333333");
        finished["contract"]["objective"] = json!("Completed synthetic task");
        finished["contract"]["status"] = json!("completed");
        finished["contract"]["clarification"] = Value::Null;
        finished["contract"]["outcome"] = json!({"summary":"Finished <svg onload=\"window.__operatorXss=true\">","artifact_refs":["private:fixture-artifact"],"evidence_refs":["private:fixture-evidence"],"acceptance":[]});
        (StatusCode::OK, Json(json!([first, finished])))
    }
    async fn cancel(
        State(trace): State<Arc<Mutex<Trace>>>,
        headers: HeaderMap,
        Path(id): Path<String>,
    ) -> (StatusCode, Json<Value>) {
        if !allowed(&headers) {
            return (StatusCode::UNAUTHORIZED, Json(json!({})));
        }
        if id != TASK {
            return (StatusCode::NOT_FOUND, Json(json!({})));
        }
        let mut trace = trace.lock().unwrap();
        trace.actions.push("task:cancel".into());
        trace.canceled = true;
        (StatusCode::OK, Json(json!({"accepted":true})))
    }
    async fn mandate(
        State(trace): State<Arc<Mutex<Trace>>>,
        headers: HeaderMap,
        Path((id, action)): Path<(String, String)>,
    ) -> (StatusCode, Json<Value>) {
        if !allowed(&headers) {
            return (StatusCode::UNAUTHORIZED, Json(json!({})));
        }
        if id != MANDATE || !matches!(action.as_str(), "pause" | "resume" | "revoke") {
            return (StatusCode::NOT_FOUND, Json(json!({})));
        }
        trace
            .lock()
            .unwrap()
            .actions
            .push(format!("mandate:{action}"));
        (StatusCode::OK, Json(json!({"accepted":true})))
    }
    let trace = Arc::new(Mutex::new(Trace::default()));
    let app = Router::new()
        .route("/operator", get(operator::shell))
        .route("/v1/tasks", get(list))
        .route("/v1/tasks/{id}/cancel", post(cancel))
        .route("/v1/mandates/{id}/{action}", post(mandate))
        .with_state(trace.clone());
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(tcp, app).await.unwrap();
    });
    let module = std::env::var("CODIFIX_OPERATOR_PLAYWRIGHT_MODULE")
        .expect("set the installed Playwright module entry path");
    let mut command = tokio::process::Command::new("node");
    command
        .args([
            "--input-type=module",
            "-e",
            BROWSER,
            &origin,
            &module,
            TOKEN,
        ])
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), command.output())
        .await
        .expect("bounded browser fixture")
        .unwrap();
    server.abort();
    assert!(
        output.status.success(),
        "browser stdout:\n{}\nbrowser stderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let trace = trace.lock().unwrap();
    assert!(trace.lists >= 5);
    assert_eq!(
        trace.actions,
        vec![
            "mandate:pause",
            "mandate:resume",
            "mandate:revoke",
            "task:cancel"
        ]
    );
}
const BROWSER: &str = r#"
import assert from 'node:assert/strict';
const [origin,module,token]=process.argv.slice(1);
const {chromium}=await import(module);
const browser=await chromium.launch({headless:true,executablePath:process.env.CODIFIX_OPERATOR_CHROME||'/usr/bin/google-chrome',args:['--no-sandbox']});
try {
  const context=await browser.newContext();
  const page=await context.newPage();
  const requests=[];const errors=[];
  page.on('request',request=>requests.push(request.url()));
  page.on('pageerror',error=>errors.push(error.message));
  await page.goto(origin+'/operator');
  await page.waitForTimeout(150);
  assert.equal(requests.filter(url=>url.includes('/v1/')).length,0);
  assert.equal(await page.locator('article').count(),0);
  assert.equal(await page.locator('#refresh').isDisabled(),true);
  await page.locator('#token').fill(token);
  await page.getByRole('button',{name:'Connect',exact:true}).click();
  await page.waitForFunction(()=>document.querySelectorAll('article').length===1);
  assert.equal(await page.locator('#token').inputValue(),'');
  assert.equal(await page.locator('article img, article svg, article script').count(),0);
  assert.equal(await page.evaluate(()=>window.__operatorXss),undefined);
  assert.ok((await page.locator('article').textContent()).includes('$0.000050 of $0.000100'));
  assert.ok((await page.locator('article').textContent()).includes('2099-01-01T00:00:00Z'));
  assert.ok((await page.locator('article').textContent()).includes('Which scope?'));
  assert.equal((await page.locator('article details').getAttribute('open')),null);
  assert.equal(await page.getByText('hidden-synthetic-payload-marker',{exact:false}).count(),0);
  await page.getByText('Show private request payload',{exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('article details pre')?.textContent.includes('hidden-synthetic-payload-marker'));
  await page.getByText('Show private request payload',{exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('article details pre'));
  await page.locator('#filter').selectOption('all');
  assert.equal(await page.locator('article').count(),2);
  assert.ok((await page.locator('article').nth(1).textContent()).includes('private:fixture-evidence'));
  assert.equal(await page.locator('article img, article svg, article script').count(),0);
  if(process.env.CODIFIX_OPERATOR_SCREENSHOT) await page.screenshot({path:process.env.CODIFIX_OPERATOR_SCREENSHOT,fullPage:true});
  const card=page.locator('article').first();
  for(const [label,accepted] of [['Pause mandate','Pause mandate accepted.'],['Resume mandate','Resume mandate accepted.']]) {
    await card.getByRole('button',{name:label,exact:true}).click();
    await page.waitForFunction(text=>document.getElementById('notice').textContent.includes(text),accepted);
  }
  page.once('dialog',dialog=>dialog.dismiss());
  await card.getByRole('button',{name:'Revoke mandate',exact:true}).click();
  assert.equal(requests.filter(url=>url.endsWith('/revoke')).length,0);
  page.once('dialog',dialog=>dialog.accept());
  await card.getByRole('button',{name:'Revoke mandate',exact:true}).click();
  await page.waitForFunction(()=>document.getElementById('notice').textContent.includes('Revoke mandate accepted.'));
  await card.getByRole('button',{name:'Cancel task',exact:true}).click();
  await page.waitForFunction(()=>document.getElementById('notice').textContent.includes('Cancel task accepted.'));
  assert.equal(requests.filter(url=>url.endsWith('/revoke')).length,1);
  assert.ok(requests.every(url=>new URL(url).origin===origin));
  assert.ok(requests.every(url=>!url.includes(token)));
  assert.equal(await page.evaluate(()=>localStorage.length+sessionStorage.length),0);
  assert.equal((await context.cookies()).length,0);
  assert.ok(!(await page.locator('body').textContent()).includes(token));
  await page.getByRole('button',{name:'Disconnect and clear',exact:true}).click();
  assert.equal(await page.locator('article').count(),0);
  assert.equal(await page.locator('#token').inputValue(),'');
  const before=requests.filter(url=>url.includes('/v1/')).length;
  await page.reload();
  await page.waitForTimeout(150);
  assert.equal(requests.filter(url=>url.includes('/v1/')).length,before);
  assert.equal(await page.locator('article').count(),0);
  await page.locator('#token').fill('synthetic-wrong-token-with-no-authority');
  await page.getByRole('button',{name:'Connect',exact:true}).click();
  await page.waitForFunction(()=>document.getElementById('notice').textContent.includes('Access was denied.'));
  assert.equal(await page.locator('article').count(),0);
  assert.equal(await page.locator('#disconnect').isDisabled(),true);
  assert.deepEqual(errors,[]);
  console.log('Operator browser flow passed: private memory-only access, safe rendering, controls and revoke confirmation.');
} finally {await browser.close();}
"#;
