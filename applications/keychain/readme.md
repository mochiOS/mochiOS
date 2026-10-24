# Keychain.app

Native AppCore/ViewKit management UI for the login-managed encrypted vault.
Uses existing omochi controls, theme, scrolling and picker menus.

- OS password authentication unlocks the vault automatically; screen lock/logout
  clears keys. Ordinary reveal/delete/grant/revoke needs no extra password.
- Search item labels/services/accounts, inspect owner/signing ID, and add items.
- Confirm the full target package/signing ID before granting or revoking reads.
- Hide displayed secrets after ten seconds, blur, selection changes or cancel;
  discard delayed reveal completions after focus loss.
- A legacy/differently protected vault requests its old password once for
  migration/recovery. It never silently replaces existing data with an empty vault.
- IPC/KDF stays off the UI thread; storage failures clear stale unlocked UI state.

Run `mmake preview-keychain` or open the app in `mmake ui`. Both Linux previews
use disposable encrypted session data and do not store a host login password.
Previously persistent preview vaults are left untouched. Native login integration,
password-change crash handling and remaining limitations are described in
`services/keychain/README.md`. Automatic OS login without password material leaves
the vault closed until OS password authentication; it does not create a weaker key.

The supplied revised icon is bundled as `appicon.png`; its original copy is kept
at `docs/resources/appicons/Key Chain.app.png`. The first API is Rust AppCore;
C/Kome bindings are not included.
