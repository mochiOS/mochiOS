# Application distribution

`build/application-sets.toml` defines which application bundles are built into
an image. After changing it, regenerate the committed mmake targets with:

```sh
python3 scripts/mmake/image-applications.py --root . --makefile > build/mmake/application-sets.mk
mmake image-applications-test
```

The normal `mmake image` / `mmake run` image includes Binder, Settings, Files,
Terminal, AppStore, DiskUtility, Viewer, Fonts, SystemMonitor and Keychain.
Only this set is a native application prerequisite of normal image staging.

Terra, Edit, Calendar, Mail and Calculator are designated for AppStore delivery.
Neither their application bundles nor their built-in package verification records
are added to normal or installer images. This change does not implement downloads,
change package signatures, or build Terra's vendored browser engine.

`mmake installer-image` builds the System set plus Installer.app into
`out/installer-artifacts/disk.img`. Its staging, partition images, ESP and disk
image use a separate `out/mmake/installer/` tree. The image uses the existing boot
and partition formats. It omits `/system/.installed`, so the existing first-boot
service-manager flow starts Installer after Binder. Normal OS staging retains
that marker and contains no Installer bundle or Installer verification record.

This application split does not enable Installer's disabled native write backend
or add an authenticated installation payload. Building installation media is not
a claim that disk installation has been enabled. Shared legal documents remain
available under `/system/resources/legal` in both image variants.

`test.app` is a development application, excluded from both distribution images.
Explicit application build targets and the Linux UI preview remain available;
`mmake apps` continues to build the existing all-application developer set.
