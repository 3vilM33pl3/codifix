# Held-out coding family: sparse sensor transforms — DRAFT

Five public modules, two ordinary public tests and three cumulative maintenance
steps. The public source contract precedes ten hidden black-box probes.

1. Canonical ingestion: duplicate sums/cancellation, invalid coordinates, wide
   accumulation/final overflow and explicit zero shape.
2. Rectangular transpose: swapped dimensions, sorted coordinates, self-alias and
   missing-source preservation, plus a twice-transposed round trip.
3. Composition: rectangular dot products, signed/wide cancellation, destination
   aliasing and unchanged output/revision on missing, shape or overflow failures.

The private engine is a fixture reference, not an agent-visible solution. Probe
sets must pass cumulatively; starter and no-op controls must fail each stage.
This is in-memory integer processing, not actual sensor accuracy, filesystem
durability, embedding quality or a production algorithm change. Labels remain
draft and held out from model calibration pending Olivier's review.
