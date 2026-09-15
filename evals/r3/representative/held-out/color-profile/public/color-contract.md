# Violet preview colors — C21

This fictional image pipeline stores channel samples together with a named color
profile. In the one-channel test model, profile P maps a stored value x to display
intensity x/255; profile Q maps it to (x/255)^2. The same numeric sample under
different profiles can have different meaning. These are invented transforms,
not an implementation of real ICC or standard color spaces.

C21 preserves intended display intensity when converting profiles. To convert
Q to P, compute y=round(255*(x/255)^2) with nearest integer rounding. Update both
samples and destination profile as one operation, retain source profile/digest
and conversion revision, and never apply that conversion again to already-P data.
Changing only a profile label is assignment/reinterpretation, not conversion.
Unknown/missing source profile is ineligible for automatic conversion.

The complete-image digest covers samples and profile metadata. A raw-sample hash
alone does not establish the same interpreted image. An actual display-device
calibration or perceptual-quality claim needs evidence beyond this toy transform.
