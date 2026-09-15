# C7 journal, August 18 UTC

09:00: authoritative private A record is 10.20.0.10, TTL3600.
09:05: private resolver R-east caches that answer with expiry10:05.
09:30: operator lowers authoritative TTL to60; no address change yet.
09:40: operator changes private address to10.20.0.20.
09:42: R-east answers10.20.0.10 with remaining TTL1380; a fresh resolver R-west
answers10.20.0.20 with TTL60. R-east is not shown violating its cached TTL.
09:43: public resolver answers external relay192.0.2.40 as before; dashboard green.

All addresses are synthetic fixture addresses. The packet records no old-mirror
shutdown. New-mirror readiness passed from the west segment; no successful new
artifact read from the east segment is recorded by09:43.
