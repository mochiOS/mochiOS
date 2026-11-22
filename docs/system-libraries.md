# System library components

System libraries are part of the verified, immutable System image. They update
with the OS. AppStore applications may depend on them; package.service does not
gain permission to modify `/system`.

The OS image builder consumes prebuilt inputs through
`build/system-libraries.toml`. Each entry names a package, its workspace-relative
product directory and the SHA-256 of its complete manifest. Producers own source
fetching, compilation and dependency assembly. The OS builder does not compile
WebKit or invoke Terra's build system.

WebKit's image layout is:

- `/system/libraries/libwebkit.so.1`: public native entry point.
- `/system/libraries/webkit/`: private upstream DSOs, preserving SONAMEs.
- `/system/libraries/webkit/helpers/`: exported Web and Network helpers.
- `/system/libraries/webkit/fonts.conf`: configuration for the installed System fonts.
- `/system/share/licenses/org.mochios.webkit/`: license notices.

Prepare the input from the verified mochiOS/libs build using that repository:

```sh
python3 tools/package_webkit.py --prefix /path/to/verified-target-prefix \
  --output /path/to/mochiOS/out/system-libraries/webkit --version 2.54.2
```

The generated manifest must match the digest pinned in the OS catalog. Updating
that digest is a reviewed OS source change, not an application installation.
Missing or changed inputs stop the image build rather than silently omitting a
required component. Existing `mmake image` rootfs staging consumes these inputs;
`mmake ui` does not need them.

`stage-system-libraries.py` validates the catalog identity, captured manifest,
file sizes/digests, modes, binary file bindings and confined destination paths.
It refuses symlink payloads, conflicting files and targets outside the System
library/license namespaces. It copies only into the new rootfs staging tree,
then invokes the existing `msign package bind-built-in` and `built-in-record`
commands. Built-in provenance in the input is a requested delivery mode, not
proof of authenticity. The ordinary System image sealing/signing and verified
boot establish trust in those records; a staging result is not a signed image.

Terra uses the existing `org.mochios.system` built-in verification identity with
package `org.mochios.webkit` and ABI `mochios-1`. Its checked-in dependency file
pins that identity for the public DSO and the two exported helpers. Terra's own
AppStore package has a separate application publisher/signature. The signed
helper selection and captured-byte validation continue to apply.

The System build integration and native final link do not establish HTML display
or full engine runtime correctness. Fonts/configuration/trust resources and
interactive engine behavior still need their corresponding runtime verification.

The Fontconfig configuration uses the actual installed family names: Inter
Variable, IBM Plex Sans JP and UDEV Gothic. It searches only
`/system/libraries/fonts`, with no home directory, host `/usr` configuration or
global writable cache. The existing filesystem policy permits reading immutable
System data; this does not grant a helper broad filesystem authority or authorize
loading arbitrary code.

### Native verification gate

Terra's opt-in `engine-probe` feature exercises the normal engine adapter and
AppCore signed executable references. It is separate from the distributed
application. A pass requires both real helpers, completed navigation, an engine
frame, abnormal Web process termination reporting and termination/reaping of all
owned helpers. A successful link, offline dependency-graph preparation or entry
into Terra's main function alone does not satisfy this gate.

The shared loader must accept both interleaved GNU and packed lld version-table
records, while retaining chain bounds, disjointness and version-binding checks.
Rust consumers also need the development SDK driver's compiler-builtins math
isolation so hidden Rust intrinsics do not suppress newlib's dynamic math exports.
Locally rebuilt runtimes must be recorded separately from a published SDK.
Temporary diagnostic runtimes are not SDK release artifacts.

Keep native VM results and exact input hashes with the execution evidence. Leave
`runtime_tested` false until the complete probe passes. Visual verification of
`terra:///engine-check` remains a separate check in the normal desktop application.

### Native browser file/input workflow (2026-11-23)

The candidate adapter adds a shared per-browser ephemeral network session,
verified transport reporting, broker-selected upload/download staging and a
WebKit InputMethodContext fed by ViewKit's generic InputMethodClient. No browser
code or WebKit dependency was added to AppCore or the kernel. Existing helper
capability requirements are unchanged.

Terra's README defines the current single-file/64 MiB limits and cancellation
semantics. The ABI's major-1 prefix remains 96 bytes on x86_64; the append-only
file extension needs the new 120-byte table. Terra safely copies the historical
prefix and disables unsupported extensions with an older engine. Build the
System component and Terra together to enable the new file/IME workflows.

Records and candidate binaries are under `out/reports/terra-workflow/`. The
previous engine probe's pass is historical evidence, not validation of these
new file/IME paths. The bundled `terra:///engine-check` now exercises password
input, file selection with an actual File.arrayBuffer read, and a Blob download.
Check those in a fresh candidate image, plus real HTTPS navigation, login in
two tabs, Japanese conversion/candidate selection, paste, cancelled selection,
failed save, closing a tab during selection, and helper termination. Do not
set runtime_tested for this revision on the basis of host contracts or builds.

The first native Rust build encountered a stale generated libc utsname layout.
A targeted libc cache clean regenerated the existing correct mochiOS definition;
no ABI declaration or size assertion was changed.

Implementation commits: mochiOS/libs `3069890` (shared session/TLS) and
`288a728` (files/IME), ViewKit `1b2ba76`, Terra `0e5b7a5`.
Terra 50 tests, the overlapping engine-probe configuration's 51 tests, ViewKit
319 tests (3 existing ignored), 3 WebKit host contracts and build-tool tests
passed. Both libwebkit.so.1 and the normal engine-linked Terra native product
built successfully. These results do not test live websites or native GUI input.

Reviewable unsigned candidates are `out/reports/terra-workflow/terra-package/Terra.mpkg`
and `out/reports/terra-workflow/system-webkit/`. The latter was staged with the
normal package_webkit tool, retaining the previously reviewed dependency/font
closure and its original notices, with the new adapter DSO. It remains
runtime_tested=false and signed=false. `verification.json` records source commits
and hashes. The default frozen System component/pin has not been replaced,
and no push, signing, release or AppStore upload was performed.
