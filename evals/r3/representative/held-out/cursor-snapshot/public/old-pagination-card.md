# Retired listing card

The original design assumed severity values were unique, used only that field
as a cursor and fetched the latest table on every page. It also recommended
restarting silently if a cursor expired. L4 replaces those assumptions.

Project choice is part of listing identity, not an optional UI-only filter.
Do not accept a Juniper cursor on an Iris request or apply authorization after
the limit. Retain this card to explain r18, not as permission to reuse its cursor.
