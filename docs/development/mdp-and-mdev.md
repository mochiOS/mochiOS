# MDP artifacts and development VMs

## Local commands

Run `mmake bootstrap` once after syncing the workspace. It installs checkout-backed
`mdp` and `mdev` symlinks in `~/.local/bin`, refuses to replace unrelated commands,
and does not download SDKs. Keep that directory on PATH. Python 3.11+ is required.
A custom install location is available with:

```sh
python3 scripts/dev/bootstrap.py --bin-dir /your/bin
```

Run `mmake development-environment-test` to test the tools and integration without
compiling the OS or launching QEMU.

MDP's generic format and SDK dependency lock are documented in
[`tools/mdp/README.md`](../../tools/mdp/README.md).

## VM environments

```sh
mmake run                # Existing image build, then mdev default VM
mmake run-existing       # Start the already-built image through mdev
mdev list                # Named VM environments, live status, source images
mdev run default         # Reboot the registered image without building
mdev remove default --yes
```

VM environments are stored under `out/develop/<name>/`. The regular development VM
copies the input image to its own writable disk on each boot. **Changes inside the
guest disk are reset on the next boot.** Its firmware variables and logs remain in
its runtime directory. The original `out/artifacts/disk.img` is never passed to
QEMU as a writable disk by this path. `MDEV_NAME` selects another development VM
name when using `mmake run`.

For persistent installer experiments, register a separate VM:

```sh
mdev create installer-test --image path/to/disk.img --runner scripts/runner.sh --persistent
mdev run installer-test
```

Persistent disks are retained across runs. A changed source image is an error;
create another named VM rather than silently resetting existing guest data.
The runner keeps its existing Secure Boot, device and capability-related settings.
The smoke-test runner remains separate and rejects mdev runtime arguments.

## Artifact-only composition

The new `mmake mdp-assemble` target consumes `mdp.assembly.lock` and invokes no
producer builds. Generate and review that lock using `mdp lock-assembly` against
an explicit `mdp.assembly.toml` listing local incoming MDPs. Both formats and
commands are described in the MDP README.

```sh
MDP_ASSEMBLY_LOCK=/path/to/mdp.assembly.lock MDP_OUTPUT=/path/to/new-output mmake mdp-assemble
```

The output contains `tree/<area>/...` and `receipt.json`. Existing output directories
are never overwritten. An application remains an MPKG inside its producer's MDP;
MDP never extracts or re-signs that application package.

## Migration status

The generic tool, dependency locks, artifact-only composition entry point,
bootstrap installation and named VM launch are implemented. **The existing OS
component build graph still exists.** `mmake build` and `mmake run` currently use
that graph; they have not been represented as an artifact-only migration.

Each owning repository must next gain a reviewed MDP producer manifest, its SDK
and dependency locks, and an independent build entry point. Product-specific
configuration must specify the selected MDPs, permitted output areas, initfs
membership, package provenance and signing requirements. Once the current image
assemblers accept those verified trees directly, the root's default image target
can switch to artifact-only composition and legacy producer calls can be removed.

Do not switch defaults before those producers and the signed image path are
verified. MDP hashes protect integrity relative to reviewed locks; they do not
replace mochiOS package signatures, image signatures, or capability checks.
