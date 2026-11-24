# QEMU mouse regression — 2026-11-24

The previous boot test explicitly selected PS/2. It did not verify the USB HID
tablet used for absolute mouse integration. This was an acceptance coverage gap.

## Cause and repair

- The xHCI Input Context contains an extra Input Control Context before the Slot
  and Endpoint Contexts. EP1 IN (DCI 3) was configured at `3 * context_size` rather
  than `4 * context_size`. QEMU accepted the command but never fetched the intended
  interrupt transfer ring. QEMU traces confirmed this missing fetch.
- Configure the endpoint from its DCI, including its context offset, add flag and
  Slot Context entry count; copy Slot Context to its correct Input Context offset.
- Issue Set Protocol only for boot-subclass HID devices.
- Keep a single HID transfer and reusable DMA report buffer outstanding while
  idle. A timeout must not enqueue another transfer or allocate another buffer.
- Sleep between idle polls instead of occupying a vCPU with yield loops.
- Advance command/transfer rings through Link TRBs and toggle cycle on wrap.
  Consume Event Ring entries in dequeue/cycle order, publishing ERDP ownership
  without zeroing controller-produced entries.
- Accept successful short HID reports using their actual byte length.
- Persist initialization/failure diagnostics through the inherited logger.

All changes are in the userland USB driver. No kernel, capability grants,
signature verification, input endpoint authentication or UI design changes.
The implementation was cross-checked against the primary QEMU xHCI source:
https://github.com/qemu/qemu/blob/master/hw/usb/hcd-xhci.c

## Verification

Ordinary `mmake -j2 build` succeeded. Four Rust regression tests cover endpoint
context placement (32/64-byte contexts), repeated ring wrap, consumer ownership
and short-report/error handling. Three Python tests verify that USB is selected
explicitly, missing USB cannot silently fall back to PS/2, and press/release plus
relative motion are encoded correctly.

Fresh Secure Boot QEMU/KVM guests used the ordinary image, 2 vCPU, 1 GiB, virgl
and the existing development automatic login. Each interaction assertion requires
Applications to open on mouse click and close on Escape.

| Run | Device | Interaction verified | Spaced motion injections | Full desktop time upper bound (s) | Strict sub-10s acceptance |
|---|---|---|---|---|---|
| `mouse-usb-verified` | tablet | True | 600 | None | False |
| `mouse-ps2-verified` | ps2 | True | 0 | 9.062799602000268 | True |

The USB stress result proves interaction after 600 injected motion reports and an
idle period; it does not claim 600 reports were individually consumed. Its strict
boot check did not pass because the status area was absent from the captured
sub-ten-second reference frames. This result must not be described as a complete
boot-acceptance pass. The separate PS/2 run passed both boot and interaction checks.
Raw results, screenshots and build logs are in `out/reports/boot-performance/` and
`out/reports/mouse-regression/`. Those folders include failed diagnostic iterations;
use the named final runs rather than treating every diagnostic as a success.

## Final USB acceptance

`mouse-usb-acceptance` passed both input and full-desktop checks at an external
upper bound of **9.877 seconds**, using the same
final image as the stress and PS/2 runs. It also sends Caps Lock at eight seconds
and USB pointer motion at 8.6 seconds.

A subsequent early-input run exposed a test comparison error: the final reference
contained the hovered Applications icon and tooltip, while the boot frame showed
a different hover-animation state. The comparison now uses the three stable Dock
icons and verifies Applications with the real click round trip. No image results
were rewritten retrospectively and no rendering threshold was widened. The
wallpaper, logo, status and clock checks are retained.

Final image SHA-256: `942faf9d693eac846b7e2c0fc1d2be116107229b532f5e4ea3c6c09f519f9014`.
Driver fix: `cb04221`. The successful ordinary build log is
`out/reports/mouse-regression/image-final.log`; the native producer remained cached
after committing the unchanged tested sources.
