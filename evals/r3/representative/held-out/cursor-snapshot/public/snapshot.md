# Snapshot S20, project Iris

The captured eligible rows in order are:
incident i10 severity5; incident i11 severity5; incident i12 severity5;
incident i13 severity4; incident i14 severity2.
Project Juniper has incident j01 severity9 in the underlying table but it is not
eligible for the Iris listing. A limit applied to the table before the project
filter can omit legitimate Iris candidates.

The first page from S20 is i10,i11. The remaining ordered rows are available in
the captured snapshot above. These are fixture data for the ordered-read contract,
not live incident records.

After page one, live incident i09 is inserted into Iris at severity6 and i14's
severity is edited to7. Neither change modifies S20. A new snapshot would put
i14 and i09 ahead of the severity5 rows. S20's original count remains five.
