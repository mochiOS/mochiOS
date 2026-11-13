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
