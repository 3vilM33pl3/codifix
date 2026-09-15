# Inspected configuration, 09:17 UTC

Database max_connections = 200. Operations reserves eight connections; the job
service must fit within the remaining 192. The configured API pool maximum is 24
per process; twelve API replicas are active. A separate scheduler uses a maximum
of 12 connections in the same job-service budget. Pools open connections on
demand; configured maxima are not current session counts.

API in-flight request cap = 120 per replica. Pool acquisition timeout = 2 s.
SQL statement timeout = 15 s after acquisition. Requests rejected before work
starts receive an explicit busy response and a bounded retry hint. The report
worker currently starts an unbounded transaction for the daily audit summary.

The autoscaler uses API latency, not pool acquisition time or database connection
headroom. No read replica is configured. Restarting a pod discards its local
queue but is not a substitute for reconciling work that already started.
