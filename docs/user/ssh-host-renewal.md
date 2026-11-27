# SSH host certificate renewal

Run the Homelab companion on the SSH host and the CA companion next to the operator's Step CA console. Each needs Python 3 and the `cryptography` package with OpenSSH certificate support (version 40 or newer). The Homelab host needs OpenSSH client and server tools. Only the operator's console needs the `step` CLI and its signing credentials. The agents exchange the public host key and renewal metadata over [private companion XRPC](https://codifix.dev/docs/network-inbox). A per-task OpenBao KV v2 entry carries the signed certificate.

On Debian the companion is `/usr/bin/codifix-agency`. The package includes its embedded Python adapter source at `/usr/libexec/codifix/agency/ssh-renew.py`. Before enabling renewal, check the exact Python runtime available to the service with `python3 -I -c "from cryptography.hazmat.primitives.serialization import SSHCertificate"`. Debian Bookworm installations with older cryptography need a Python runtime that provides this interface.

## Configure the two roles

Give each service its own `CODIFIX_SSH_RENEWAL_CONFIG` environment variable naming an operator-owned JSON file. The top-level key is the exact host name. Use the same reviewed host name, host key fingerprint, allowed principals and trusted SSH CA public key on both sides. These examples use synthetic identities and paths.

The CA profile binds the exact requesting companion peer ID and the local operator peer ID. Grant the Homelab peer `ssh_host_sign` on the resource `{"kind":"ssh_host","id":"build.example.org"}` with criterion `approved_certificate_handed_off`. Scope the CA token to KV create at `secret/data/ssh-renewals/*`. The staging directory contains public certificates only. Permit the signing operator to write there and the companion to read it.

```json
{
  "build.example.org": {
    "host": "build.example.org",
    "principals": ["build.example.org"],
    "fingerprint": "SHA256:REPLACE_WITH_ENROLLED_HOST_PUBKEY_FINGERPRINT",
    "ca_public_key": "/etc/codifix/ssh-host-ca.pub",
    "requester": "homelab",
    "operators": ["ca-operator"],
    "stage_dir": "/var/lib/codifix/ssh-signed",
    "step_url": "https://ca.example.org",
    "step_root": "/etc/codifix/step-root.crt",
    "step_issuer": "ssh-operator",
    "step_duration": "24h",
    "bao_url": "https://bao.example.org",
    "bao_ca": "/etc/codifix/bao-ca.pem",
    "bao_prefix": "secret/data/ssh-renewals",
    "bao_token_env": "CODIFIX_BAO_CERT_WRITER"
  }
}
```

On the Homelab host use the following profile. The login identity belongs to a restricted verification user. The probe starts a new SSH process. OpenSSH validates the pinned CA and its KnownHostsCommand checks the exact certificate presented during that login. Grant the local `expiry-alert` peer the capabilities `ssh_host_inspect` and `ssh_host_install` for this host, with criteria `served_host_certificate_observed` and `fresh_ssh_login_verified`. Give it a mandate with enough turns for both tasks. Supply a separate read-only OpenBao token. The host runtime needs permission to replace its certificate, validate the configured sshd and reload that service. The deployment owns the paths and the maintenance window, expressed in UTC as inclusive start and end minutes.

```json
{
  "build.example.org": {
    "host": "build.example.org",
    "principals": ["build.example.org"],
    "fingerprint": "SHA256:REPLACE_WITH_ENROLLED_HOST_PUBKEY_FINGERPRINT",
    "ca_public_key": "/etc/codifix/ssh-host-ca.pub",
    "operators": ["ca-operator"],
    "address": "127.0.0.1",
    "port": 22,
    "host_public_key": "/etc/ssh/ssh_host_ed25519_key.pub",
    "host_certificate": "/etc/ssh/ssh_host_ed25519_key-cert.pub",
    "sshd_config": "/etc/ssh/sshd_config",
    "sshd_service": "sshd.service",
    "login_user": "codifix-probe",
    "login_identity": "/run/credentials/codifix/probe-key",
    "window_utc": ["02:00", "02:30"],
    "install_requesters": ["expiry-alert"],
    "bao_url": "https://bao.example.org",
    "bao_ca": "/etc/codifix/bao-ca.pem",
    "bao_prefix": "secret/data/ssh-renewals",
    "bao_token_env": "CODIFIX_BAO_CERT_READER",
    "coordinator": {
      "home_requester": "expiry-alert",
      "home_specialist": "homelab",
      "home_mandate": "00000000-0000-4000-8000-000000000001",
      "ca_peer": "ca",
      "ca_specialist": "authority",
      "ca_mandate": "00000000-0000-4000-8000-000000000002",
      "renew_before_seconds": 604800,
      "deadline_seconds": 259200
    }
  }
}
```

Apply migrations and start both agency workers. Configure the host expiry monitor to derive a stable UUID per alert and invoke the following command when the alert fires, and periodically while open. The command advances the durable event, reusing its original task IDs and deadline. An early alert returns `not_due`. Outside the maintenance window an approved event returns `waiting_for_window`.

```sh
codifix-agency --config home.json ssh-renew --host build.example.org --event "$EVENT_ID"
```

When signing is pending, the result includes `ca_task`. On the CA console, inspect the task's requester, host, public key fingerprint, expiry and requested principals. Approve and interact with Step through its normal provisioner, or reject with a reason. The CLI uses Step's `--host --sign` mode on the existing public key. If another operator-approved Step ceremony already produced a certificate, stage it as `<ca_task>-cert.pub` before approving. The worker checks its signature, host key, principals and extended expiry before writing it to OpenBao.

```sh
codifix-agency --config ca.json ssh-decision --peer ca-operator --task "$CA_TASK" --approve
codifix-agency --config ca.json ssh-decision --peer ca-operator --task "$CA_TASK" --reject "Host ownership needs review"
```

A successful installation reports the approving peer ID, certificate expiry and verified fresh login. If configuration validation, reload, served-certificate readback or login fails, the host restores the earlier certificate and reloads the prior configuration. Failed and rejected operations retain the reason in task evidence. After correcting the cause, use a fresh expiry event.

## Disposable acceptance

With a disposable pgvector database and Docker access, run the following test. It uses Step CA, OpenBao KV policies, PostgreSQL, two companion workers, private XRPC with synthetic signed service tokens, and an actual fresh SSH login. The host requires `step`, `bao`, `sshd`, `ssh`, `ssh-keyscan`, `openssl`, `socat` and Python cryptography. The independent real-PDS transport gate is `node tests/federation-pds/run.mjs --agency`.

```sh
CODIFIX_TEST_REQUIRE_DB=1 cargo test --locked -p codifix-agency-runtime --test ssh_renewal -- --ignored --nocapture
```
