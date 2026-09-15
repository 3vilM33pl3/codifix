# Retired client experiment shortcut

The early prototype chose browser session ID as the bucket input and reran
assignment whenever a display name changed. It counted each exposure row as an
independent trial. E3 supersedes those practices for account-level evaluation.

The same seed makes a function reproducible for the same input; it does not make
different sessions of one account equivalent. Do not override authenticated
identity from a request body or promote anonymous fallback into measured exposure.
