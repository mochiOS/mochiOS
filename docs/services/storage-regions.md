# Native bounded storage regions

`device.storage.scoped.write` is a privileged, non-user-grantable permission for
requesting writable regions from the filesystem owner. It does not imply raw
`device.storage`. AppCore exposes `StorageRegion`, which owns a native provider
Handle; applications never receive a raw backend device ID. Neither Installer's
manifest nor its launch wiring is granted this permission by this change.

## Admission and lifetime

The `MFS1RGNW` request carries only the inventory generation, logical disk ID,
first sector, sector count and the hardware epoch retained during plan review. It must arrive through a delegated endpoint Handle,
without attachments. The filesystem checks the kernel-authenticated sender,
rejects mounted/unknown/read-only targets and partial observations, and compares
fresh inventory before reserving the target. A new driver identity query supplies
Linux's disk sequence and real hardware geometry. This identity is read before
reinspection and checked against the reviewed epoch, then checked again on the
actual opened block device. A blank disk
can be selected without inventing a GPT GUID.

A READ-only root-Handle query (`MFS1DEPO`) returns the ephemeral hardware epoch
for a specific inventory generation and selectable disk. AppCore exposes it as
`hardware_epoch`; Installer binds it into the reviewed plan and checks it again
before acquiring a region. Same-GUID clones substituted after review are refused.
The epoch is identity evidence, not permission to write.

The backend opens and retains the actual Linux block-device file with exclusive
whole-device ownership, checks the expected disk sequence, rejects the disk used
by the active legacy block session, and holds the requested range. Reopening by
path or rediscovering by GUID on each write is forbidden. Reusing a Linux device
number cannot retarget an existing object. Every I/O checks the retained device's
liveness and disk sequence. Tokens increase monotonically and never wrap/reuse.

The Handle has READ, WRITE, CONTROL and TRANSFER, without DUPLICATE. Positioned
reads and writes are sector-aligned and bounded; flush is the only control.
Unrelated file controls, sequential I/O and attachments are refused. Provider
object IDs are not authority: native object dispatch is mandatory. Mount creation
remains excluded while a reservation exists. Releasing the final reference closes
the exact backend region. A close that cannot be confirmed leaves the reservation
quarantined rather than allowing the disk to be mounted or retargeted.

Any I/O failure permanently revokes further operations on the filesystem object.
Known device errors propagate to Installer. A malformed/lost control response
poisons the shared control session: no storage transfer or metadata-buffer reuse
is permitted after uncertain completion. Failed opens with uncertain results also
retain their filesystem reservation. Reboot/recovery, rather than guessing that
nothing happened, is required for this case.

## Existing backend integration

The System Domain implements additive generic OPEN_REGION/CLOSE_REGION device
controls through the existing capability-checked device-control boundary. Existing
block syscalls copy caller buffers as before. Opaque region IDs are internal to
the trusted filesystem owner. There is no kernel Installer policy, path parser,
formatter or new application syscall.

mDriver operations 27–31 are open, read, write, flush and close. Negotiation bit 8
is optional; it is not added to the mandatory legacy mask. Older drivers refuse
region use safely. The current implementation intentionally supports one active
writable region and 512-byte logical sectors. Ambiguous disks behind one
controller, mounted disks, unsupported sector sizes and unavailable authoritative
geometry are errors, not guessed destinations.

Transfers use reserved grant page 8 under the single serialized control owner;
this page already belongs to installation metadata and does not overlap display
surfaces. Legacy payload installation cannot run while a region owns that page.
Trusted legacy metadata is restored before each subsequent legacy use. The
production Linux patch and System Domain share operation constants and tests.

The region excludes supported owner/backend operations, not a compromised trusted
process already holding broad raw storage authority. Do not grant broad storage
access to Installer as a shortcut.

## Verification

`python3 mboot/mdriver/tests/scoped-storage-test.py` extracts the exact production
patch header and tests retained-device identity, mounted legacy exclusion,
exclusive-open failure, bounds/overflow, stale tokens, hot removal, changed device
sequence, short I/O, flush failure and close. Its devices are in-memory fixtures.
The patched mDriver object is also built against Linux 6.12.98 and the actual
mDriver configuration. Filesystem and AppCore tests cover admission, bounds,
revocation, native reference cleanup and capability separation. These checks do
not constitute a physical-disk, firmware-boot or power-loss qualification.
