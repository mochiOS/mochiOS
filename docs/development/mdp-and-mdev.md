# MDP artifacts and development VMs

## Output directories

Workspace outputs are grouped by purpose:

```text
out/
  images/system/       Finished normal OS images and symbols
  images/installer/    Finished installation image
  images/hardware/     Optional hardware media
  images/releases/     Locally collected release files (never auto-published)
  build/os/            OS image assembly and intermediate files
  build/preview/       Linux preview workspace and session state
  build/host/          Host configuration tools
  cache/               Reusable compiler, port and MDP extraction caches
  sdk/                 Imported SDKs and immutable SDK contexts
  develop/             Named mdev VMs, private disks and execution logs
  reports/             Tests, diagnostics and migration records
  tmp/                 Disposable temporary files
```

Each producer repository keeps its own `out/`; the workspace does not relocate
those directories. MDP packages remain at the producer paths recorded in the
assembly lock. `out/arm/` belongs to the separate architecture investigation and
is not moved by this layout migration.

Run `mmake migrate-output-layout` once for an existing checkout, or use
`mmake bootstrap`, which also runs this migration. Moves rename directories on
the same filesystem; they do not copy multi-GB images. Conflicting old/new
folders and locked mdev VMs stop migration before any directory is moved.
Managed VM source-image paths are updated without resetting guest disks or
changing their content fingerprints.

