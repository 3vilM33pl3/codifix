# Yew search-panel experiment E3

The assignment unit is the authenticated account ID, not display name, browser
session, request ID or device. A fixed experiment ID and seed feed a pinned
bucket function with1000 buckets numbered0..999. At10% rollout, buckets0..99
receive treatment; at20%, buckets0..199 do. Record the actual rollout revision
with each exposure. An authorized percentage change expands the included range;
it does not require rebucketing existing account identities.

Anonymous or missing-account requests are excluded from experiment measurement
and receive the ordinary control interface. Do not substitute an arbitrary
session identifier or client-provided account claim as authenticated identity.
Display-name edits do not change account ID or assignment.

Exposure counts are not independent account counts. Outcome analysis must join
actual exposures, eligibility and account identity, retaining rollout revisions.
Assignment correctness alone does not establish a beneficial treatment effect.
