# Selected fault injections

The old selected report is g7; the proposed replacement is g8. Each row is a
separate synthetic experiment starting with g7, not successive stages of one run.

| Run | Last confirmed operations | Live/recovery evidence |
| --- | --- | --- |
| a | v10 writes and renames g8; no flush | live reader sees g8; power-loss recovery selects g7 |
| b | write, file flush, rename g8; directory flush returns error | live name is g8; recovery outcome not yet observed |
| c | write, file flush, rename, directory flush all confirmed | injected recovery selects intact g8 |
| d | temporary write fails before rename | selected name remains g7 |

The intent journal retains operation IDs and these confirmations. No result
authorizes deleting g7 history or assuming run b matches a. These observations
do not test actual disk firmware, all filesystem implementations or remote replicas.
