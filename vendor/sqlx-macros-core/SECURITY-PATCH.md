# SQLx PostgreSQL dependency resolution

This source and the companion `sqlx-macros-core` overlay come from SQLx
0.8.6 at commit `bab1b022bd56a64f9a08b46b36b97c5cff19d77e`. Both use the
upstream MIT and Apache 2.0 licences.

The Cargo manifests omit weak feature forwarding to the unused MySQL and
SQLite drivers. This keeps those drivers, SQLite's native bindings and the
MySQL driver's RSA dependency out of the PostgreSQL release lock. The
PostgreSQL driver, features, migration macros and runtime source are upstream
0.8.6. Cargo test and bench targets for the upstream checkout are omitted
from the vendored dependency manifest.
