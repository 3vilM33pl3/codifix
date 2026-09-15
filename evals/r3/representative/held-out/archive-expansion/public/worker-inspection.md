# Import worker w7 inspection

The decoder accepts metadata sizes as allocation hints and streams output. The
caller can stop it, but w7 does not currently count actual bytes per entry or
across the archive. Increasing decoder buffer size changes throughput/memory
behavior, not accepted output limits. Metadata checks alone cannot enforce A5.

The visible import directory is scanned by retrieval. One scanner pass ran
while log-a was partial; the packet records an attempted scan, not a complete
inventory of what entered any retrieval result. A private staging directory is
available but not selected by w7's configuration.

A separate rejected test archive had an entry named ../outside.txt. Path safety
must be checked independently of sizes. The test does not prove u-53 contains
that entry or that a host path was overwritten.
