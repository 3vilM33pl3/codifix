# CA package binding: PostgreSQL 17 on Debian 12 ARM64

Bound on 24 September 2026. This is the reviewed package identity for the
inactive Trust preparation. It is not authority to change the CA.

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
| Signed `bookworm-pgdg` InRelease date | `Tue, 22 Sep 2026 15:15:39 UTC` |
| InRelease SHA-256 | `2397236cabb65bf45f901a850a31ad96e63953345c487af9be555bc0af15dce0` |
| Signed ARM64 `Packages.gz` SHA-256 | `a8c72003102b04151b052936f0ddfef746da74bdf36ff925773ec18e43b2a6a3` |

The CA agent must validate the key fingerprint and key hash before trusting the
repository. It must verify the InRelease signature with that key. If the live
repository index has changed, stop and report the newly signed identity; do not
silently substitute newer packages.

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
unnecessary. Debian dependencies such as `libllvm19` may be added only if the
APT simulation identifies them by exact package/version and the CA agent lists
them in its pre-change report. The simulation must propose no removal,
downgrade, PostgreSQL-15 server replacement or distribution upgrade.

Upgrading `postgresql-common`, `postgresql-client-common` and `libpq5` is an
expected part of this package set. They are shared with PostgreSQL 15, so this
is the principal existing-service risk. PostgreSQL 15 must not restart, stop,
move or change cluster identity during the inactive Trust preparation.

## Fail-closed rule

Any mismatch in repository identity, architecture, version, package hash or APT
transaction stops the change before installation. The CA agent reports the
observed value and preserves its local evidence. It does not find an
alternative repository, upgrade Debian or relax the receiver profile.
