# Maple path access model

This program is a deterministic path-policy model. It does not open host files,
follow real symlinks or establish a production filesystem sandbox. Its root is
fixed at `/work`. Paths are absolute ASCII, at most256 bytes; components use
letters, digits, dot, underscore or hyphen. Collapse repeated separators and
single-dot components. Reject any double-dot component rather than interpreting
parent traversal; reject percent escapes, backslashes, relative and non-ASCII
paths. Root `/` is syntactically valid but outside this model's authorized root.

`grant read|write PATH` inserts one operation-specific subtree grant and prints
`ok`; `revoke read|write PATH` removes only that exact normalized grant, also
printing `ok` when absent. Grants must be inside `/work`. A grant at `/work/team`
covers that path and descendants, not `/work/teamster`. Read does not imply write
and write does not imply read. Repeated grants do not multiply authority.

`check read|write PATH` prints `allow:RESOLVED` or `deny`. Both the normalized
requested path AND final resolved path must be inside `/work` and covered for
the requested operation. Invalid check paths are denied; invalid mutations print
`invalid` and preserve existing policy. Syntax errors and extra tokens print
`invalid`. No grant means deny.

`link SOURCE TARGET` defines/replaces an in-memory alias; SOURCE must be within
`/work`, TARGET may be any syntactically valid absolute path. A target outside
the root can be recorded but access through it is denied. Apply the longest
component-prefix alias to the current path, append its unmatched suffix, and
repeat until no alias matches. Normalize the resulting path each time. Allow
at most eight rewrites; cycles or a ninth rewrite deny. `unlink SOURCE` removes
only that exact normalized alias. Both successful mutations print `ok`.

An alias is not authority: grant checks still apply to both requested and final
paths. Replacement/unlink affects the next check immediately. Alias resolution
is based on the current policy state, not a cached earlier allow decision.

The archived prototype compared string prefixes and treated any operation grant
as sufficient. It also ignored aliases. Those shortcuts are superseded here.
Preserve the command interface and existing public tests while implementing the
component, operation and resolution rules. No host path should be inspected.
