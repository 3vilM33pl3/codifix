# Held-out coding family: route precedence — DRAFT

Five public modules, two ordinary tests and ten private probes across three
cumulative stages. The existing helpers define a strict bounded grammar; this
is not actual HTTP routing or an authorization implementation.

1. Literal/parameter/rest/end specificity, independent of ID and with winning
   captures only. Lexicographic priority is not a count of literal segments.
2. Explicit matching HEAD before GET fallback, no other-method fallback and
   correct actual method/captures.
3. Capture-name-independent pattern conflicts, atomic replacement, exact retry
   identity and removal/reuse. The table contract also bounds capacity at 32.

Reference and cumulative probes must pass; starter and no-op controls must fail
each stage. Private solutions stay out of agent workspaces. All labels remain
draft and held out from development calibration until reviewed by Olivier.
