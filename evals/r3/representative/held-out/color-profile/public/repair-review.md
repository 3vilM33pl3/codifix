# C21 repair review — pending

Validate known source profile and existing conversion provenance. For Q input,
perform the specified sample transform and atomically record P plus source
profile/digest/revision. For already-P input, preserve samples; reject an unknown
profile for automatic conversion. Hash interpreted image data, including profile
metadata, separately from any raw-sample checksum.

Test endpoints 0/255, the midtone example, already-converted input, missing profile
and same bytes under different tags. Review replacements without erasing source
history. Do not strip metadata, guess d's profile, adjust real monitors or change
display algorithms under this repair. No live asset or device is modified.
