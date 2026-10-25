# Disk Utility

Initial native AppCore / ViewKit interface for disk, partition and volume inspection.
Default window: 960 × 640. Labels use English and the existing omochi theme,
shared button padding and VK Symbols. No disk access or changes are performed.

## Preview

`mmake preview-disk-utility` opens the app directly. `mmake ui` includes it in the
application catalogue. Linux uses disposable, explicitly labelled sample disks;
it never enumerates, opens, mounts or modifies host disks. Set
`MOCHIOS_PREVIEW_DISK_UTILITY_DEMO=0` to inspect the unconnected state instead.
Native mochiOS always opens the unconnected state; it never displays the fixtures
as real device information. Refresh reloads fixtures while keeping selection and
disclosure state; it is disabled for the unconnected backend.

The sidebar is a disk → partition → volume tree, with unallocated disk space as a
separate entry. Selecting an entry opens its details. Disk partition bars are
proportional to partition sizes; the list below provides selectable names and
sizes even when a partition is too small to target in the bar. Volume usage is
separate from disk unallocated space. Missing/invalid usage stays unknown rather
than being reported as zero. At widths below 700px, the list and details become
separate views with a Disks back button. These dimensions are app layout choices,
not new global design tokens.

The wide toolbar reserves Mount/Unmount, Partition and Erase controls; they are
disabled and labelled as unconnected. Mount, unmount, erase, partition editing
and rename are not implemented or exposed as working actions. The manifest requests only IPC and window creation; there is
no raw storage capability. The application icon is the existing `docs/resources/appicons/Disk Utility.app.png`,
copied unchanged to `appicon.png`. Partition/eject VK Symbols are pending; controls
use existing VK Symbols and text labels. No placeholder disk-management icons were invented.

## Connecting actual storage

Existing `mochi_user_platform::storage::control` provides LIST_DEVICE / INSPECT
and partition-control selectors. Their response contracts, access rights and
current mDriver support need verification before connecting them. Existing
filesystem volume status / POSIX statvfs provides volume accounting, including
read-only state; it does not enumerate all devices or implement mount/unmount.
The filesystem protocol's legacy OP_MOUNT returns root metadata and must not be
misrepresented as dynamic mounting.

The next stage is an authenticated userspace inventory adapter that keeps device,
partition and mounted-volume identities distinct and queries capacity from the
actual filesystem owner. Inventory work must leave the UI thread, retain known
selection by identity, and report stale/denied/unavailable states explicitly.
Management operations require a separate capability-checked path, protected
system-volume rules and operation-specific confirmation. The UI process should
not gain unrestricted raw write access merely to display inventory.

## Verification

Model tests cover partition accounting, unknown usage and stable selection through
refresh/disclosure. UI tests cover compact/wide pane bounds and native rendering
for every entry kind. Visual verification is performed by the user. Host and
mochiOS builds use the existing preview workspace and native SDK respectively.
