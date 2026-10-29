# Filesystem-owned storage reservations

This is the first part of the Installer storage boundary. Installer owns its
installation logic; no installation service or kernel installation policy is
introduced. The interface is generic and can also serve disk-management apps.

## Authority

`filesystem.service` admits the fixed `MFS1RSRV` request only after querying
`capability::check_thread` for the kernel-authenticated IPC sender's
`device.storage` capability. There is no PID, package name, capability string,
user ID or claimed approval flag in its payload. The kernel still enforces the
existing IPC capabilities and native Handle mechanism. Merely knowing a service
address, disk ID or inventory generation does not grant a reservation.

The request selects a disk from the owner's latest inventory generation. Before
allocation, the service freshly enumerates and compares complete observed disk
GUID, backend device ID, hardware capacity, sector size, and full partition
GUIDs/ranges. It rejects partial inventory, unknown mount ancestry, untracked
filesystem mounts, read-only or unknown-access disks, missing hardware identity,
mounted disks, conflicting reservations and table exhaustion. Partition-map
capacity estimates and blank disks without an authoritative identity are not
accepted as hardware evidence.

The native reply contains exactly one provider Handle with READ, CONTROL and
TRANSFER rights. It has neither WRITE nor DUPLICATE rights. The provider-owned
object shares the existing non-reused file-description ID allocator; the numeric
ID never appears in the acquire reply. Native-object delivery is checked before
provider dispatch. Ordinary filesystem messages cannot close it by guessing an
ID. The reservation is removed by the final native-reference release, including
failed reply publication and process termination. If native object creation
fails, the unpublished reservation is retired immediately.

## Scope and revalidation

The only operation on this Handle is `MFS1RVAL`, a read-only reinspection. Both
native QUERY and CONTROL can request it with no payload/attachments and exactly
an eight-byte result. READ/WRITE/SEEK/SYNC and unrelated controls are rejected.
Reinspection must match the original private fingerprint and mounted-volume
protection. Any failed observation or changed identity permanently revokes this
reservation. A restored-looking disk does not revive a revoked reference.
Close it, rediscover and review the destination before acquiring a new one.

Reservations are exclusive within this filesystem owner. While any reservation
exists, new service-owned filesystem mounts are conservatively rejected; existing
mounted filesystems remain usable. Device filesystems do not count as disk
mounts. The current service-owned mount metadata must cover every ext4 volume;
unknown provenance is a refusal, not evidence that a disk is idle.

## AppCore API

`storage::inventory_observation()` preserves the owner's selection generation;
`storage::inventory()` retains its existing return type and read-only behavior.
`StorageReservation::acquire` accepts an explicitly delegated filesystem SEND
Handle, an observation and a disk selection. That endpoint must come from trusted
launch/session wiring, not a caller-supplied global service number. The SDK does
not discover or manufacture authority. The service independently checks the
actual sender even when an endpoint Handle has been delegated.

The SDK validates exact status framing and received rights, reclaims unexpected
references once, and immediately revalidates the acquired object. Dropping the
non-cloneable wrapper closes it. Calls are blocking worker operations, never
paint callbacks. Linux preview returns UnsupportedPlatform and does not probe
host disks.

## Deliberate limits before destructive installation

This reservation is **not** a raw block-write capability. It does not constrain
another process already holding the existing broad `device.storage` capability,
and it cannot be used as proof for a later independent raw syscall. No Installer
manifest permission, write operation, formatter, unmount, partition edit or
installation success path is enabled by this work.

The current backend provides observations rather than a pinned per-device open
with a device lifetime/epoch. Matching GUIDs and layout are not a substitute for
that ownership: identical clones or a replaced device can reproduce metadata.
Before exposing destructive operations, the storage backend must bind the actual
opened device and permitted ranges to the reservation, enforce mounted-disk
exclusion at that operation boundary, revalidate and perform the operation under
the same exclusion, and strictly propagate flush/read-back failures. Cancellation
and uncertain I/O must not release authority as if nothing was written.

The new acquire request is native application IPC, not a Linux syscall ABI or a
kernel parser extension. Existing filesystem protocols and disk syscalls retain
their meaning. Current providers can refuse acquisition when hardware identity
or mount ancestry is unavailable. This is expected fail-closed behavior.

## Verification

Tests exercise capability rejection, malformed requests and attachments, alias
and duplicate reservation conflicts, bounded allocation, irreversible revocation,
mounted/unknown/read-only targets, changed observations during admission, mount
exclusion, publication cleanup, forbidden object operations, and SDK reference
cleanup. Host filesystem fixtures use temporary image files, never physical
storage. Native filesystem and SDK builds check the production adapters; these
tests do not claim a real-machine installation or power-loss test.

Future mutating controls must use a separately authorized storage object with
explicit WRITE authority. Adding a formatter or partition operation must not
silently turn these existing READ/CONTROL-only reservations into write permits.
