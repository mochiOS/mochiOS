# Native desktop boot acceptance

Build the ordinary image with `mmake build`, then run from the workspace root:

```sh
python3 scripts/tests/boot-performance.py boot-check-01 --gtk
```

Use a new run name each time. This requires Linux X11, accessible KVM and a DRM render device, QEMU with virgl, OVMF Secure Boot firmware, `sfdisk`, `debugfs`, `xwininfo`, Python Pillow and libX11. It uses the existing development image's login configuration; it does not bypass authentication or alter the image.

The timer starts immediately before launching QEMU. The fixture uses 2 vCPUs, 1 GiB RAM, KVM, `-cpu host`, Secure Boot and fresh writable firmware/guest-disk copies. Host page caches are not cleared. TCG and other hardware configurations need separate measurements.

The test captures frames below ten seconds and compares the wallpaper, OS logo, status symbols and Dock to the settled desktop. A READY log or placeholder frame alone cannot pass. After the boot capture interval it also clicks Applications and waits for the panel, sends Escape, and verifies that the desktop returns. The full-desktop time and this later functional round-trip time are separate JSON fields; the latter must not be reported as having completed below ten seconds.

Evidence is stored under `out/reports/boot-performance/<run>/`: captures, externally timestamped boot messages, guest service logs, configuration, image SHA-256 and `result.json`. Existing base-image boot logs are rejected to prevent stale readiness evidence. The command exits unsuccessfully if no complete frame below ten seconds is validated, real input fails, KVM is unavailable or the guest crashes.

For an additional early-input probe, use a fresh name and add `--early-input`. It sends Caps Lock at eight seconds and moves over Applications at 8.6 seconds. Inspect the sub-ten-second captures for the `あ` indicator and App Library tooltip. The normal image-completeness checks and later Applications/Escape test still run; early input transmission timestamps alone do not prove visual response.

The default input check now selects the QEMU USB HID Tablet explicitly and sends
absolute motion plus real button press/release reports. It must not substitute
PS/2 when USB fails. Test the independent legacy path with
`--pointer-device ps2`.

For an idle-then-continuous-input regression check:

```sh
python3 scripts/tests/boot-performance.py usb-pointer-stress --gtk --pointer-stress --interact-at 17 --seconds 45
```

This sends 600 spaced position changes after the desktop has been idle, then
requires the Applications click/Escape round trip to succeed. The report records
the selected device, number of injected reports and final QEMU device selection.
Injected reports are not a measurement of how many reports the guest consumed;
the subsequent visible interaction is the functional assertion. USB and PS/2
must be exercised in separate runs.

The earlier November 23 acceptance runs forced PS/2. They demonstrate that path
only and do not validate the USB tablet used for absolute-pointer integration.

Image completeness compares the first three stable Dock icons. Applications is
verified by actually clicking it: its hover scale and tooltip must not make an
early-input frame fail just because the final reference has a different hover
state. This does not relax the wallpaper, OS logo, status symbols or clock checks.
