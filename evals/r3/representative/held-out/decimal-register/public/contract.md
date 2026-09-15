# Nettle measurement register

The register stores signed fictional measurement units at exactly three decimal
places, represented by an i64 count of milliunits. It starts at zero. This is an
in-memory numeric model, not money, pricing or persistence. Every successful
mutation prints `value:` followed by a normalized decimal with exactly three
fractional digits. Zero is `0.000`, never negative zero.

`set VALUE` replaces the amount; `add VALUE` performs exact checked addition.
VALUE is at most40 ASCII bytes: optional leading minus, one or more digits,
optionally a decimal point and one to three fractional digits. Leading zeros
are allowed. No plus sign, exponent notation, trailing decimal point, missing
integer part, NaN, infinity, whitespace inside a value or excess fractional
precision is allowed. Pad short fractions with zeros; do not round parsing.
Reject values outside i64 milliunit range rather than saturating them.

`scale NUM DEN` multiplies the current amount by signed integer NUM and divides
by positive integer DEN. NUM lies in[-1000000,1000000]; DEN in[1,1000000]. Round
the exact rational result to nearest milliunit, with half ties away from zero.
Use sufficiently wide intermediates and reject an out-of-range final milliunit
value. Round once after multiplication/division, not by first rounding a factor
or converting through binary floating point. Scaling by zero yields zero.

`show` prints the normalized current value without mutation. Syntax errors,
invalid inputs and arithmetic overflow print `invalid` and preserve the previous
value. Extra command tokens are invalid. The parser and arithmetic must handle
both i64 extrema, including formatting the minimum without signed abs overflow.

The archived prototype used f64 for parsing and scaling and saturating addition.
Those shortcuts are superseded by this exact contract. Preserve public command
behavior and tests; choose any implementation obeying the numeric semantics.
