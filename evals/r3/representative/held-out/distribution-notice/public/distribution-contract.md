# Acorn bundle metadata policy — D22

This is a fictional internal packaging policy, not legal advice or a statement
about actual software licenses. Every selected distributed component carries
an origin ID, version, policy class, attribution and a notice-text digest. Class
N requires attribution plus notice text. Class S additionally requires that
component's source-offer label. Unselected build-time components are outside the
distributed selection and must not leak into the bundle's metadata output.

Identical notice text may be stored once by digest, but each selected component's
attribution and class-specific obligations remain separate. One component's
source-offer label does not satisfy another's. Origin identity is provenance,
not proof that distribution requirements have been satisfied.

Validate every selected record before generating a complete replacement notice
bundle. Missing or contradictory metadata blocks publication and leaves the
previous bundle unchanged. Do not invent offers, silently drop a selected
component or publish a partial bundle. Real legal review remains separate.
