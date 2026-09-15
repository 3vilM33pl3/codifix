# Retired installation shortcut

The former process checked HTTPS, filename and a successful extraction, then
installed the latest alias. This predates signed channel/platform manifests and
must not be followed for revision 7 releases. Signature failure is not permission
to disable verification temporarily.

The public notes say "4.2.0 stable is ready" but do not include an artifact digest.
They describe release intent; they do not identify which same-named binary a
particular cache served. Keep the note for context, not as an installation grant.
