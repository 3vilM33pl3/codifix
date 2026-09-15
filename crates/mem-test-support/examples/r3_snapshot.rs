// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted-host JSONL fixture owner. Disposable PostgreSQL only; not a service
//! API, production backup tool or evidence of model quality. Keep stdout private:
//! fork replies contain temporary database logins and project-scoped API tokens.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use mem_test_support::{FrozenDatabase, IsolatedDatabase, fingerprint::benchmark_fingerprint};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::ConnectOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Write},
};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    PlanChunks {
        documents: Vec<Document>,
    },
    Prepare {
        semantic_time: DateTime<Utc>,
        documents: Vec<Document>,
        #[serde(default)]
        embedding: Option<Embedding>,
    },
    Fork,
    Fingerprint {
        id: Uuid,
    },
    Release {
        id: Uuid,
    },
    Shutdown,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    project: String,
    source: String,
    text: String,
    #[serde(default)]
    vectors: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Embedding {
    model: String,
    dimensions: usize,
    // Only a trusted-host loopback proxy is usable by this disposable harness.
    // This is transport metadata; the snapshot owner makes no provider calls.
    base_url: String,
}

impl Embedding {
    fn validate(&self) -> Result<()> {
        let port = self
            .base_url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|value| value.strip_suffix("/v1"))
            .and_then(|value| value.parse::<u16>().ok());
        ensure!(
            port.is_some_and(|port| port > 0)
                && (1..=8192).contains(&self.dimensions)
                && !self.model.is_empty()
                && self.model.len() <= 128
                && self
                    .model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "invalid disposable embedding profile"
        );
        Ok(())
    }
    fn space(&self) -> String {
        format!("openai_compatible|{}|{}", self.base_url, self.model)
    }
}

#[derive(Serialize)]
struct Binding {
    project: String,
    source: String,
    memory_id: Uuid,
    canonical_id: Uuid,
    version_no: u32,
    source_sha256: String,
}

