# syntax=docker/dockerfile:1
# SPDX-License-Identifier: AGPL-3.0-or-later
# Disposable Debian-12/Python receiver test runner. Not a CA package.
ARG PGVECTOR_IMAGE=pgvector/pgvector@sha256:cf134a767f474095eeba57e0117be8e568e011a63f33fbf252f14c9b760f8e6f
ARG BOOKWORM_IMAGE=memory-ca-bookworm-builder:20260920

FROM ${PGVECTOR_IMAGE} AS pg17
FROM ${BOOKWORM_IMAGE}
COPY --from=pg17 /usr/lib/postgresql/17/bin/psql /usr/lib/postgresql/17/bin/psql
COPY --from=pg17 /usr/lib/x86_64-linux-gnu/libpq.so.5* /usr/lib/x86_64-linux-gnu/
LABEL dev.memory-layer.fixture="trust-receiver-runner"
LABEL dev.memory-layer.postgres-client="17.11"
LABEL dev.memory-layer.bookworm-source="sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac"
