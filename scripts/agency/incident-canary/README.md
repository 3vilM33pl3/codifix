# Controlled incident exercise

Run the local test from this directory with `python3 -B -m unittest test_stage`. It uses a temporary database and loopback HTTP service. The production procedure below targets one service named `codifix-incident-canary`. Its only repair clears the active exercise in its own SQLite database.

## Stage the selected scope

Copy `scope.example.json` to a private working directory. Replace the illustrative instance, Codifix project, operator conversation, reporting-agent conversation, team and Linear project UUIDs with the operator's selection. Choose a disposable Linear project for the exercise. Use fresh mandate UUIDs and a distinct UUID for Alertmanager. Set each peer and specialist name to its enrolled identity. The `home.peer` name must resolve to Homelab at both remote agencies. Set `alert_webhook` to the private TLS ingress that routes the exact incident webhook path. Each invocation of `render` creates a new output directory.

```sh
python3 -B stage.py render "$SCOPE" "$STAGED"
```

1. On the Homelab host, install `service.py` as root-owned mode 0644 at `/usr/local/libexec/codifix-incident-canary/service.py` and install the adjacent systemd unit. Start that unit. Its service account and the Homelab agency must be able to write `/var/lib/codifix-incident-canary`. The unit creates this directory. Create `runbook.md` there, owned by the Homelab agency. With a sandboxed Homelab unit, permit that one directory in `ReadWritePaths`. Verify the item exists with `python3 -I /usr/local/libexec/codifix-incident-canary/service.py user-request`.
2. Set Homelab's `CODIFIX_INCIDENT_PROFILE` to the staged incident profile. For an existing incident profile merge the service and compatible shared identity settings. Enable Inbox, register the selected Homelab specialist with capability `incident_report`, and add the nonoperator alert peer from `alert-peer.json`. Add the Homelab grant ID to the enrolled reporting peer. Configure private AT peers and HTTPS endpoints for the reporting agency and Linear. On the reporting side, enable Inbox and merge `reporter-incident-inbox.json` into `social.peers.<home-peer>`; that recipient selects its own chat.
3. On the reporting agency, enable the reporter specialist's `delegate` capability. On Linear, add the `linear_track` capability and merge `linear-tracking-entry.json` into `linear.tracking`. Add the Linear grant ID to the Homelab peer on the Linear instance. Set the Linear PAT only in that provider's protected environment. Restart the changed agency services with their normal credentials and check `/health`. On **each owning instance**, run `codifix-agency --config "$CONFIG" grant --peer "$OPERATOR" --file "$STAGED/$ROLE-grant.json"` for its role `home`, `reporter` or `linear`. The grants expire after 30 days.
4. Create one dedicated random bearer shared by Homelab's `CODIFIX_CANARY_ALERT_TOKEN` and the Alertmanager-mounted Secret file `/etc/alertmanager/codifix-canary/token`. In the private ingress, forward **only** POST on the configured incident path to Homelab `/v1/incidents/alertmanager`; restrict ingress to the approved monitoring source. Preserve the Authorization header and verify TLS with the monitoring trust root. Merge `alertmanager-route.json` as the **first child** of the active route and append `alertmanager-receiver.json` to its receivers. The JSON objects are YAML-compatible. Use the monitoring deployment configuration for the mounted secret and route. Validate the rendered configuration with `amtool check-config` before applying. The exact alertname, service and canary label isolate this receiver from ordinary alerts.

## Observe a single repair

On the canary host run as `codifix-agency-homelab`, saving stdout in the exercise directory. `begin` makes the loopback service return HTTP 503 for both its health endpoint and catalogue item. The generated alert start and agent start identify the same outage.

```sh
python3 -I /usr/local/libexec/codifix-incident-canary/service.py begin > run.json
python3 -B stage.py exercise "$SCOPE" run.json "$EXERCISE"
```

Send the generated alert to the selected Alertmanager via its private API. For Pi Kubernetes a local port forward can supply `ALERTMANAGER`.

```sh
curl -fsS -H 'Content-Type: application/json' --data-binary @"$EXERCISE/alert.json" "$ALERTMANAGER/api/v2/alerts"
```

Wait for the initial alert acknowledgment in the operator Inbox. Submit the duplicate agent report **on the enrolled reporting agency**. Its existing private AT transport obtains a fresh PDS token for each method and signs the audience for the Homelab agency. The standard delegate worker verifies the peer response. Retain the local reporter task ID from `.contract.id`.

```sh
codifix-agency --config "$REPORTER_CONFIG" submit --peer "$REPORTER_OPERATOR" --file "$EXERCISE/reporter-delegate.json"
```

Wait for acknowledgments from both sources in the incident timeline. Review the proposed scope and risks in the Homelab operator task and send its exact `approve <digest>` answer. Reposting the same alert during this exercise uses the same alert fingerprint and start. The receiving agency verifies health and performs the catalogue item request. The canary's state records the incident ID that cleared the fault. The synthetic Alertmanager event expires after 45 minutes.

## Check independent receipts

1. Save `service.py state` as `final-run.json`. As an authenticated Homelab operator, save `GET /v1/incidents/{repaired_by}` as `incident.json`. As the reporting operator, save its completed A2A `GET /tasks/{reporter-task-id}` response as `report-task.json`, sending its bearer and `A2A-Version: 1.0`. The reads can use each instance's private operator endpoint.
2. Wait for Linear and Inbox outbox delivery. Collect the following receipts using locally authorized PostgreSQL read access on each owning instance. Use the incident and existing conversation UUIDs from the reviewed scope.

```sh
psql "$HOME_DB" -XAt -v ON_ERROR_STOP=1 -v incident="$INCIDENT" -f delivery.sql > delivery.json
psql "$HOME_DB" -XAt -v ON_ERROR_STOP=1 -v incident="$INCIDENT" -v chat="$OPERATOR_CHAT" -f inbox.sql > home-inbox.json
psql "$REPORTER_DB" -XAt -v ON_ERROR_STOP=1 -v incident="$INCIDENT" -v chat="$REPORTER_CHAT" -f inbox.sql > reporter-inbox.json
python3 -B stage.py check "$SCOPE" final-run.json incident.json report-task.json delivery.json home-inbox.json reporter-inbox.json
```

The checker requires the two source acknowledgments, the same incident returned by the signed peer report, one approved repair, both probes, verified Linear readback, drained event delivery and receiver-owned Inbox evidence. The major-incident postmortem is under `/var/lib/codifix-incident-canary/postmortems/<incident-id>.md`.
