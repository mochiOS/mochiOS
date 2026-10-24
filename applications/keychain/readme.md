# Keychain.app

Native AppCore/ViewKit management UI for the local encrypted keychain service.
Uses existing omochi controls, theme, native scrolling and native picker menus.
It contains no sample vault or plaintext-storage fallback.

- Create/unlock/lock the vault and change its separate master password.
- Search item labels, service names and accounts; inspect owner and signing ID.
- Add an item owned by Keychain, reveal it after reauthentication, or delete it.
- Grant/revoke reads to known application identities after confirming the target
  package/signing ID and re-entering the vault password. Ownership is unchanged.
- Hide revealed values after ten seconds, on focus loss, selection changes and
  cancellation. Binary secrets are identified rather than decoded lossily.
- Perform IPC/KDF work off the UI thread. Failed writes clear stale unlocked UI
  state, and service errors never silently create a replacement empty vault.

Run `mmake preview-keychain`, or open Keychain through `mmake ui`. The independent
preview retains its encrypted vault between launches; desktop sessions are
explicitly disposable. Native deployment installs both Keychain.app and
keychain.service. See `services/keychain/README.md` for the security boundary,
storage format and current lifecycle limitations.

There is no dedicated Keychain application icon among the supplied assets; the
launcher uses its existing generic fallback. No other application's icon changes.
The first API is Rust AppCore; C/Kome bindings are not added in this stage.
