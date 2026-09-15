# Archive recovery contract B3

Daily backup snapshots are retained for 35 days. A snapshot from August 30 may
contain a10 and a11 even after active-store deletion. The backup system cannot
remove one attachment from an existing immutable snapshot. Restores must replay
current expiry and hold state before ordinary read access is enabled; restoring
an older snapshot does not revive an expired payload's search eligibility.

Active holds also cover backup destruction for snapshots containing covered
case material. Those snapshots remain until case authority resolves the hold;
the ordinary 35-day schedule is not an unconditional deadline for every snapshot.
The inventory needed to enumerate all H4 snapshots is still being compiled.

Recipient exports are outside this backup operator's recall control. The packet
does not inventory all exported copies or provide their recipients' deletion
receipts. Do not represent active-store cleanup as global erasure.
