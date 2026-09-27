# Application manifest

Every application bundle contains one `manifest.toml`. It is both the signed
package manifest and the runtime application metadata source; `about.toml` is not
part of the application bundle format.

```toml
format = 1

[package]
id = "org.example.editor"
name = "Editor"
version = "1.0.0"
vendor = "Example"
kind = "application"
architecture = "x86_64"
abi = "mochios-1"

[application]
entry = "entry.elf"
description = "Plain text editor"
icon = "appicon.png"
resources = ["appicon.png"]

[[binary]]
path = "/applications/Editor.app/entry.elf"
kind = "application"
requires = ["window.create"]
```

Binder, Settings, Files, the workspace service, and shell application launching
read this same file. Package installation copies the verified manifest into the
installed application bundle. Updates replace the package record and bundle copy
in the same transaction; removal removes both.

Application `entry`, `icon`, and resource paths are normalized relative paths.
The signed manifest validator and the runtime package parser both reject parent
traversal, absolute paths, malformed bundle names, and unsupported Control Center
actions.
