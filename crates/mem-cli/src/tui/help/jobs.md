# Jobs

## Purpose

Inspect durable work for the current project. Curation commits before enrichment
finishes. Job details contain sanitized reasons, dependencies and attempt history.

## Layout

Project queue statistics, bounded stage list and selected attempt details.
Retry confirmation stays visible above the list, including its stable request ID.

## Controls

- `r`: refresh the first page. `PgDn`: next page. `s`: cycle status filter.
- `j/k` or arrows: select. `Enter`: inspect. `PgUp`: scroll details up.
- `d`: scroll details down. `R`: review retry for a failed or blocked stage.
- `y`: confirm the exact retry. `n` or Escape: cancel.

## Workflows

Retry retains original inputs, completed stages and history. The service enforces
current permissions, read-only mode and execution policy. It does not transfer
execution authority to the operator. An uncertain reply retains its request ID;
retrying the same confirmation is safe, but provider calls may incur repeat billing.
