# Aspen distribution notice bundle

This exercise uses fictional notice classes, not legal advice or interpretation
of actual licenses. The registry contains versioned component IDs, notice text
tokens, license IDs/classes/text and optional source-offer tokens. Tokens are
one to64 ASCII letters,digits,underscore,hyphen or dot; no whitespace/delimiters.
All commands operate in memory and never publish or fetch a URL.

`license ID CLASS TEXT` inserts/replaces metadata and prints `ok`. CLASS is
`notice` or `source`. `component ID LICENSE NOTICE` inserts/replaces a component
and prints `ok`; referenced license metadata may be supplied later. `offer ID
TOKEN` sets/replaces an offer for an existing component and prints `ok`; unknown
component is `invalid`. Replacing component metadata preserves its offer.

`ship ID` selects an existing component for distribution; `unship ID` deselects
it. Both print `ok`; ship of an unknown component is `invalid`, while unship of
an absent selection is harmless. Repeated selection does not duplicate output.
Only selected components contribute to the bundle and its required checks.

`build` validates all selected metadata first. Each selected component needs a
known license and its own attribution. Class `source` also requires an offer
for that component; class `notice` does not. Missing required metadata prints
`invalid-metadata` without changing the last successful bundle or generation.
Unused incomplete metadata does not block a build.

A successful changed bundle increments generation from0 and prints `built:N`.
If the canonical bundle is identical to the last successful one, print `unchanged:N`
without incrementing. The first build, including an empty selection, is generation1.
`show` prints `bundle:none` before any success; afterward it prints:
`bundle:N licenses:TEXT,... attributions:ID=NOTICE,... offers:ID=TOKEN,...`

License texts are deduplicated by exact text and sorted lexically. Attributions
retain EVERY selected component identity even when notice text is shared, sorted
by ID. Offers include exactly the selected source-class components, sorted by ID;
an irrelevant offer on a notice-class component is omitted. Different license IDs
with identical text may share one text entry without erasing their components'
individual obligations. Source versus notice class is checked per component.

Invalid syntax/tokens/classes and extra arguments print `invalid` without mutation.
The prototype bundled every registered component, collapsed attributions by
notice text and ignored missing metadata or source offers. That behavior is
superseded by this scoped, complete-build contract.
