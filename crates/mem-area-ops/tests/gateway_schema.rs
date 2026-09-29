// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real gateway-only CLI against exact disposable PostgreSQL databases.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use sqlx::{ConnectOptions, Connection, PgConnection, PgPool, postgres::PgConnectOptions};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use uuid::Uuid;

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_memory-gateway-schema"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("PG") {
            command.env_remove(name);
        }
    }
    command
}
fn private() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}
fn failure(output: Output, message: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains(message));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SENSITIVE_TEST_PASSWORD"));
}
fn execute(path: &Path, review: &Value) -> Command {
    let mut cmd = command();
    cmd.arg("execute")
        .arg(path)
        .arg(review["review_sha256"].as_str().unwrap())
        .args(["--writers-stopped", "--fresh-database"]);
    cmd
}
fn descriptor() -> Value {
    json!({"format_version":1,"space_id":Uuid::new_v4(),
        "connection":{"kind":"loopback","port":1},
        "database":"not_connected","owner":"schema_owner","password":"SENSITIVE_TEST_PASSWORD"})
}

#[test]
fn default_review_is_offline_exact_and_never_discloses_private_values() {
    let dir = private();
    let path = dir.path().join("target.json");
    let mut value = descriptor();
    write(&path, &value);
    let review = success(command().arg(&path).output().unwrap());
    assert_eq!(
        review,
        success(command().arg("review").arg(&path).output().unwrap())
    );
    assert_eq!(
        review["status"],
        "gateway_schema_review_only_no_database_connection"
    );
    assert_eq!(review["schema"]["migration_head"], 61);
    assert_eq!(review["database"], "not_connected");
    assert_eq!(review["owner"], "schema_owner");
    assert_eq!(review["enrolls_gateway"], false);
    assert_eq!(review["creates_roles_or_grants"], false);
    let output = review.to_string();
    assert!(!output.contains("SENSITIVE_TEST_PASSWORD"));
    assert!(!output.contains(path.to_str().unwrap()));
    assert!(review.get("area").is_none());
    assert!(review.get("instance_id").is_none());
    assert_eq!(review["review_sha256"].as_str().unwrap().len(), 64);
    // A password or target/space change invalidates the exact prior consent,
    // before any attempted connection to this deliberately unavailable port.
    value["password"] = json!("changed-private-password");
    write(&path, &value);
    failure(
        execute(&path, &review).output().unwrap(),
        "digest does not match",
    );
    assert_ne!(
        review["review_sha256"],
        success(command().arg(&path).output().unwrap())["review_sha256"]
    );
    value["connection"] = json!({"kind":"unix","directory":"/run/postgresql","port":5432});
    write(&path, &value);
    assert_eq!(
        success(command().arg(&path).output().unwrap())["connection"]["kind"],
        "unix"
    );
}

