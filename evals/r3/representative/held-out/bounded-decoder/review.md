# Held-out coding family: bounded run decoder — DRAFT

Five modules, two public tests and ten hidden probes across three cumulative
stages. The fictional RLE format and explicit limits precede all hidden probes.

1. Split count/value pairs, binary values, malformed count and truncated/empty EOF.
2. Actual output bounds, exact boundaries, cumulative input cap and huge run counts.
3. Private staging, unchanged prior result on failure, explicit abandon/restart,
   invalid-command preservation and single commit generation.

The reference/cumulative sequence must pass; starter and no-op controls must fail.
No actual decompression library, filesystem or durable crash recovery is involved.
The related archive-expansion knowledge family stays in this held-out split.
Private labels remain draft pending review, never model-calibration inputs.
