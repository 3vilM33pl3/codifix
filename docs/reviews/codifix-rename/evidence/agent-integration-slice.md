# Agent integration and runtime checkpoint

The core mapping is committed in `7f3eeabc` and `7a657b06`. Runtime paths,
environment names and preserved authentication identities are in `c81fcac1`;
`83a98feb` aligns required-database CI variables and development process detection.

Focused config/platform/record/watch tests: 103 passed. The first broad CLI
runtime run passed 105 and failed 12, including cascading poisoned test locks.
The primary gaps were unfinished plugin/bootstrap and packaging names, plus
historical Markdown links to moved crates. This is an intermediate checkpoint,
not acceptance of the whole rename.

The agent slice renames source skills, plugin/marketplace registrations, MCP
tool/prompt names and resource scheme. Semantic memory_id/memory_type and
/v1/memory routes remain. CODIFIX_BIN retains the original strings.Fields
invocation behavior. Go helper tests and the plugin contract pass.

`.agents/codifix-command.json` deliberately still selects the installed
`memory --area workshop` during implementation. A query through the renamed
helper succeeded with both MEMCTL_BIN and CODIFIX_BIN unset. This is an explicit
transition selection, never an error fallback. Switch this file to codifix only
after installed Workshop cutover, and verify before finishing the plan.

New names do not imply that deployed clients, global skills or native packages
have been replaced. Those remain tracked cutover work.
