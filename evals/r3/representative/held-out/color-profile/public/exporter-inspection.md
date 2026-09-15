# Preview exporter v14 inspection

The source asset carries Q. v14 copies its samples unchanged and writes a P tag
because the web preview expects P. Its checksum compares only raw sample bytes,
so it reports `unchanged image`. It records no conversion revision or source
profile. A second import path applies Q-to-P arithmetic unconditionally, even
to an already-P asset.

A proposed patch c33 strips all profile metadata. Another lowers monitor
brightness to make the preview resemble the source. Neither implements C21's
sample-and-profile conversion or idempotence. No actual monitor measurements
or release approval accompany these proposals.
