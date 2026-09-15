# Writer v10 inspection

v10 writes a temporary file, renames it over `report.bin`, and returns `durable`
when rename succeeds. It never flushes the file or parent directory. On any later
journal error it reports `rolled_back`, without checking the selected name.

A proposed patch p26 flushes only the temporary file before rename. Another
increases a post-rename sleep. Neither confirms the directory barrier required
by P17. No patch is released. Sleeping is not a modeled persistence operation.
