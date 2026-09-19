// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real CLI and empty disposable PostgreSQL targets, never existing services.
#![allow(clippy::unwrap_used)]
#[allow(dead_code)]
#[path = "../../mem-area-store/tests/support/mod.rs"]
mod support;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, Connection, PgConnection, PgPool, postgres::PgConnectOptions};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Output},
};
use uuid::Uuid;

struct Fixture {
    admin: PgPool,
    options: PgConnectOptions,
    name: String,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let admin = mem_test_support::migrated_pool().await?;
        let name = format!("memory_schema_{}", Uuid::new_v4().simple());
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
    async fn cleanup(self) {
        sqlx::query(&format!("DROP DATABASE {}", self.name))
            .execute(&self.admin)
            .await
            .unwrap();
    }
}
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_memory-area-import"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("PG") {
            cmd.env_remove(name);
        }
    }
    cmd
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
}
fn request(fixture: &Fixture, dir: &std::path::Path) -> Command {
    let plan = success(command().arg("schema-plan").output().unwrap());
    assert_eq!(plan["migration_head"], 60);
    let descriptor = json!({"format_version":1,"instance_id":Uuid::new_v4(),"area":"trust",
        "connection":{"kind":"loopback","port":fixture.options.get_port()},
        "database":fixture.name,"username":fixture.options.get_username(),
        "password":fixture.options.to_url_lossy().password().map(|p|percent_encoding::percent_decode_str(p).decode_utf8().unwrap().into_owned())});
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = dir.join("target.json");
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut cmd = command();
    cmd.arg("initialize-schema")
        .arg("--target")
        .arg(path)
        .arg("--target-sha256")
        .arg(format!("{:x}", Sha256::digest(bytes)))
        .arg("--schema-sha256")
        .arg(plan["schema_sha256"].as_str().unwrap());
    cmd
}
fn private() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn confirm(cmd: &mut Command) {
    cmd.args(["--execute", "--writers-stopped", "--fresh-database"]);
}

#[tokio::test]
async fn cli_initializes_without_bootstrap_and_refuses_reinitialization() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let directory = private();
    let mut cmd = request(&fixture, directory.path());
    failure(cmd.output().unwrap(), "execution assertions");
    confirm(&mut cmd);
    let report = success(cmd.output().unwrap());
    assert_eq!(
        report["status"],
        "fresh_schema_initialized_only_not_imported_or_activated"
    );
    assert_eq!(report["creates_credentials"], false);
    assert_eq!(report["starts_services"], false);
    let mut connection = fixture.connect().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT max(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        60
    );
    for table in [
        "projects",
        "auth_principals",
        "loop_definitions",
        "background_jobs",
        "supervisor_actions",
        "supervisor_instance_binding",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&mut connection)
                .await
                .unwrap(),
            0,
            "no application bootstrap for {table}"
        );
    }
    connection.close().await.unwrap();
    failure(cmd.output().unwrap(), "empty dedicated database");
    // The exact schema made by the shipped command accepts the existing
    // reviewed importer, including tombstones and provenance, without bootstrap.
    let source = support::database().await.unwrap();
    support::seed(&source.pool).await;
    let mut captured = support::capture(&source.pool).await;
    captured.project_provenance();
    let (partition, _) = captured.partition();
    let target = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(fixture.options.clone())
        .await
        .unwrap();
    let receipt =
        mem_area_store::import_partition(&target, support::TARGET, support::OPERATION, &partition)
            .await
            .unwrap();
    assert!(receipt.row_count > 0);
    assert!(
        mem_area_store::import_partition(&target, support::TARGET, support::OPERATION, &partition)
            .await
            .unwrap()
            .already_applied
    );
    assert_eq!(
        support::rows(&target, "public.memory_entries").await,
        support::rows(&source.pool, "public.memory_entries").await
    );
    target.close().await;
    source.cleanup().await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
async fn cli_refuses_other_clients_and_even_empty_preexisting_objects() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let directory = private();
    let mut cmd = request(&fixture, directory.path());
    confirm(&mut cmd);
    let mut connection = fixture.connect().await;
    failure(cmd.output().unwrap(), "empty dedicated database");
    sqlx::query("CREATE TABLE preserve_me (id integer)")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    failure(cmd.output().unwrap(), "empty dedicated database");
    let mut connection = fixture.connect().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pg_tables WHERE schemaname='public'")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM preserve_me")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        0
    );
    connection.close().await.unwrap();
    fixture.cleanup().await;
}

#[test]
fn schema_plan_is_deterministic_and_bad_execution_never_uses_ambient_database() {
    let first = success(command().arg("schema-plan").output().unwrap());
    let second = success(
        command()
            .arg("schema-plan")
            .env("PGDATABASE", "do-not-connect")
            .output()
            .unwrap(),
    );
    assert_eq!(first, second);
    assert_eq!(first["schema_sha256"].as_str().unwrap().len(), 64);
    failure(
        command()
            .args([
                "initialize-schema",
                "--target",
                "/does-not-exist",
                "--target-sha256",
                &"a".repeat(64),
                "--schema-sha256",
                &"0".repeat(64),
                "--execute",
                "--writers-stopped",
                "--fresh-database",
            ])
            .output()
            .unwrap(),
        "execution assertions",
    );
}
