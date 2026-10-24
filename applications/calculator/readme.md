# Calculator

A native AppCore / ViewKit calculator using the existing omochi controls, colors,
typography, continuous corners and motion. The app icon is reused from the user's
exported desktop mock. Backspace has no registered VK Symbol yet, so the keypad
uses a labelled **Back** button in the toolbar.

The default window is 360 × 540 (47% of the previous 600 × 690 area). Standard
mode has four columns and five rows, with a double-width zero. Digits, operations
and equals use the existing omochi button palettes. Copying, clearing and startup
preferences are available in **More**; right-click the result to copy it. Scientific
mode adds six common functions above the keypad; further functions are in More.

## Modes

- **Standard**: editable expressions, parentheses, four arithmetic operations,
  percentage, current-operand sign switching, reciprocal and the previous answer (Ans).
  `200+10%` and `200-10%` use the relative percentage (220 and 180); multiplication
  and standalone percentages use the ordinary fraction (`200*10%` is 20).
- **Scientific**: standard operations plus right-associative powers, square roots,
  square, factorial, sin/cos/tan, inverse trigonometry, ln/log/exp, abs, pi and e.
  Degrees/radians are explicit. Factorial accepts integers from 0 to 33.
- **Convert**: live conversion of length, mass, temperature, area, volume, time,
  speed, data, energy and pressure. US/Imperial volumes and SI/IEC data units are
  separate. Temperatures below absolute zero are rejected.
- **Programmer**: BIN/OCT/DEC/HEX, 8/16/32/64-bit words, signed two's-complement
  interpretation, AND/OR/XOR/NOT, shifts, integer arithmetic and remainder.
  Arithmetic wraps at the selected word size; shifts outside that size are
  rejected. Signed division truncates toward zero and right shift is arithmetic.
  Literals use the selected base, or an explicit `0b`/`0o`/`0x` prefix. Incomplete
  expressions must be corrected before switching base/word size.

Each mode keeps its draft and previous answer during the session. History opens
in a separate native window, so the keypad remains available. Up to 200
calculations persist with their mode, angle/base/word settings and conversion
units. Select a row to restore it; right-click a row to copy its expression/result
or delete it. Clearing all history requires confirmation. Stored Ans references
are materialized as exact literals so replay does not depend on a later answer.
Damaged history is reported and kept untouched until an explicit clear.

## Precision and input

Decimal literals and rational arithmetic use checked, reduced i128 fractions.
For example, `0.1+0.2` is exactly `0.3`, and the rational result of `1/3` remains
exact internally when continued with Ans. The main display marks rounded values
with `≈`; terminating decimal rendering is exact up to 38 fractional places.
Overflow in standard calculations is an explicit error. Scientific calculations
can fall back to finite f64 values outside the rational range; transcendental
functions also use f64. This is not an arbitrary-precision or symbolic calculator.
Non-real results, division by zero and unsupported input never become NaN/Inf
success results. Input length, token count and nesting are bounded.

The input is the standard ViewKit TextField, including selection, cursor editing,
IME and clipboard editing. Keypad insertion uses its normal text-input event
rather than appending past the caret. Enter or `=` calculates, Escape clears,
Ctrl/Super+1–4 selects a mode, Ctrl/Super+H toggles history, and Ctrl/Super+Shift+C
copies the result. The **±** key changes the operand at the caret/selection. **C** clears that entry
and becomes **AC**; AC and Escape clear the expression and repeat operation.
Scientific function keys wrap the current operand; unfinished parentheses are
closed on equals. Repeated equals reapplies the final operation, including
relative percentages. Entering a new number after equals and pressing equals
applies that operation to the new number; an operator continues with exact Ans.
Ctrl/Super+Z and Shift+Z undo/redo input edits. The displayed result uses digit
grouping and measured font fitting while copied values remain ungrouped.

Both conversion fields accept native text editing. The keypad edits the selected
field. Swap exchanges units and values, preserving the physical quantity.

## Startup and storage

**Use at Startup** explicitly saves the current mode; merely switching mode does
not change the preference. The preference is private per-user application data
under `/libraries/applications/org.mochios.calculator/users/<USER>/start-mode` on
mochiOS. Temporary/backup publication retains the previous setting on write
failure. A crashed writer can leave its `.lock` directory; stale-lock cleanup is
not automatic yet. Invalid settings report an error and fall back to Standard. History uses the
sibling `history-v1` file with the same bounded, locked publication mechanism.

Desktop previews use `MOCHIOS_PREVIEW_CONFIG_ROOT/calculator/start-mode` and the
existing shared preview clipboard slot. Independent previews initialize a shared
config root under `out/preview/config`; direct Linux launches outside the preview
use `~/.local/share/mochios/calculator` for settings. Native copying uses AppCore's
capability-checked clipboard API. A complete `mmake ui` session remains disposable.

`mmake ui` includes the application in the launcher catalogue.
`mmake preview-calculator` starts an independent Linux preview; `mmake calculator`
builds the native application, and rootfs staging installs Calculator.app and its
capability manifest. Sources belong to the root repository; no remote repository
is assumed.

Tests cover decimal exactness and limits, parser precedence and invalid input,
scientific domains, programmer wrap/shift/base semantics, all unit-pair round
trips, startup persistence, native keypad/keyboard interaction, angle selection,
per-mode drafts, repeated operations, operand edits, undo/redo, persisted history
and corruption handling, independent history input, bidirectional conversion,
compact keypad bounds/overlaps, and the shared preview clipboard.
