# Chrono 0.4 compatibility adapter

First-party code uses the maintained `sad-chrono` implementation. This adapter
lets dependencies with a `chrono` 0.4 constraint use the same types and feature
set. It contains no upstream Chrono implementation. The registry lock retains
`sad-chrono` and its dependency graph for auditing.

Use the adapter only for the features listed in its manifest. Remove it when
all dependencies support the maintained implementation directly. The migration
keeps the existing serialized timestamps, SQL timestamp types, and time formatting.
