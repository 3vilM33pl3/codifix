// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dedicated fresh gateway schema preparation, never area import or enrollment.
use super::{SchemaPlan, describe, migrations, prepare};
use crate::{Connection, Failure, Input, Result, TARGET_LIMIT, hash, json};
use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, PgConnection};
use std::{path::Path, time::Duration};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GatewayTarget {
    format_version: u32,
    space_id: Uuid,
    connection: Connection,
    database: String,
    owner: String,
    password: Option<String>,
}

#[derive(Serialize)]
pub struct GatewaySchemaReport {
    pub status: &'static str,
    pub review_sha256: String,
    pub target_sha256: String,
    pub space_id: Uuid,
    pub connection: Connection,
    pub database: String,
    pub owner: String,
    pub schema: SchemaPlan,
    pub creates_roles_or_grants: bool,
    pub enrolls_gateway: bool,
}

fn load(path: &Path) -> Result<(Input, GatewayTarget, GatewaySchemaReport)> {
    let mut input = Input::open(path, TARGET_LIMIT)?;
    let (target, target_sha256): (GatewayTarget, _) = json(&mut input, TARGET_LIMIT)?;
    if target.format_version != 1 || target.space_id.is_nil() {
        return Err(Failure::Target);
    }
    // Validate all target details without opening a connection or consulting
    // ambient libpq environment/.pgpass. No arbitrary remote hostname or URL.
    target.connection.options(
        &target.database,
        &target.owner,
        target.password.as_deref(),
        "memory-gateway-schema",
    )?;
    let schema = super::plan()?;
    let review_sha256 = hash(
        format!(
            "memory-gateway-fresh-schema-v1\n{target_sha256}\n{}\n",
            schema.schema_sha256
        )
        .as_bytes(),
    );
    let report = GatewaySchemaReport {
        status: "gateway_schema_review_only_no_database_connection",
        review_sha256,
        target_sha256,
        space_id: target.space_id,
        connection: target.connection.clone(),
        database: target.database.clone(),
        owner: target.owner.clone(),
        schema,
        creates_roles_or_grants: false,
        enrolls_gateway: false,
    };
    input.unchanged()?;
    Ok((input, target, report))
}

/// Offline exact target/SQL review. Password bytes and the input-file path are
/// not output; an explicitly reviewed Unix socket directory identifies the DB.
pub fn preview(path: &Path) -> Result<GatewaySchemaReport> {
    Ok(load(path)?.2)
}

/// Only a fresh empty database owned by the exact configured role is eligible.
/// Writer fencing is an explicit operator attestation plus an instantaneous
/// other-client check, not a mechanism for fencing subsequently arriving clients.
pub async fn initialize(
    path: &Path,
    review_sha256: &str,
    writers_stopped: bool,
    fresh_database: bool,
) -> Result<GatewaySchemaReport> {
    if !writers_stopped || !fresh_database {
        return Err(Failure::Target);
    }
    let (input, target, mut report) = load(path)?;
    if report.review_sha256 != review_sha256 {
        return Err(Failure::Binding);
    }
    let migrator = migrations();
    if describe(&migrator)?.schema_sha256 != report.schema.schema_sha256 {
        return Err(Failure::Binding);
    }
    let options = target.connection.options(
        &target.database,
        &target.owner,
        target.password.as_deref(),
        "memory-gateway-schema",
    )?;
    let mut connection = tokio::time::timeout(
        Duration::from_secs(10),
        PgConnection::connect_with(&options),
    )
    .await
    .map_err(|_| Failure::Connection)?
    .map_err(|_| Failure::Connection)?;
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let identity: (String, String, String) = sqlx::query_as(
            "SELECT pg_catalog.current_database(),current_user::pg_catalog.text, \
             pg_catalog.pg_get_userbyid(datdba) FROM pg_catalog.pg_database \
             WHERE datname=pg_catalog.current_database()",
        )
        .persistent(false)
        .fetch_one(&mut connection)
        .await
        .map_err(|_| Failure::Connection)?;
        if identity != (target.database.clone(), target.owner.clone(), target.owner) {
            return Err(Failure::GatewayTarget);
        }
        input.unchanged()?;
        prepare(&mut connection, &migrator).await
    })
    .await
    .map_err(|_| Failure::Schema);
    let _ = tokio::time::timeout(Duration::from_secs(5), connection.close()).await;
    result??;
    report.status = "gateway_schema_initialized_only_not_enrolled_or_activated";
    report.schema.status = "fresh_schema_initialized_only_not_imported_or_activated";
    Ok(report)
}
