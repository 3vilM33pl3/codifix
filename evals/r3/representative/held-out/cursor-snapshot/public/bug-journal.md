# Listing handover incident I-7

Build r18 returned i10,i11 on the first Iris page, then encoded only severity5
in the cursor. The next query used severity<5, yielding i13,i14 and omitting i12.
The query logs show no transport error. A green HTTP status did not mean complete
enumeration of tied rows.

A separate prototype used live offset2 for the next page. After the insertion
and severity edit, the live order changed to i14,i09,i10,i11,i12,i13. Offset2
would revisit i10,i11, not continue S20. That is a demonstrated counterexample
to offset pagination over this mutable ordering, not a measured production count.

The client stopped after the last returned page. The packet contains no full
cross-project export or independent total of all incidents created by customers.
