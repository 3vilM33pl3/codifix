# syntax=docker/dockerfile:1
# SPDX-License-Identifier: AGPL-3.0-or-later
# Disposable receiver-compatibility fixture only. It is not a CA install image.
# PostgreSQL minor ABI compatibility lets the official pgvector 0.8.6 extension
# built for PostgreSQL 15 load into the exact PostgreSQL 15.18 server observed
# on the CA. The test suite proves that load before any restore is attempted.
ARG POSTGRES_IMAGE=postgres@sha256:752791ad6ccfc6fbfcf4d9b43413c8642901f1c2008ebdc1423ea32fd66fd4f1
ARG PGVECTOR_IMAGE=pgvector/pgvector@sha256:a6579dded3ff4ed3d9565ae54ab253e2cb7ae24c5d1eb3bec8a89e73f038b68a

FROM ${PGVECTOR_IMAGE} AS pgvector
FROM ${POSTGRES_IMAGE}

COPY --from=pgvector /usr/lib/postgresql/15/lib/vector.so /usr/lib/postgresql/15/lib/vector.so
COPY --from=pgvector /usr/share/postgresql/15/extension/vector* /usr/share/postgresql/15/extension/
COPY --from=pgvector /usr/share/doc/pgvector /usr/share/doc/pgvector

LABEL dev.memory-layer.fixture="trust-receiver-pg15"
LABEL dev.memory-layer.postgres="15.18"
LABEL dev.memory-layer.pgvector="0.8.6"
LABEL dev.memory-layer.postgres-source="docker.io/library/postgres@sha256:752791ad6ccfc6fbfcf4d9b43413c8642901f1c2008ebdc1423ea32fd66fd4f1"
LABEL dev.memory-layer.pgvector-source="docker.io/pgvector/pgvector@sha256:a6579dded3ff4ed3d9565ae54ab253e2cb7ae24c5d1eb3bec8a89e73f038b68a"
