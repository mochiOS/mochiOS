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
