# M4 native deck visual review

Reviewed 17 September 2026. This review covers the first rendered revision and
the final repair checks of the 18-slide M4 deck. Every slide was opened
individually with `view_image`; a montage was not used as a substitute. No slide
objects or content were edited during this review.

**Final visual status: passed.** Native revision `XPNgiWlMLaNeJg` has 18 slides.
All blocking visual findings are resolved. The detailed first-pass findings
below are retained as review history, not outstanding repair requests.

## Evidence and scope

- Rendered evidence: `/tmp/m4-deck-review-20260917/renders/v1/slide-01.png`
  through `slide-18.png`.
- Native object evidence: `/tmp/m4-deck-review-20260917/edited-v1.json`.
- Content and notes: `/tmp/m4-deck-review-20260917/requests/content.json`.
- Scope authority: [approved-plan.md](approved-plan.md).
- This is a visual and wording audit. Financial, implementation, competitor and
  source claims require the separate structural and factual checks.

## Overall result

The v1 render has no visible clipping, accidental text overlap, missing glyphs or
out-of-canvas content across all 18 slides. The native canvas remains 960 by 540
points, equivalent to 16:9. Arial, the pale Codifix palette, gradient background
variation and bottom-right brand mark are consistent. Body copy is substantially
more readable than in the original deck.

The first five slides explain the problem, independent control and knowledge
reuse without requiring ATProto expertise. The main diagram on slide 4 works in
plain language; the protocol reference is supporting information. Slide 5's
numbered sequence runs correctly from 1 to 2 to 3, then down to 4 and left through
5 to 6. Its wording needs one causal link: recording a correction before its
owner reviews it.

## Consolidated repair pass

### 1. Complete the correction sequence on slide 5

Step 4 currently says the responder checks the answer and acts. Step 5 then
approves corrected knowledge, without an earlier action creating or recording
that correction. Add that action to step 4. A short option that should fit is:

> Responder checks, acts,\
> and records corrections.

The existing step-4 body, `p5_i19`, also wraps “answer” onto a line by itself in
the render. The shorter wording resolves both the story gap and the weak wrap.
Step 3 is titled “Cited answer”, but its body does not say what is cited. If the
verified workflow supports the wording, use “Source team returns an answer with
source citations” while retaining the sharing boundary in the surrounding
explanation or notes.

### 2. Make employee AI units explicit on slide 13

The line “£5k allowance / month; £2.5k expected spend per employee” can be read as
an organisation-wide allowance paired with an employee-level expected spend.
Both amounts need the same explicit denominator and period. Suggested wording,
subject to the M2 fact check:

> Employee AI, per employee per month:\
> £5k allowance; £2.5k expected spend.

The relevant native object is `m4_s13_text_1`. Keep this as a staffing/financial
assumption, separate from evidence of employee throughput.

### 3. Explain the funding horizon and buffer on slide 18

The table heading says “24-month funding objective”, includes “M25–27 operating
buffer”, and the lower text says “the funding objective ends at month 24”. The
visible slide does not explain how these relate. If the source model includes a
buffer after the 24-month plan, label it explicitly as a 24-month plan plus a
three-month operating buffer. Preserve the distinction between spending, cash
headroom and funding requirement. The repair must follow the verified M2
definition rather than infer a new funding assumption.

### 4. Name the retention measure precisely on slide 17

The displayed calculation includes expansion as well as churn. “Annual
retention” alone can imply customer retention. Prefer “Net revenue retention
assumption” or the exact corresponding model label. The arithmetic shown,
`(1 - 5%) × (1 + 45%) = 137.75%`, is internally consistent. This wording check does
not independently validate either assumption.

### 5. Align visible baselines in slide 11's pilot stages

The third card's “Analysis” heading and supporting line sit visibly lower than
their peers. The rightmost “2 weeks” also appears slightly lower. Native
coordinates alone do not establish visual alignment: the reused and newly
created objects have different rendering behaviour or internal padding.
Compare and align the rendered baselines across all three cards.

- Left heading: `p11_i8`.
- Middle heading: `p11_i13`.
- Right heading: `m4_s11_text_1`.
- Right body: `m4_s11_text_2`.
- Duration labels include `p11_i7`, `p11_i11` and `p11_i15`.

### 6. Straighten horizontal rules and connectors

The newly created horizontal connectors and rules use a non-zero vertical
transform. For example, slide 6's `m4_s6_edge_1` and `m4_s6_edge_2` have
`scaleY: 0.0042` on a 3,000,000 EMU object, producing approximately a one-point
vertical drift across each rule. The render shows a slight slope. The same
pattern occurs in the horizontal `m4_s3_edge_*`, `m4_s4_edge_*` and
`m4_s5_edge_*` connectors.

Use exactly horizontal or vertical endpoints where the intended direction is
horizontal or vertical. Preserve the existing arrowheads and the correct
workflow order. Slide 5's short downward arrow is correct; it may be lengthened
slightly if spacing allows, but this is optional.

## Slide-by-slide inspection

