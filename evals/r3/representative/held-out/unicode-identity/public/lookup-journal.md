# Lookup inspection

Client build v2 performs ASCII lowercase only. A composed `Café` query selects
account11; a decomposed `Café` query selects account12. Both HTTP requests return
success, but the selected stable IDs and grants differ. The client does not
compose the combining sequence and the stored index is mixed-revision.

A draft repair suggested normalizing the query and returning the first matching
row by creation time. It has not shipped. That proposal would hide the collision
rather than reconcile the two owners under U4. The packet contains no evidence
that either account's grants have been misused or that one account is fraudulent.

Only these three existing records are inventoried. No complete historical label
or script-confusable scan has been performed.
