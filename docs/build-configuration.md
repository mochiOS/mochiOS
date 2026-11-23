# Product configuration

Run `mmake menuconfig` to edit `.config`. Saving changes configuration only; it
neither builds producers nor downloads or publishes software. Existing values
remain unchanged unless edited. All UI text and help is in English.

## Categories

- **Applications**: independently embed Terra, Edit, Calendar, Mail or Calculator.
  All five default to off for AppStore delivery. Required System apps remain
  mandatory; WebKit is included independently of Terra.
- **Drivers**: USB xHCI, PS/2 i8042 and Virtio networking. These flags control
  the driver MDP producers and their image placement.
- **Development**: selftest payloads, kernel instrumentation, native development
  tools and their build concurrency, the Rust toolchain and development login.
- **Boot and trust**: the existing Secure Boot requirement and development
  signing/trust choices. Development keys and auto-login should remain disabled
  in release configurations; changing their defaults is not part of this update.
- **Image layout**: initial disk, ESP, Data and initfs capacities in MiB.
- **VM launch defaults**: existing runner display, acceleration, graphics, CPU
  and GDB options. Per-VM mdev environment overrides take precedence. These
  settings do not configure compilation or alter an existing VM disk.

After changing image/producer settings, run `mmake build`. It records the current
workspace inputs, builds the repository-owned MDP producers and then assembles
the image. `mmake -j N build` forwards the job count to MDP. The image and installer entry points use the same ordered producer stage;
`mmake mdp-assemble` remains available for explicit artifact-only composition. The composer rejects artifacts for a different `.config`; changing
menuconfig is not enough to reuse an old artifact lock for a different product.
Application build commands remain owned by their MDP producers; the root
entry point delegates to MDP instead of duplicating their build recipes.

The former ARCH_X86_64, ENABLE_SMP, ENABLE_KPTI, ENABLE_ASLR and
USER_BUILD_MPK_SAMPLES menu entries had no consumer in the current native build
path. They are no longer advertised as functional switches. Their existing
configuration entries are preserved, and their defaults remain for compatibility.
No kernel security feature has been enabled or disabled by this menu change.

## Controls

| Key | Action |
| --- | --- |
| Up / Down | Select an entry |
| Enter / Right | Open a category or edit an option |
| Escape / Left | Return to the parent category |
| Space | Toggle a boolean or cycle a choice |
| y / n | Explicitly enable or disable a boolean |
| / | Search keys, labels and descriptions across all categories |
| F3 | Jump to the next search result, wrapping around |
| ? / F1 | Read the selected option's description, key and default/current value |
| d | Restore only the selected option's default |
| s | Save |
| q | Quit, confirming before discarding unsaved changes |

Descriptions are also shown below the option list. Help scrolls with the arrow
and Page Up / Page Down keys. Search never changes option values. Unknown entries
are retained when menuconfig saves, including existing keys not exposed in the UI.
`mmake config` subsequently validates supported values and merges new defaults.

## Validation

`mmake menuconfig-test` compiles the host tool with warnings treated as errors,
checks schema/default consistency, and exercises search, help, resetting, saving
and preservation of existing settings through a pseudo-terminal. It also checks
invalid boolean values and VM CPU counts through the real configuration merger.
`mmake image-applications-test` covers optional-app selection and removal of
bundle/trust records when those apps are disabled.

For schema parsing without a terminal or any writes:

```
out/build/host/menuconfig --schema build/schema.conf --config .config --check
```

`--check` validates schema structure and loading; numeric/boolean configuration
validation remains in `mmake config`.
