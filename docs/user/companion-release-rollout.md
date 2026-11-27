# Roll out a tested CA companion release

The Debian package installs the publisher command at `/usr/libexec/codifix/agency/release-rollout.py`. The examples below use its source-tree path.

The publisher certifies a specific companion executable and asks an enrolled CA to install it. The CA uses a local supervisor process so it can restart the companion that received the request. A short outage is expected. The commands below use example paths for one publisher and one CA.

## Certify the candidate on the publisher

Supply the executable for the CA’s platform, an executable smoke test, and an executable that exercises upgrading a disposable installation from the previous release. Each test receives the candidate path as its sole argument and must exit successfully. Use a fresh receipt filename. The receipt records the version, release URL, executable SHA-256 and SHA-256 of each test’s output. Both successful gate logs remain next to the receipt as `<sha256>.log` with owner-only read/write permissions. The CA receives the two hashes as evidence that certification passed.

```sh
python3 scripts/agency/release-rollout.py certify \
  --artifact /srv/releases/candidate/codifix-agency \
  --version "$CANDIDATE_VERSION" --release-url "$RELEASE_URL" \
  --artifact-test /srv/release-tests/check-companion \
  --upgrade-test /srv/release-tests/upgrade-from-previous \
  --receipt /srv/releases/candidate/tested.json
```

Stage that executable and receipt on the CA through your normal release distribution process. The CA reads its local copies. The release URL identifies the candidate in the evidence.

## Prepare a Debian ARM64 release for the CA

Freeze the commit containing the intended workspace version. The rebuilt companion reports that version from Cargo. On the build host, point `RELEASE_REV` to the full commit ID, `ARM_BUILD` to a fresh absolute directory, and `BUILDER_IMAGE` to the immutable Bookworm builder image ID. The build verifies the ARM64 ABI and runs version/help checks under QEMU.

```bash
install -d -m 700 "$ARM_BUILD"
bash packaging/ca-bookworm/release.sh "$RELEASE_REV" "$ARM_BUILD" "$BUILDER_IMAGE" arm64
```

The package lifecycle and companion rollout use different artifacts. Run the repository's native Debian package lifecycle gate on a disposable ARM64 runner. On that runner, extract the companion for the certification command above:

```bash
set -euo pipefail
packages=("$ARM_BUILD"/output/*_arm64.deb)
test "${#packages[@]}" -eq 1
test "$(dpkg-deb -f "${packages[0]}" Architecture)" = arm64
dpkg-deb -x "${packages[0]}" "$ARM_BUILD/extracted"
sha256sum "${packages[0]}" "$ARM_BUILD/extracted/usr/bin/codifix-agency"
```

Pass the extracted `usr/bin/codifix-agency` as `certify --artifact`. Give its upgrade test the prior **ARM64** executable and a disposable CA environment. The test should install the candidate over the previous version, restart it and check the running version. Preserve the executable, its receipt, and the two gate logs together. Stage the exact certified bytes on the CA. A package installer can consume a locally staged package provided its installed companion matches that receipt. The supervisor checks the executable path selected by the actual companion service.

## Configure the two peers

On the publisher, configure the private rollout JSON with `agency_binary`, `agency_config`, `local_peer`, `pds`, `did`, `pds_token_env`, `help_capability` and `delegation`. Set the named PDS token in the publisher process environment. Choose the capability you intend to advertise publicly while help is available. The rollout request and task evidence travel between the enrolled peers over private XRPC.

A read-only observer and an existing inspect exchange can confirm connectivity. For rollout enrollment, the CA operator must separately authorize the publisher's private identity for `upgrade_candidate` scoped to `{"kind":"installation","id":"<CA instance UUID>"}`, and provision the local supervisor profile and installer. The publisher also needs an enrolled public profile that can temporarily advertise the chosen help capability, a source mandate for delegation, and ingress for the CA's authenticated `dev.codifix.agency.checkUpgrade` return call. Keep the supervisor in a separate process from the service it restarts.

The publisher’s delegation needs a current mandate for `delegate`, the CA peer resource, and the `delegated_result_reported` acceptance criterion. Its nested request targets the CA mandate for `upgrade_candidate` and uses the `upgrade_verified` criterion. Give both requests a deadline that covers installation and verification. The nested request needs a smaller delegation depth than its parent. The publisher inserts the certified receipt into that request.

Keep the CA supervisor profile under local administrator control. For example, use an independent supervisor with this profile:

```json
{
  "publisher_peer": "publisher",
  "release_receipt": "/srv/releases/candidate/tested.json",
  "artifact": "/srv/releases/candidate/codifix-agency",
  "installed_artifact": "/usr/local/bin/codifix-agency",
  "install_command": ["/usr/local/libexec/install-ca-companion"],
  "companion_health": "http://127.0.0.1:8787/health",
  "openbao_health": "https://bao.example.org/v1/sys/health"
}
```

The installer receives the staged executable as its final argument. It must install the binary at the configured path and restart the CA companion service. Set the health URLs to the actual companion and OpenBao instance. Make the relevant TLS trust roots available to the supervisor.

## Run and verify

Start the publisher from the repository root:

```sh
python3 scripts/agency/release-rollout.py deploy \
  --rollout /etc/codifix-agency/rollout.json \
  --receipt /srv/releases/candidate/tested.json
```

On the CA, run the local supervisor while the publisher waits:

```sh
codifix-agency --config /etc/codifix-agency/config.json \
  upgrade-once --profile /etc/codifix-agency/upgrade.json
```

A response of `{"handled":false}` means no queued request was available. Invoke the supervisor again after the request arrives. The publisher prints the task ID and status changes. Success requires the installed candidate digest, the running companion’s advertised version, initialized and unsealed OpenBao, and an authenticated return message from the restarted CA to the publisher. A sealed OpenBao requires the normal custodian ceremony. Use the reported task ID and failed step to inspect an unsuccessful rollout.

The executable integration example in [the disposable rollout fixture](../../tests/federation-pds/upgrade.py) constructs both mandates, the nested request and the installer. To exercise it locally, set `CODIFIX_TEST_PRIOR_AGENCY` to a previous companion executable and `CODIFIX_TEST_DATABASE_URL` to a disposable pgvector server with database creation privileges, then run `node tests/federation-pds/run.mjs --agency`.