Old compiler configurations, SDK wrappers and historical VM backing images may
contain absolute paths. Existing checkouts retain documented compatibility
symlinks for those old locations; new builds use the paths above. These links
are not additional copies, and fresh checkouts do not create absent legacy
locations. The exact mapping is in `out/reports/output-layout.json`. Do not
remove compatibility links while old caches or historical VMs still use them.

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
mdev start default       # Run in the background
mdev show default        # Configuration, live state and last exit result
mdev logs default --follow
mdev stop default        # Stop through the live supervisor
mdev remove default --yes
```

VM environments are stored under `out/develop/<name>/`. The regular development VM
copies the input image to its own writable disk on each boot. **Changes inside the
guest disk are reset on the next boot.** Its firmware variables and logs remain in
its runtime directory. The original `out/images/system/disk.img` is never passed to
QEMU as a writable disk by this path. `MDEV_NAME` selects another development VM
name when using `mmake run`.

`mdev configure default --env KEY=VALUE` changes a stopped VM's backend settings.
`mmake run` preserves those settings. `mdev doctor default` checks local inputs.
Stopping sends SIGTERM to the emulator's process group, escalating after the
timeout; it does not request a graceful guest OS shutdown. Shut down inside the
guest first when retaining guest filesystem state matters.

For persistent installer experiments, register a separate VM:

```sh
mdev create installer-test --image path/to/disk.img --runner scripts/runner.sh --persistent
mdev run installer-test
```

Persistent disks are retained across runs. A changed source image is an error;
create another named VM rather than silently resetting existing guest data.
Alternatively, `mdev reset installer-test --image path/to/new.img --yes` explicitly
discards its private disk and firmware and selects a new base image.
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

## Build every repository, then assemble the product

The root `mmake build` first merges configuration, records current workspace
inputs with `mdp workspace lock`, and runs `mdp workspace build` before consuming
`mdp.assembly.lock`. Repository-owned MDP recipes still own compilation. Producer
failures stop image assembly; missing or changed artifacts are never accepted.
The same ordered preparation applies to image and installer image entry points.
Use `mmake mdp-assemble` for explicit artifact-only composition.
The registry in `mdp.workspace.toml` covers every standard app,
the kernel, boot, services, drivers, extensions, libraries, programs and SDK.
Source-only dependencies are explicit; optional host tools and hardware media
have explicit producer entries too.

```sh
# After syncing, selecting an SDK and reviewing configuration/source changes:
mmake -j 4 -v build
mmake run
```

`-v` streams producer output, which is useful for the first full toolchain build.

`workspace lock` records the current Git revisions and local source edits;
it does not silently update repositories. Build completion produces exact MDP
hashes in `mdp.assembly.lock`. `mmake build` records current local changes
automatically; when invoking MDP directly, review source changes and refresh
the workspace lock explicitly. Cargo lock changes are retained. The build cache checks consumed
source, resolution, configuration, SDK and dependency identities, not timestamps
alone. Changes to consumed sources during a build prevent publishing a new
composition lock. Keep shared API repositories stable while verifying a complete
OS snapshot. A selected producer can still build while unrelated repositories
are edited; their stale artifacts are excluded until rebuilt.

Dependency cache keys use the exchanged artifact identity and measured generated
SDK files. A producer rebuilding the same artifact does not by itself rebuild
its consumers. Generated SDK settings are checked by content before cache reuse.

To work on one project, without rebuilding the whole workspace:

```sh
mdp workspace lock
mdp workspace build --select app-terminal -j 4
mmake build
```

This builds that producer and its dependencies. Previously built artifacts are
included only while their inputs still match. A partial build from a clean tree
does not make a complete OS; root assembly reports missing required producers.

Each repository owns `mdp.toml` and `mdp.recipe.json`. SDK variants in devkit and
the separate standard-library example have explicit alternate manifests. The
recipes use the mochiOS SDK's compiler definitions in
`tools/devkit/build-support/`, rather than invoking the root Makefile. Apps
export MPKGs; the generic MDP tool does not interpret them. The OS-specific image
consumer validates selected application inventories, places them in the existing
System layout and uses the existing built-in verification/signing tools.
Capability declarations and image verification remain in force.

## SDK inputs

For this source-development workspace, install the reviewed shared SDK with
komeup and copy its SDK tree to `out/sdk/shared-base` **before** creating the
workspace lock. That directory's content is pinned as an external input. It is
not downloaded or selected by `mmake`. `build/system-libraries.toml` separately
pins the existing mochiOS/libs System WebKit component manifest. Supply that
component at its declared location before building its import producer.

The runtime/fonts and shared SDK producers are materialized into verified,
content-addressed SDK contexts under `out/sdk/contexts/`. The local shared SDK
uses the imported compiler/sysroot plus the just-built runtime; its metadata
explicitly identifies this local overlay and does not claim to be a released
SDK. No SDK or library is published by this workflow.

After `sdk-context` completes, an independent application checkout can reuse the
explicit SDK context without invoking any workspace-wide build:

```sh
mdp lock --context /path/to/mochiOS/out/cache/mdp/developer-context.json
mdp sync
mdp build --context /path/to/mochiOS/out/cache/mdp/developer-context.json
```

The context is a local development descriptor, not a redistributable SDK. It
checks the real SDK artifact hashes and source revisions; its machine paths are
kept under `out/`, while consumer `mdp.lock` records the SDK identity. Rebuild the
context when SDK sources change. Generic format, API and pipeline details are in
[WORKSPACES.md](../../tools/mdp/WORKSPACES.md).

## Compilation caches and concurrent work

The source workspace currently shares `out/cache/rust/target` between native Rust
producers. This is a compiler cache; each repository still owns its exported
MPKG/MDP files. Cargo serializes builds that use the same target directory, so
an external application using that cache can delay OS integration builds.

An independent consumer can select a private cache in its own `mdp.recipe.json`
without changing the SDK or root assembler:

```json
"bindings": { "RUST_TARGET_DIR": "$(PROJECT)/out/cache/mdp/target" }
```

This trades reuse of the workspace's existing compiled dependencies for parallel
build isolation. Temporary compiler files use `out/tmp` in this workspace;
SDK/producer caches and VM state are separate from temporary files.

## Installer and optional artifacts

`mmake installer-image` consumes the same reviewed artifacts and adds Installer
only to the separate installation image. It does not enable Installer's disk
write gate. Normal System/Store/development application separation is unchanged.

Optional hardware media needs the already assembled normal boot/rootfs images:

```sh
mmake build
mdp workspace build --select mboot -j 4
mmake mboot-image
```

The optional `boot-hardware` producer owns compilation of the System Domain.
mBoot verifies and consumes its MDP as a prerequisite; it compiles its own
firmware and mDriver, and does not invoke boot's compiler pipeline. The final
hardware image is then exported separately from the normal OS payloads.

Host tools can be built explicitly with `--select mdp`, `--select mdev`,
`--select mws` or `--select mmake`; they are not OS payloads. `mmake clean` removes
composed images and artifacts while preserving producer caches, SDK contexts and
`out/develop/` VMs. It does not erase another repository's build outputs.

`mmake mdp-migration-test` checks registry coverage, producer shell syntax,
artifact-only root plans, MPKG inventory validation and forbidden placements.
`mmake development-environment-test` checks generic MDP/VM/bootstrap behavior.
Full native builds and signed image verification remain separate requirements;
a successful unit test is not proof that an OS boots.
