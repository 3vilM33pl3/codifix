# Respond to a service outage

The Debian package installs the example at `/usr/share/codifix/agency/incidents.json.example` and the systemd inspector at `/usr/libexec/codifix/agency/inspect-systemd.py`.

The Homelab Agent accepts Alertmanager webhooks and reports from authorized agencies. It assigns one incident to an active service outage, records the initial impact, and uses the service's configured investigation and repair commands. The operator and reporting agents receive progress in their private Codifix Inbox. The Linear Agent maintains one incident issue and creates linked follow-up issues.

## Configure the agencies

1. Add the capability `incident_report` to the Homelab specialist. Give its mandate the resource `{"kind":"service","id":"catalogue"}` for each monitored service. Configure a dedicated nonoperator peer for Alertmanager and grant each allowed reporting peer the mandate.
2. Configure the outbound Linear peer on the Homelab instance. On the Linear instance, authorize the Homelab peer for `linear_track` with its team and project. The [Linear configuration example](../../crates/codifix-agency-runtime/examples/linear-agent.json) shows the project mapping.
3. Copy the [incident profile](../../crates/codifix-agency-runtime/examples/incidents.json) to an operator-owned file and set `CODIFIX_INCIDENT_PROFILE` to its absolute path in the agency service environment. Replace the example IDs, executable paths, service name and request URL. Install the [example systemd inspection command](../../crates/codifix-agency-runtime/examples/inspect-systemd.py) or supply a service-specific one. Create the runbook file and give the agency write access to it and the postmortem directory.
4. Configure Alertmanager's webhook receiver to send firing alerts to `/v1/incidents/alertmanager` on the Homelab instance with the dedicated peer's bearer token. Map the service name to the `service` label. Codifix uses the alert fingerprint and start time to return the same receipt for repeat deliveries.
5. Enable Inbox on the Homelab instance and set `inbox.operator_conversation` in the incident profile to the operator's existing private conversation ID. To deliver to an operator on another agency instead, set `inbox.operator_peer` to its enrolled private AT peer ID. List the allowed reporting agents in `inbox.reporting_peers`. A listed agent receives updates only for incidents it reported.
6. Each remote recipient enables Inbox and sets `social.peers.<homelab-peer>.incident_conversation` in its agency configuration to its own private conversation ID. The sender needs the peer endpoint and AT enrollment. Each method-bound service token delivers a structured update through `dev.codifix.agency.deliverIncidentNotice`. The recipient chooses the destination and deduplicates retries. No custom notification program is required.

For another notification channel, the optional `notify` command accepts JSON on stdin containing `incident_id`, `task_id`, `event_id`, `service`, `reporters` and `update`. Return success after delivery and deduplicate by event ID.

## Reports and repair decisions

Agents submit a private task using the authorized specialist and mandate, capability `incident_report`, the service resource, and acceptance criterion `incident_report_processed`. Its payload contains `kind: "incident_report"`, an RFC 3339 `starts_at`, and a short impact `summary`. Reports for an open outage or a known outage interval join the same incident. Read `GET /v1/incidents/{id}` as an authenticated operator or reporting peer for the evidence, timeline, verification and Linear link.

The inspection command emits JSON with `impact`, `logs`, `metrics`, `recent_changes`, `cause` and the configured `repair` name. Optional `runbook_note` and `followups` describe concrete lessons and further work. Write short summaries suitable for authorized reporters and Linear. Use the service's logs, metrics, deployment history and relevant dependencies to establish the cause. State the uncertainty when the evidence is incomplete.

Each repair has a condition, a fixed command and an explicit decision about autonomous use. A passing condition permits the configured autonomous action. Other repairs require the operator to answer the originating task with the displayed `approve <digest>`, which binds the action, risks, recovery options and investigation. Failed conditions or unknown repair names require intervention. Commands run with a 30-second timeout.

The worker verifies both service health and a representative user operation. A failed probe keeps the incident open. After manual intervention, answer the task to request another verification. A verified recovery records the outcome in Linear, appends applicable lessons to the runbook and writes a Markdown postmortem for a major repair. Pending ticket or notification delivery appears in the incident timeline.