| Slide | Visual and readability result | Wording or evidence concern |
| --- | --- | --- |
| 1 | Clear hierarchy, generous spacing and readable cover. No clipping. | The starting workflow appears visibly on the cover. No protocol knowledge is needed. |
| 2 | Clear application-to-platform handover. The arrow points correctly. All three consequences fit. | Fictional/illustrative status is explicit. |
| 3 | Three stages fit and progression is clear. | The required Omarchy analogy is named without a definition. A short plain-language gloss would help unfamiliar readers, but the accompanying “environment for agentic work” explanation makes the slide understandable without it. Preserve the approved analogy. |
| 4 | Independent units and bidirectional exchange are legible. Correct arrow directions. | Main text works without protocol expertise. The ATProto/Memory distinction is supporting copy rather than a prerequisite to understanding the diagram. |
| 5 | Correct numbered snake sequence. No overlap. Step 4 has a weak one-word wrap. | Record the correction before origin review; make source citations explicit if supported. See repair 1. |
| 6 | Flat measurement rows are readable. Small rule slope is visible. | Customer outcomes remain proposed measurements. Capacity versus realised cash savings is clearly distinguished. |
| 7 | Three-column evidence table is readable. Supporting limitation is prominent. | “Implemented software” is kept separate from customer and quality proof. Parent factual review should verify every capability/evidence row. |
| 8 | Buyer and expansion progression are easy to scan. | Buyer and expansion hypotheses are explicit. The line break in “technology-operations leadership” is slightly awkward; “technology operations leadership” would read more naturally. |
| 9 | Plain comparison copy and a single highlighted focus statement work well. | Broad competitor statements still need the planned primary-source check. The slide does not claim competitors lack all governance. |
| 10 | Price and included deployments are clear. Separate commercial components remain readable. | “Initial Base” in the qualification is an internal model name not yet defined for the reader. “The base forecast” would be clearer if equivalent. |
| 11 | All content fits. Third-stage text baselines differ from the other cards. | Eight-week sequence is clear: two baseline weeks, four assisted-operation weeks and two analysis weeks. See repair 5. |
| 12 | Four evidence gates are readable with consistent numbered hierarchy. | No unconditional date or customer count is implied. |
| 13 | Founder and planned organisation are clearly separated. No overflow. | Employee AI denominator/period is ambiguous. The planned assurance programme and incomplete certifications are visibly qualified. See repair 2. |
| 14 | Capital proof points form a clear closing composition. | “Capital should demonstrate” is slightly unnatural; “The next proof points” or “What the funding should establish” would be simpler. No fixed monetary ask appears. |
| 15 | Architecture and trust boundary appendix is legible. Good separation of the two responsibilities. | Copy clearly says recipients can retain copies, signatures do not prove truth, and Spaces/delegation remain future work. |
| 16 | Two evidence counts are clear, with limitations in readable text. | No quality, customer ROI, perfect recall or token-saving advantage is implied. Parent should verify the dated R3 counts and run validity. |
| 17 | Financial table is readable, with units and forecast qualifications visible. | Use the exact net revenue retention label. Fractional accounts are explicitly expected values rather than signed customers. See repair 4. |
| 18 | Comparison table fits and keeps GBP/USD alternatives separate. | Clarify the 24-month horizon versus M25–27 buffer. The copy already distinguishes AI headroom from expense and the scenarios from a fundraising ask. See repair 3. |

## Optional polish

- Internal source codes such as M1, M2, F1 and R3 are useful to maintainers but are
  not explanatory to an investor. The footers could use plain source labels,
  retaining full internal references and links in speaker notes.
- Most titles end without punctuation, while a few short titles retain terminal
  periods. Normalising those endings would improve consistency without changing
  meaning.
- Retain the material implementation and commercial qualifications. Avoid adding
  further generic warnings to the already substantial footer text.

## Re-review of the first final candidate

All 18 images in `/tmp/m4-deck-review-20260917/renders/final/` were inspected
individually after the consolidated copy and alignment repair. The wording
repairs on slides 5, 13, 17 and 18 pass. The three pilot-stage text baselines on
slide 11 now align visibly. No new text clipping, missing glyphs or colour/font
regressions appeared.

This candidate did not pass visual sign-off because the connector repair changed
the rendered line lengths on slides 2–6:

- Slide 2: the handover arrow extends into the platform card and crosses text.
- Slide 3: both progression arrows extend through the destination cards.
- Slide 4: exchange arrows extend into the unit cards, including a text row.
- Slide 5: horizontal arrows cross step text, and the vertical arrow runs
  through step 4 down to the slide number/footer.
- Slide 6: full-width row rules shrink to roughly one-quarter width.

The implementation owner identified the native cause: Google normalises newly
created line objects to an intrinsic 3,000,000 EMU size. Reusing creation-time
scales while setting a zero orthogonal axis lost the intended lengths. The
second repair computed each intended span against the actual native size while
retaining exact horizontal/vertical geometry. The corrected slides 2–6 received
the targeted re-review below. The first candidate's other 13 slides passed the
final regression check.

The structural/factual owner records source, claim, sharing and native
editability checks separately.

## Final visual sign-off

Evidence: `/tmp/m4-deck-review-20260917/verified-native.json`, revision
`XPNgiWlMLaNeJg`, and `/tmp/m4-deck-review-20260917/renders/verified/`.

Slides 2, 3, 4, 5 and 6 were opened individually after the second line-geometry
repair. Every connector now has the intended span and direction. Arrows stay in
the gaps between cards. Slide 5's vertical arrow stays between steps 3 and 4,
and its numbered workflow proceeds correctly through all six steps. Slide 6's
two horizontal rules span the complete measurement rows. No lines cross text.

The verified PNGs for slides 1 and 7–18 have SHA-256 hashes identical to their
already-inspected final-candidate images. All 18 slides are therefore covered
by individual visual inspection and the targeted regression check.

- No visible clipping, unintended overlap, missing glyphs or out-of-canvas
  content remains.
- Slide 5 explicitly records a correction before origin review and describes
  source citations.
- Slide 11's pilot-stage headings and body baselines align.
- Slide 13 states both employee AI amounts per person, per month.
- Slide 17 names net revenue retention.
- Slide 18 explains funding through month 24 and retained AI headroom plus
  months 25–27 operating outflows.
- The first five slides remain understandable without protocol expertise.
- Arial, pale Codifix colours, 16:9 proportions and branding remain consistent.

No further visual repairs are required for this revision. The optional wording
polish listed above does not prevent use of the deck.
