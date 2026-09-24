# CA package binding: PostgreSQL 17 on Debian 12 ARM64

Bound on 24 September 2026 and last observed after the repository was re-signed
at 20:53:49 UTC. This is the reviewed package identity for the CA PostgreSQL-17
replacement. It is not authority to change the CA outside the separately
approved replacement instruction.

## Repository identity

Use the official PostgreSQL Apt Repository for Debian 12 ARM64. Do not change
Linux distribution and do not use an Ubuntu, Raspberry Pi, testing or unstable
PostgreSQL repository.

```text
Types: deb
URIs: https://apt.postgresql.org/pub/repos/apt
Suites: bookworm-pgdg
Components: main
Architectures: arm64
Signed-By: /usr/share/keyrings/postgresql-pgdg.asc
```

| Evidence | Expected value |
| --- | --- |
| Signing-key fingerprint | `B97B0AFCAA1A47F044F244A07FCC7D46ACCC4CF8` |
| ASCII key SHA-256 | `0144068502a1eddd2a0280ede10ef607d1ec592ce819940991203941564e8e76` |
| Signed `bookworm-pgdg` InRelease date | `Thu, 24 Sep 2026 20:53:49 UTC` |
| Observed InRelease SHA-256 | `37f6aa488afe294015f1328834843b215fbcbde62c41e339c9ff1dcf9930c34b` |
| Observed signed ARM64 `Packages.gz` SHA-256 | `e6dec1f2e121c9f47903a85438c742ecf5442aa695d8d8a6340ca71dfc235f69` |
| Observed signed ARM64 `Packages.gz` size | `1231876` bytes |

The CA agent must validate the key fingerprint and key hash before trusting the
repository. It must verify the InRelease signature with that key and verify the
signed ARM64 index entry against the downloaded index. PGDG may regenerate the
signed index while retaining the authorized package artifacts. A changed
InRelease or `Packages.gz` identity is recorded but is not, by itself, a stop
condition. Continue only when the signature is valid under the bound key and
every authorized package below has the exact bound version and `.deb` hash. Do
not silently substitute newer packages.

The prior signed-index binding (`2397236c...` / `a8c72003...`) was correctly
rejected by the CA after PGDG re-signed the repository. The refreshed signed
index above was independently downloaded and verified on Monolith. Its signed
SHA-256 entry for `main/binary-arm64/Packages.gz` matches the downloaded file,
and every exact package version and `.deb` hash below is unchanged. This refresh
authorizes no newer package candidate.

Authoritative upstream references:

- <https://www.postgresql.org/download/linux/debian/>
- <https://apt.postgresql.org/pub/repos/apt/README>
- <https://wiki.postgresql.org/wiki/Apt>

## Exact package set

The signed ARM64 package index above contains these reviewed candidates:

| Package | Version | Architecture | `.deb` SHA-256 |
| --- | --- | --- | --- |
| `postgresql-17` | `17.11-1.pgdg12+2` | arm64 | `a93463e4f4ef9e11acfc9c541e9ba59ccefccc9fc09a89c7d6f4989ae40ba020` |
| `postgresql-client-17` | `17.11-1.pgdg12+2` | arm64 | `a9e43d4fabea44c52d7be60996e1447b857e1e78c844ca5beb8cd87f05f6fd57` |
| `postgresql-17-pgvector` | `0.8.6-1.pgdg12+1` | arm64 | `ec9959c0614b6ba722d9ef530e41fec8a8faaf1684300e4e35b82bc6a55d01f3` |
| `postgresql-common` | `293.pgdg12+1` | all | `f2fb700d5a16075f73e12932830f81b0dad176c7c7d924c5c26bc96cdcfccbc6` |
| `postgresql-client-common` | `293.pgdg12+1` | all | `a4e2461975abffae23688fc95c4fdc97fea4c4c9cc096c2c4a7a4e334dfc3353` |
| `libpq5` | `18.6-1.pgdg12+2` | arm64 | `1e2b364b93ff16293d6f4a409e49b0d8c0521b9f7ae3423007f443c31c0f6766` |

Do not install `postgresql-server-dev-17`; the binary pgvector package makes it
unnecessary. `pg_upgradecluster --check` compares major-version package names,
so the old build-time-only `postgresql-server-dev-15` package creates a false
parity requirement. If it is the sole parity failure, simulate its removal and
continue only when exactly that package would be removed and no installed
package has a reverse dependency on it. Remove it without autoremove, retain the
package inventory in evidence, and rerun the normal unforced check. Do not use
`--force-packages`.

Debian dependencies such as `libllvm19` may be added only if the APT simulation
identifies them by exact package/version and the CA agent records them locally.
The simulation must propose no unexpected removal, downgrade, PostgreSQL-15
server replacement or distribution upgrade.

Upgrading `postgresql-common`, `postgresql-client-common` and `libpq5` is an
expected part of this package set. They are shared with PostgreSQL 15, so this
is the principal existing-service risk. PostgreSQL 15 must not restart, stop,
move or change cluster identity during the inactive Trust preparation.

## Fail-closed rule

Any mismatch in signing identity, signature, architecture, bound package
version, bound `.deb` hash or reviewed APT transaction stops the change. A
newly generated index is acceptable only under the rotation rule above. The CA
agent records the observed index identity and preserves its local evidence. It
does not find an alternative repository, upgrade Debian or relax artifact
verification.
