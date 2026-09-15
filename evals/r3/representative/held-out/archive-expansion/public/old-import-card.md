# Retired small-upload shortcut

The prototype checked only compressed upload bytes and metadata size sums. It
treated partner-signed archives as trusted and wrote files directly where the
watcher would find them. A5 supersedes that procedure.

Do not accept an oversized entry because the complete archive is still below
its total limit. Do not disable validation to finish an urgent support import.
Signature validity and resource/path validation answer different questions.
