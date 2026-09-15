# Retired mirror checklist

The old checklist changed TTL at the same time as the address and treated a
successful public lookup as global convergence. It also recommended pointing
internal clients at the public relay if their view differed. C7 supersedes those
shortcuts; the split views are intentional access/routing boundaries.

Do not remove the old mirror merely because one fresh resolver has the new
answer. Avoid an unscoped restart of all builds just to flush caches: interrupted
work and established connections need their own reconciliation.
