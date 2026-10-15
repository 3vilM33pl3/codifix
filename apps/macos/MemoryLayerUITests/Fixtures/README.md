# Native acceptance fixtures

These synthetic HTTP responses are test inputs, never production screen data.
The loopback server has no database and never forwards a request to another
service. All write results exist only in its process memory.

From the repository root:

```sh
python3 apps/macos/MemoryLayerUITests/Fixtures/server.py --port 4261
```

In the app, explicitly add a **Local session** connection named **Acceptance
fixtures**, with service `http://127.0.0.1:4261`, project `synthetic-project`, and
optional repository path `/synthetic/repo`. Leave the real development
connection intact. The fixture service identifies itself as `fixture-dev`.

The server covers connection bootstrap, Memories, Query, replacement Review,
Automations and its approval queue, Bundles, and Access. The sample ZIP is an
intentional dummy recognized only by this fixture server; do not import it into
a real Memory Layer service. The displayed service token is an invalid dummy.

Use the `__scenario` endpoint to exercise `normal`, `loading`, `empty`,
`insufficient`, `auth`, `restricted`, and `failed` responses. For example:

```sh
curl -s http://127.0.0.1:4261/__scenario \
  -H 'Content-Type: application/json' -d '{"name":"empty"}'
```

Refresh the relevant screen after changing scenarios. Reset the replacement
proposal with `{"name":"normal","reset":true}` to test both approve and reject.
`GET /__requests` lists request methods and paths for acceptance evidence; it
does not log request bodies, cookies, or tokens. Stop the server when finished
and reconnect the app to the existing development service.
