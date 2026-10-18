# Knowledge area routing

For the everyday choice of an area and project, start with
[Areas and projects](https://codifix.dev/docs/areas). This page covers the
operator-owned routing policy in current source on `main`; it does not imply
that the published `3.0.0-rc.1` package includes later routing changes.

Codifix can route local CLI, TUI and watcher traffic to independently operated
areas. Install an operator-owned JSON file at
`~/.config/codifix/area-routing.json` (or the equivalent XDG location), or
select it with `--area-routing /absolute/path.json` /
`CODIFIX_AREA_ROUTING`. On Unix the file must belong to the current user or
root and must not be group/world writable. Use mode `0600` or `0644`.

```json
{
  "version": 1,
  "default_area": "general",
  "areas": {
    "workshop": {
      "instance_id": "22222222-2222-4222-8222-222222222222",
      "config": "/etc/codifix/instances/workshop/client.toml"
    },
    "general": {
      "instance_id": "11111111-1111-4111-8111-111111111111",
      "config": "/etc/codifix/instances/general/client.toml"
    },
    "trust": {
      "instance_id": "33333333-3333-4333-8333-333333333333",
      "config": null
    }
  },
  "routes": [
    {"area": "workshop", "repository": "/srv/repos/example-app"},
    {"area": "workshop", "project": "example-app"}
  ]
}
```

Replace every example identity with its enrolled identity. Add Homelab
and its exact repositories in the same format. Each locally usable area has a
distinct absolute config path and distinct instance identity. A remote-only
area has `config: null`; choosing it for local queries or writes produces an
error directing the caller to its host or explicit remote expertise.

Every selected application configuration includes its matching identity:

```toml
[area]
name = "workshop"
instance_id = "22222222-2222-4222-8222-222222222222"
```

Keep the ordinary service, database and authorization settings in that area's
configuration. The routing policy contains no credentials. Loading a route
checks that the application's identity matches the routing entry.

Selection order is:

1. Explicit `--area workshop` / `CODIFIX_AREA`.
2. An exact repository identity route (shared Git common directory for linked
   worktrees, otherwise canonical path), additionally matching the project when
   the rule specifies one.
3. An exact project-only route.
4. General.

Conflicting matches fail with an explicit ambiguity error. Thus two repositories
can have the same project slug and still route to different areas; outside those
repositories use `--area` when the project name is ambiguous. Repository routes
do not implicitly include sibling repositories or descendants. An unavailable
selected config/service never redirects into General. Explicit `--config`
must agree with the selected route for knowledge commands.

```bash
codifix --area workshop query --project example-app --question "What changed?"
codifix --area homelab tui --project example-infra
codifix --area workshop status --project example-app --json
```

Status JSON includes `area.name` and `area.instance_id`. The TUI header shows the
area, instance and executable release. Existing federation views provide its
synchronization and expertise state.

## Watchers and offline ownership

Use one watcher manager per profile. It reloads the routing file on each scan,
resolves each agent repository independently and pins the child's exact area,
config and routing file. Managed service names contain area, instance and agent
session identity. The real agent session ID is preserved in capture attribution.
Invalid or removed routes stop the affected managed watcher; the manager does
not start a fallback writer. A routing/config identity change stops the previous
managed child before starting its replacement.
Each daemon holds a kernel lock for its area's repository state, preventing a
manual second watcher from racing the manager. Crashes release the lock.

Area deployments reject `watcher enable`, which installs a legacy project-only
service. Use the manager and stop old legacy watchers during the coordinated
cutover. Named application services are managed by the release operator; legacy
`service enable/disable` cannot target an area. Explicit `service run --config`
remains pinned to its operator-supplied configuration.

Area configurations derive filenames containing both name and instance UUID for
offline DuckDB, watcher state, watcher audit and flush requests. Even previously
shared explicit paths therefore become distinct. Repeated loading produces the
same filenames. No capture/event IDs are rewritten. Old unscoped offline queues
and watcher state remain on disk for the reviewed cutover reconciliation; they
are not automatically replayed into a newly selected area.

For managed deployments pin `CODIFIX_AREA_ROUTING` to the installed file so
its removal is an error. Without a routing file or area selection, legacy
installations continue unchanged. Development binaries do not automatically
discover the production routing file; explicit `--area-routing` opts them in.
Package/setup commands do not route knowledge. The daemon and manager retain
their explicit launch configuration while the manager selects each child area.
