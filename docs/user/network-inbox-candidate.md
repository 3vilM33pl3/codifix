# Network/Inbox Linux candidate rehearsal

This runbook is for exercising the **3.1.0-rc.1** Codifix package and the
**0.2.0-rc.1** Linux desktop package in a disposable environment. It does not
authorize a production installation, production grants, real AT login, live
Linear changes, public release, or CA/SSH changes. Candidate builds and the
checks below are not a production acceptance result. Follow this page and the
[Network and Inbox operator setup](network-inbox.md#operator-setup) from the
source revision named by the candidate manifest.

The candidate artifacts are staged under
`/absolute/path/to/candidate-artifacts` on the
build host. For portable commands below, set `CANDIDATE_DIR` to that directory
or to the directory containing the candidate's delivered manifest, provenance,
checksums, and artifacts. The manifest is the authority for filenames and
build/provenance paths; do not infer a package from its name alone.

## 1. Verify candidate bytes and provenance

Use a clean checkout of the source revision named by the manifest. Read the
manifest and provenance records before selecting artifacts. Confirm the core
version is `3.1.0-rc.1`, the desktop version is `0.2.0-rc.1`, and the source
revision is the reviewed commit. Verify that every artifact digest matches the
manifest and its supplied SHA-256 checksum file. For example, from the
candidate directory:

```sh
export CANDIDATE_DIR=/absolute/path/to/network-inbox-candidate
find "$CANDIDATE_DIR" -maxdepth 2 -type f -print | sort
cat "$CANDIDATE_DIR/candidate-manifest.json"
(cd "$CANDIDATE_DIR" && sha256sum -c SHA256SUMS)
```

The delivered checksum file and its basenames are authoritative; if the
manifest changes the layout, follow its recorded paths. Compare every
artifact's digest and source revision in the build/provenance record as well: a
checksum only proves that the bytes match that record. Candidate files are
unsigned unless the manifest explicitly says otherwise. Keep the candidate
directory intact while reviewing; builds refuse to overwrite existing release
artifacts.

## 2. Exercise the core Debian package lifecycle

Run the isolated Docker lifecycle harness from the same clean source checkout
as the candidate. The image must already be prepared and available locally;
pass its full immutable Docker image ID, not a tag. The image must contain the
candidate package's declared runtime dependencies, including Python 3.11 or
newer, `gpg`, `libc6`, `libgcc-s1`, and `libstdc++6`. A stock Ubuntu image is not
enough unless those dependencies and the matching architecture are present.
From the clean source checkout, prepare the supplied test image once and record
the resolved image ID alongside the test output:

```sh
docker build -f packaging/tests/Dockerfile.install-test \
  -t codifix-debian-install-test:local .
docker image inspect --format '{{.Id}}' codifix-debian-install-test:local
```

The build may fetch the base image and Debian packages. The lifecycle run below
uses only the captured local image ID and does not pull or use the network.

```sh
docker image inspect --format '{{.Id}}' sha256:FULL_LOCAL_IMAGE_ID
bash packaging/tests/test-debian-install-docker.sh \
  sha256:FULL_LOCAL_IMAGE_ID \
  "$CANDIDATE_DIR/<core-package-from-manifest>.deb"
```

The inspect command must print the exact `sha256:...` ID supplied to the
harness. The harness uses `--pull never`, `--network none`, only read-only
mounts for the selected package and harness, and bounded container resources.
Docker removes this one-shot container on exit with `--rm`; do not use broad
container cleanup commands that could affect other work.
It runs a fresh install, upgrades from a synthetic `3.0.0` package assembled
from the candidate's own package hooks, repairs the candidate, and removes it.
It checks that operator configuration/data sentinels survive and that no
service is started. The synthetic prior package is a lifecycle fixture, not a
historic release package. A passing lifecycle check does not run the service,
provision a database, apply migrations, or exercise Inbox behavior.

Do not use the hosted-runner lifecycle script for this rehearsal. The separate
`packaging/ca-bookworm/install-test.sh` is intentionally restricted to a fresh
native GitHub-hosted runner and must not be run locally or inside this Docker
container. Do not invoke the harness's internal `--container` mode directly or
otherwise bypass the outer harness.

The package hooks preserve existing configuration and runtime data. Install
does not enable or start services and does not migrate a database. The optional
agency service unit is shipped only as an example under
`/usr/share/codifix/agency`; no active unit is installed. This application-data
location preserves the setup examples even when a minimal image excludes docs. Never point a candidate service at a
live Codifix database.

## 3. Inspect and smoke the desktop package

The separate desktop lifecycle harness uses its own prepared Ubuntu 24.04
image with GTK and WebKit runtime dependencies. Run it by exact image ID:

```sh
docker build -f packaging/tests/Dockerfile.desktop-install-test \
  -t codifix-desktop-install-test:local .
desktop_test_image=$(docker image inspect --format '{{.Id}}' codifix-desktop-install-test:local)
bash desktop/tests/test-debian-install-docker.sh "$desktop_test_image" \
  "$CANDIDATE_DIR/codifix-desktop_0.2.0~rc.1_amd64.deb"
```

This checks runtime library/symbol resolution, installed payload bytes, fresh
installation, upgrade from a synthetic `0.1.4` fixture, same-version repair and
removal. Disposable user XDG configuration/data survive; the package contains
no maintainer hooks and the harness starts neither the GUI nor a service. The
fixture is not a historical released desktop package. Never invoke the
harness's internal `--container` mode on the host.

Select the desktop DEB from the manifest, verify its hash, and inspect its
Debian metadata and archive without installing it on the build or operator
host:

```sh
DESKTOP_DEB="$CANDIDATE_DIR/<desktop-package-from-manifest>.deb"
dpkg-deb -f "$DESKTOP_DEB" Package Version Architecture Depends
dpkg-deb -c "$DESKTOP_DEB"
desktop_unpack=$(mktemp -d)
trap 'rm -rf -- "$desktop_unpack"' EXIT
dpkg-deb -x "$DESKTOP_DEB" "$desktop_unpack"
find "$desktop_unpack" -type f -perm /111 -print
```

An extraction/package check proves the archive can be read and inspected. A
desktop startup smoke must be performed inside a disposable graphical Linux
environment with its own user profile and keyring; use the executable path
shown by `dpkg-deb -c`. Confirm that the window reaches its initial signed-out
state and exits cleanly. This checks process startup only. It does not establish
full GUI, accessibility, native portal, notification, or real service login
acceptance. Do not install the package on a host that contains operator data.

## 4. Run the source-level isolated Inbox workflow

Use the exact clean source revision named by the manifest. Point the test only
at a disposable PostgreSQL database with `vector` installed and permission to
create test databases. The script requires `CODIFIX_TEST_DATABASE_URL`, sets
`CODIFIX_TEST_REQUIRE_DB=1`, and supplies a fixed synthetic Linear test token;
do not substitute a live token or a live Linear account.

```sh
cd /path/to/clean/codifix-source-at-manifest-revision
export CODIFIX_TEST_DATABASE_URL='postgresql://TEST_USER:TEST_PASSWORD@127.0.0.1:5432/TEST_DATABASE'
bash scripts/test-inbox-candidate.sh
```

The script builds the native desktop test executable and runs the ignored
`network::candidate::native_inbox_linear_workflow` service test, which requires
the disposable database. If you use a separate build directory, set
`CODIFIX_DESKTOP_TARGET_DIR` to that directory before running the script. Keep
the database disposable; remove only the database/container created for this
rehearsal after preserving the test output you need. Do not reuse a developer,
staging, or production database.

This test is intended to cover the native transport, service, Inbox ticket,
agency/model and local Linear fixture path, including plan preparation and
apply/readback, using synthetic auth and exact test grants. Its completion
marker is `INBOX_CANDIDATE_VERIFIED`. Do not treat that marker by itself as
proof that every package, renderer or live account path passed; retain the
actual test output and compare it with the acceptance evidence for the same
source revision.

## 5. Optional identity broker

The broker is optional; private enrolled conversations do not require it. If an
archive is present in the candidate manifest, verify its digest and inspect its
bundled manifest, source/lock provenance, notices and launcher before use. Check
`node --version`; the archive requires Node.js 22 or newer and was validated
with Node 24. Verify that the launcher check works without starting the broker:

```sh
cd /path/to/extracted-broker-archive
node launch.mjs --check
```

The archive pins
`@atproto/oauth-client-node` 0.5.13. The known Zod advisory
[GHSA-mrhx-c7xc-m3w5](https://github.com/advisories/GHSA-mrhx-c7xc-m3w5) remains
unresolved; request/response bounds are a mitigation, not a fix. The archive
and example service unit remain disabled and non-production. A synthetic
Unix-socket smoke is possible only inside the disposable, network-isolated
environment described by the archive README; any explicit candidate opt-in is
confined to that smoke. Never enable the unit, use a real account, or expose a
public identity callback in this rehearsal. See the [identity broker
runbook](../../identity-broker/README.md) for the design and limits.

## 6. Exercise Inbox in an isolated environment

Package and source-level checks do not replace operator exercise. Use a
disposable service and agency deployment backed by a disposable shared
PostgreSQL database. Follow [Operator setup](network-inbox.md#operator-setup)
and the user steps in [Network and Inbox](network-inbox.md#using-the-client),
but create fresh test-only humans, exact test grants, test contacts, and a local
Linear fixture. Keep the identity broker disabled. Do not use production
service URLs, human sessions, API tokens, grants, mandate IDs, model
credentials, Linear credentials, or databases. Verify that Network and Inbox
load without a selected project; exercise a private conversation, reviewed
participant admission, task progress, clarification, cancellation and result
readback using only synthetic data.

Record which artifact/source revision was used, the manifest and artifact
hashes, commands and outputs, disposable service/database setup, and each
manual result. Distinguish extraction or process-startup smoke from an
interactive GUI session and from the full native transport/database workflow.
Report a check as passed only when its corresponding output or manual evidence
has been retained.
