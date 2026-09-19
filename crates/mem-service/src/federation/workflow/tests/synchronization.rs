// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn retry_reports_verified_retained_cursor_not_sent_intent_count() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.grant(f.writer_id, true).await;
    let initial_cursor = f.runtime.sync.verified_snapshot().await.unwrap().sequence();
    assert_eq!(initial_cursor, 2);

    // Nothing is queued: zero sent intents must not become cursor zero.
    let (status, response) = f
        .request(
            &f.writer,
            reqwest::Method::POST,
            "/sync",
            json!({"operation":"retry"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["retained_through"], initial_cursor);

    let canonical = f.source().await;
    let review = f.review_request(canonical).await;
    let (status, response) = f
        .request(&f.writer, reqwest::Method::POST, "/reviews", review)
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{response}");
    let (status, response) = f
        .request(
            &f.writer,
            reqwest::Method::POST,
            "/sync",
            json!({"operation":"retry"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    // Gateway acceptance is not publication or retained replication. The fake
    // publisher has not supplied any receipts for the newly accepted work yet.
    assert_eq!(response["retained_through"], initial_cursor);
    let accepted: i64 = sqlx::query_scalar(
        "SELECT accepted_sequence FROM federation_gateway_spaces WHERE space=$1",
    )
    .bind(f.runtime.space.as_uuid())
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(accepted > i64::from(initial_cursor));

    f.publish_fixture_receipts().await;
    for _ in 0..2 {
        let (status, response) = f
            .request(
                &f.writer,
                reqwest::Method::POST,
                "/sync",
                json!({"operation":"retry"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{response}");
        let known = f.runtime.sync.verified_snapshot().await.unwrap();
        assert_eq!(i64::from(known.sequence()), accepted);
        assert_eq!(response["retained_through"], known.sequence());
        let (status, detail) = f
            .request(&f.writer, reqwest::Method::GET, "", Value::Null)
            .await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        assert_eq!(detail["retained_through"], response["retained_through"]);
    }
    // Reading the corrected response and repeating retry created no new work.
    let records: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_accepted WHERE space=$1")
            .bind(f.runtime.space.as_uuid())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(records, accepted);
}
