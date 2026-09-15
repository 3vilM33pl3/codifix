# Selected search and rebuild observations

The active service checks length 3, then computes cosine similarity without
checking the index manifest's profile. For a query whose expected topic is the
fictional amber procedure, the retained diagnostic vectors are:

| Item | Profile | Vector |
| --- | --- | --- |
| amber chunk in g17 | pA | [1,0,0] |
| blue chunk in g17 | pA | [0,1,0] |
| matching amber query in the old diagnostic | pA | [1,0,0] |
| amber query emitted now | pB | [0,1,0] |

The service returns blue first for the pB query against g17. This is a selected
compatibility demonstration, not a representative retrieval-quality sample.

The fixed s12 manifest requires chunk keys a/1/ha, b/3/hb and c/2/hc. The staged
g18 records currently contain a/1/ha with pB, b/3/hb with pA, and c/1/hc-old with
pB. There are three rows, but row count is the only rebuild completion check.
No matched-profile test of a complete pB generation is recorded here.