#[test]
fn invalid_target_environment_permissions_and_confirmation_fail_closed() {
    let dir = private();
    let path = dir.path().join("target.json");
    let mut value = descriptor();
    write(&path, &value);
    let review = success(command().arg(&path).output().unwrap());
    failure(
        command()
            .args(["execute"])
            .arg(&path)
            .arg(review["review_sha256"].as_str().unwrap())
            .output()
            .unwrap(),
        "execution assertions",
    );
    failure(
        command()
            .arg(&path)
            .env("PGHOST", "unreviewed")
            .output()
            .unwrap(),
        "environment overrides",
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    failure(
        command().arg(&path).output().unwrap(),
        "private file boundary",
    );
    value["area"] = json!("general");
    write(&path, &value);
    failure(command().arg(&path).output().unwrap(), "JSON contract");
    value.as_object_mut().unwrap().remove("area");
    value["connection"] =
        json!({"kind":"url","url":"postgresql://SENSITIVE_TEST_PASSWORD@remote/database"});
    write(&path, &value);
    failure(command().arg(&path).output().unwrap(), "JSON contract");
    failure(
        command()
            .args(["execute", "--password=SENSITIVE_TEST_PASSWORD"])
            .output()
            .unwrap(),
        "supplied values suppressed",
    );
}

struct Fixture {
    admin: PgPool,
    options: PgConnectOptions,
    name: String,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let admin = mem_test_support::migrated_pool().await?;
        let name = format!("memory_gateway_schema_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let options = admin.connect_options().as_ref().clone().database(&name);
        Some(Self {
            admin,
            options,
            name,
        })
    }
    async fn connect(&self) -> PgConnection {
        PgConnection::connect_with(&self.options).await.unwrap()
    }
    fn descriptor(&self) -> Value {
        json!({"format_version":1,"space_id":Uuid::new_v4(),
            "connection":{"kind":"loopback","port":self.options.get_port()},
            "database":self.name,"owner":self.options.get_username(),
            "password":self.options.to_url_lossy().password().map(|p|percent_encoding::percent_decode_str(p).decode_utf8().unwrap().into_owned())})
    }
    async fn cleanup(self) {
        sqlx::query(&format!("DROP DATABASE {}", self.name))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}

#[tokio::test]
async fn initializes_exact_gateway_schema_without_app_bootstrap_and_refuses_replay() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let dir = private();
    let path = dir.path().join("target.json");
    write(&path, &fixture.descriptor());
    let review = success(command().arg(&path).output().unwrap());
    let result = success(execute(&path, &review).output().unwrap());
    assert_eq!(
        result["status"],
        "gateway_schema_initialized_only_not_enrolled_or_activated"
    );
    assert_eq!(result["review_sha256"], review["review_sha256"]);
    let mut connection = fixture.connect().await;
    let original = sqlx::migrate!("../../migrations");
    let history: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .unwrap();
    assert_eq!(history.len(), original.iter().count());
    for ((version, checksum, success), migration) in history.iter().zip(original.iter()) {
        assert_eq!(*version, migration.version);
        assert_eq!(checksum, migration.checksum.as_ref());
        assert!(success);
    }
    for table in [
        "projects",
        "auth_principals",
        "background_jobs",
        "supervisor_actions",
        "supervisor_instance_binding",
        "federation_gateway_spaces",
        "federation_accepted",
        "federation_publish_outbox",
        "federation_audit",
        "federation_expertise_mailbox",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "no app bootstrap or gateway enrollment in {table}"
        );
    }
    connection.close().await.unwrap();
    failure(
        execute(&path, &review).output().unwrap(),
        "empty dedicated database",
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn rejects_other_clients_nonempty_objects_and_wrong_database_owner_without_writes() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let dir = private();
    let path = dir.path().join("target.json");
    write(&path, &fixture.descriptor());
    let review = success(command().arg(&path).output().unwrap());
    let connection = fixture.connect().await;
    failure(
        execute(&path, &review).output().unwrap(),
        "empty dedicated database",
    );
    let other_owner = format!("gateway_schema_owner_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE {other_owner}"))
        .execute(&fixture.admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "ALTER DATABASE {} OWNER TO {other_owner}",
        fixture.name
    ))
    .execute(&fixture.admin)
    .await
    .unwrap();
    connection.close().await.unwrap();
    failure(
        execute(&path, &review).output().unwrap(),
        "differs from the reviewed gateway target",
    );
    sqlx::query(&format!(
        "ALTER DATABASE {} OWNER TO {}",
        fixture.name,
        fixture.options.get_username()
    ))
    .execute(&fixture.admin)
    .await
    .unwrap();
    sqlx::query(&format!("DROP ROLE {other_owner}"))
        .execute(&fixture.admin)
        .await
        .unwrap();
    let mut connection = fixture.connect().await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_tables WHERE schemaname='public'")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("CREATE TABLE preserve_me(id integer)")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    failure(
        execute(&path, &review).output().unwrap(),
        "empty dedicated database",
    );
    let mut connection = fixture.connect().await;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname='public'")
            .fetch_all(&mut connection)
            .await
            .unwrap();
    assert_eq!(tables, ["preserve_me"]);
    connection.close().await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
async fn refuses_standalone_namespace_and_replication_objects_not_only_tables() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let dir = private();
    let path = dir.path().join("target.json");
    write(&path, &fixture.descriptor());
    let review = success(command().arg(&path).output().unwrap());
    for (create, remove) in [
        (
            "CREATE COLLATION public.unreviewed (provider=libc,locale='C')",
            "DROP COLLATION public.unreviewed",
        ),
        (
            "CREATE TEXT SEARCH DICTIONARY public.unreviewed (TEMPLATE=pg_catalog.simple)",
            "DROP TEXT SEARCH DICTIONARY public.unreviewed",
        ),
        (
            "CREATE OPERATOR public.=== (LEFTARG=integer,RIGHTARG=integer,FUNCTION=pg_catalog.int4eq)",
            "DROP OPERATOR public.=== (integer,integer)",
        ),
        (
            "CREATE PUBLICATION unreviewed",
            "DROP PUBLICATION unreviewed",
        ),
        (
            "CREATE FOREIGN DATA WRAPPER unreviewed NO HANDLER NO VALIDATOR",
            "DROP FOREIGN DATA WRAPPER unreviewed",
        ),
    ] {
        let mut connection = fixture.connect().await;
        sqlx::query(create).execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
        failure(
            execute(&path, &review).output().unwrap(),
            "empty dedicated database",
        );
        let mut connection = fixture.connect().await;
        let migrated: bool =
            sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert!(!migrated, "no history writes in an occupied target");
        sqlx::query(remove).execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
    }
    // Exact removal of only these fixture-owned objects leaves a genuinely
    // fresh target; the broader catalogue guard must not reject that baseline.
    success(execute(&path, &review).output().unwrap());
    fixture.cleanup().await;
}