#[derive(Default)]
struct Owner {
    phase: &'static str,
    preparation: Option<IsolatedDatabase>,
    frozen: Option<FrozenDatabase>,
    readers: BTreeMap<Uuid, IsolatedDatabase>,
    tokens: BTreeMap<String, String>,
    bindings: Vec<Binding>,
    fingerprint: Option<String>,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn validate_documents(documents: &[Document]) -> Result<()> {
    ensure!(
        !documents.is_empty() && documents.len() <= 1024,
        "invalid document count"
    );
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    for d in documents {
        ensure!(
            identifier(&d.project)
                && !d.source.is_empty()
                && d.source.len() <= 240
                && d.source.split('/').all(|s| !s.is_empty()
                    && s != "."
                    && s != ".."
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
                && !d.text.trim().is_empty()
                && d.text.len() <= 65536
                && seen.insert((&d.project, &d.source)),
            "invalid fixture document"
        );
        bytes = bytes.checked_add(d.text.len()).context("input overflow")?;
    }
    ensure!(bytes <= 8 * 1024 * 1024, "fixture too large");
    Ok(())
}

impl Owner {
    async fn prepare(
        &mut self,
        semantic_time: DateTime<Utc>,
        documents: Vec<Document>,
        embedding: Option<Embedding>,
    ) -> Result<Value> {
        self.phase = "prepare-input";
        ensure!(
            self.preparation.is_none() && self.frozen.is_none(),
            "snapshot already prepared"
        );
        validate_documents(&documents)?;
        if let Some(profile) = &embedding {
            profile.validate()?;
        }
        for d in &documents {
            if let Some(profile) = &embedding {
                let chunks = mem_search::split_search_chunks(&d.source, &d.text);
                ensure!(
                    d.vectors.len() == chunks.len()
                        && d.vectors
                            .iter()
                            .all(|vector| vector.len() == profile.dimensions
                                && vector.iter().all(|v| v.is_finite())
                                && vector.iter().any(|v| *v != 0.0)),
                    "invalid prepared fixture vectors"
                );
            } else {
                ensure!(d.vectors.is_empty(), "vectors require an embedding profile");
            }
        }
        self.phase = "prepare-connect";
        self.preparation = Some(
            IsolatedDatabase::new()
                .await
                .context("disposable database required")?,
        );
        let pool = &self
            .preparation
            .as_ref()
            .context("missing preparation")?
            .pool;
        self.phase = "prepare-seed";
        let mut tx = pool.begin().await?;
        let mut projects = BTreeMap::new();
        for d in documents {
            let project = if let Some(id) = projects.get(&d.project) {
                *id
            } else {
                let id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/synthetic-r3')",
                )
                .bind(id)
                .bind(&d.project)
                .execute(&mut *tx)
                .await?;
                let principal = Uuid::new_v4();
                let token = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
                sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'service_token','synthetic-evaluator')")
                    .bind(principal).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')")
                    .bind(Uuid::new_v4()).bind(principal).bind(id).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,expires_at)
                    VALUES($1,$2,'synthetic-r3',$3,'mlt_synthetic',now()+interval '1 day')")
                    .bind(Uuid::new_v4()).bind(principal).bind(Sha256::digest(token.as_bytes()).to_vec())
                    .execute(&mut *tx).await?;
                self.tokens.insert(d.project.clone(), token);
                projects.insert(d.project.clone(), id);
                id
            };
            let memory = Uuid::new_v4();
            sqlx::query("INSERT INTO memory_entries
                (id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document)
                VALUES($1,$2,$1,1,false,$3,$4,'reference','project',3,0.9,$5,$5,to_tsvector('english',$3))")
                .bind(memory).bind(project).bind(&d.text).bind(&d.source).bind(semantic_time)
                .execute(&mut *tx).await?;
            if let Some(profile) = &embedding {
                let chunks = mem_search::split_search_chunks(&d.source, &d.text);
                for (index, (text, vector)) in chunks.iter().zip(&d.vectors).enumerate() {
                    let id = Uuid::new_v5(
                        &memory,
                        format!("search-chunk-v1:{index}:{text}").as_bytes(),
                    );
                    sqlx::query(
                        "INSERT INTO memory_chunks(id,memory_entry_id,chunk_text,search_text,tsv)
                        VALUES($1,$2,$3,$4,to_tsvector('english',$4))",
                    )
                    .bind(id)
                    .bind(memory)
                    .bind(text)
                    .bind(format!("{}\n{text}", d.source))
                    .execute(&mut *tx)
                    .await?;
                    let vector = format!(
                        "[{}]",
                        vector
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    sqlx::query("INSERT INTO memory_chunk_embeddings
                        (chunk_id,embedding_space,embedding,embedding_provider,embedding_base_url,embedding_model,embedding_dimension,embedding_updated_at)
                        VALUES($1,$2,$3::vector,'openai_compatible',$4,$5,$6,$7)")
                        .bind(id).bind(profile.space()).bind(vector).bind(&profile.base_url)
                        .bind(&profile.model).bind(i32::try_from(profile.dimensions)?).bind(semantic_time)
                        .execute(&mut *tx).await?;
                }
            }
            self.bindings.push(Binding {
                project: d.project,
                source: d.source,
                memory_id: memory,
                canonical_id: memory,
                version_no: 1,
                source_sha256: format!("{:x}", Sha256::digest(d.text.as_bytes())),
            });
        }
        tx.commit().await?;
        self.phase = "prepare-fingerprint";
        self.fingerprint = Some(benchmark_fingerprint(pool).await?.sha256);
        self.phase = "prepare-freeze";
        self.frozen = Some(
            self.preparation
                .take()
                .context("missing preparation")?
                .freeze()
                .await?,
        );
        Ok(
            json!({"version":1,"prepared":true,"semantic_time":semantic_time,
            "snapshot_sha256":self.fingerprint,"bindings":self.bindings,
            "preparation_microusd":if embedding.is_some() { None } else { Some(0u64) },
            "embedding_preparation":if embedding.is_some() { "supplied vectors; provider accounting is external" } else { "none; offline lexical fixture only" },
            "official_evidence":false}),
        )
    }

    async fn command(&mut self, request: Request) -> Result<Option<Value>> {
        match request {
            Request::PlanChunks { documents } => {
                self.phase = "plan-chunks";
                ensure!(
                    self.preparation.is_none() && self.frozen.is_none(),
                    "plan before preparation"
                );
                validate_documents(&documents)?;
                ensure!(
                    documents.iter().all(|d| d.vectors.is_empty()),
                    "planning does not accept vectors"
                );
                Ok(Some(
                    json!({"version":1,"chunking":"memory-split-search-chunks",
                    "documents":documents.into_iter().map(|d| json!({"project":d.project,
                        "source":d.source,"source_sha256":format!("{:x}",Sha256::digest(d.text.as_bytes())),
                        "chunks":mem_search::split_search_chunks(&d.source,&d.text)})).collect::<Vec<_>>(),
                    "official_evidence":false}),
                ))
            }
            Request::Prepare {
                semantic_time,
                documents,
                embedding,
            } => Ok(Some(
                self.prepare(semantic_time, documents, embedding).await?,
            )),
            Request::Fork => {
                self.phase = "fork";
                ensure!(
                    self.readers.len() < 16,
                    "release an existing reader before forking"
                );
                let reader = self
                    .frozen
                    .as_ref()
                    .context("prepare first")?
                    .fork_read_only()
                    .await?;
                let id = Uuid::new_v4();
                let url = reader.pool.connect_options().to_url_lossy().to_string();
                let actual = benchmark_fingerprint(&reader.pool).await?.sha256;
                ensure!(
                    Some(&actual) == self.fingerprint.as_ref(),
                    "snapshot mismatch"
                );
                self.readers.insert(id, reader);
                Ok(Some(
                    json!({"version":1,"id":id,"database_url":url,"project_tokens":self.tokens,
                    "snapshot_sha256":actual}),
                ))
            }
            Request::Fingerprint { id } => {
                let reader = self.readers.get(&id).context("unknown reader")?;
                let actual = benchmark_fingerprint(&reader.pool).await?.sha256;
                ensure!(
                    Some(&actual) == self.fingerprint.as_ref(),
                    "benchmark changed"
                );
                Ok(Some(json!({"version":1,"id":id,"snapshot_sha256":actual})))
            }
            Request::Release { id } => {
                self.readers
                    .remove(&id)
                    .context("unknown reader")?
                    .cleanup()
                    .await?;
                Ok(Some(json!({"version":1,"released":id})))
            }
            Request::Shutdown => Ok(None),
        }
    }

    async fn cleanup(self) -> Result<()> {
        for (_, reader) in self.readers {
            reader.cleanup().await?;
        }
        if let Some(db) = self.preparation {
            db.cleanup().await?;
        }
        if let Some(db) = self.frozen {
            db.cleanup().await?;
        }
        Ok(())
    }
}

async fn serve(owner: &mut Owner) -> Result<()> {
    ensure!(
        std::env::var("MEMORY_LAYER_TEST_REQUIRE_DB").as_deref() == Ok("1"),
        "explicit disposable database required"
    );
    let input = std::io::stdin();
    let mut input = input.lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut bytes = Vec::new();
        let size =
            std::io::Read::take(&mut input, 16 * 1024 * 1024 + 1).read_until(b'\n', &mut bytes)?;
        if size == 0 {
            return Ok(());
        }
        ensure!(
            size <= 16 * 1024 * 1024 && bytes.last() == Some(&b'\n'),
            "oversized or incomplete request"
        );
        let Some(response) = owner.command(serde_json::from_slice(&bytes)?).await? else {
            return Ok(());
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}

#[tokio::main]
async fn main() {
    let mut owner = Owner::default();
    let result = serve(&mut owner).await;
    let phase = owner.phase;
    let category = result
        .as_ref()
        .err()
        .map(|error| {
            if let Some(error) = error.downcast_ref::<serde_json::Error>() {
                format!("json-{:?}", error.classify())
            } else if let Some(error) = error.downcast_ref::<sqlx::Error>() {
                format!(
                    "database-{}",
                    error
                        .as_database_error()
                        .and_then(|e| e.code())
                        .unwrap_or_default()
                )
            } else {
                "operation".into()
            }
        })
        .unwrap_or_else(|| "cleanup".into());
    let cleanup = owner.cleanup().await;
    if result.is_err() || cleanup.is_err() {
        eprintln!(
            "R3 snapshot operation failed in {phase} ({category}); reconcile the owned disposable PostgreSQL container"
        );
        std::process::exit(1);
    }
}
