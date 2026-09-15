# Moss sparse sensor transforms

This small fictional Rust service keeps named signed-integer matrices for sensor
channel transforms. It is an in-memory maintenance fixture, not a physical
calibration system. Five modules separate input, sparse data, rendering and state.

## Commands

Each input line emits one output line. Names contain 1–12 lowercase ASCII letters.
Shapes have 1–8 rows and columns. Coordinates are zero-based.

- `put NAME ROWS COLS r,c,value;r,c,value` stores a matrix; `-` means no entries.
- `show NAME` renders `ROWSxCOLS:r,c,value;r,c,value` in row/column order, or
  `ROWSxCOLS:-` for all zeros. Unknown names return `missing`.
- `transpose DEST SOURCE` creates the transposed source under DEST.
- `mul DEST LEFT RIGHT` creates LEFT×RIGHT under DEST.
- `revision` prints the count of successful mutations, initially `0`.

Success is `ok`; malformed input is `invalid`; invalid coordinates/shapes or
incompatible multiplication are `shape`; missing operands are `missing`; checked
arithmetic failure is `overflow`. Errors do not change matrices or revision.

## Stage 1 — canonical sparse ingestion

Validate every coordinate before accepting a whole put. Merge duplicate
coordinates by checked i128 addition of i64 values, then require each final
value to fit i64. Remove zero totals and sort coordinates. Missing entries mean
zero, not missing observations. Keep the explicitly supplied rectangular shape.
At most 128 input entries are accepted. No partial replacement on invalid input.

## Stage 2 — shape-preserving transpose

Swap both dimensions and each coordinate; canonical order follows the new row
and column. Preserve signed values and all-zero rectangular shapes. Source and
destination can be identical: read a complete source snapshot before replacement.
Missing source leaves the previous destination and revision unchanged.

## Stage 3 — atomic composition

Multiply only when LEFT.cols equals RIGHT.rows, producing LEFT.rows×RIGHT.cols.
Compute dot products with checked i128 multiplication/addition, then require each
final output value to fit i64; remove zeros and order output canonically. A
checked intermediate overflow is an error even if later cancellation could have
made a mathematical total fit. Snapshot both operands before writing so DEST may
alias either. Every successful put/transpose/mul increments revision once;
rejected work leaves prior destination, operands and revision intact.

Do not add dependencies or floating-point/embedding algorithms. Public tests are
ordinary examples; hidden tests cover dimensions, signed cancellation, aliasing,
overflow and failure-state preservation across cumulative maintenance steps.
