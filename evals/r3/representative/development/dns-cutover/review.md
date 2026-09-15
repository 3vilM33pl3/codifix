# Development knowledge family: DNS cutover — DRAFT

Five fictional sources, four questions. Split DNS views and separate resolver/
connection lifetimes require joining evidence instead of reading a green status.
This mechanism stays in development; addresses/name are synthetic fixtures.

1. R-east's old answer lives to10:05;1380seconds remain at09:42. The new TTL cannot
   rewrite that cache. The lead time was10minutes instead of the old hour.
2. The public relay is not the private mirror. Fix VPN resolver selection and
   verify private reads, rather than bypassing the intended split view.
3. Other caches may last to about10:30, and connections can outlive DNS answers.
   e2's establishment time is unknown. Verify all segments and drain connections
   before retirement; neither10:05 nor10:30 guarantees global completion.
4. The packet cannot establish an exact switch time, outage duration or complete
   cutover. It has one west read and lacks east/VPN completion evidence.

Accept equivalent calculations, timestamps with spacing and precise distinctions.
Require supported citations and blinded semantic review. All labels are draft;
this sheet/private rubrics remain outside the answering workspace.
