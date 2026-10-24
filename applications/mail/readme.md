# Mail

Native AppCore / ViewKit mail client. The existing compact list → reader
navigation, wide mailbox/list/reader layout, fonts, icons and omochi components
are retained. Default window: 1080 × 720. No web view is used.

## Preview without sign-in

`mmake ui` and `mmake preview-mail` open Mail with disposable English sample
messages by default. Inbox, reading, search, replies and drafts can be inspected
without a mail account or Keychain login. The status bar identifies preview data.
Sample accounts never connect to a server or access Keychain; Send is disabled.

Use **Accounts → Gmail / Other Account** to connect a real account explicitly.
A successful connection replaces the sample mailbox and starts real receiving.
To launch directly in the real account flow, set
`MOCHIOS_PREVIEW_MAIL_DEMO=0 mmake preview-mail` (also supported by `mmake ui`).
This option and the sample mailbox are Linux-only; native Mail still requires
normal account authentication.

## Connecting

With no registered accounts, **Add Account…** opens a service chooser. Choose
**Gmail**, **Other Account · IMAP** or **Other Account · POP3**. Gmail initially
asks only for an email address and an app password. **Advanced Settings…** reveals
username and server overrides. Other services show separate **Incoming Mail** and
**Outgoing Mail** groups, with explicit TLS / STARTTLS selections. Selecting a
security mode fills its standard port; it remains editable.

**Connect** checks incoming authentication and SMTP AUTH before saving, then
starts the first receive automatically. It never sends a test email. Both
services currently use the same username/password; blank username uses the email
address. Up to 16 accounts can be registered.

**Accounts** lists registered accounts separately from the add-account flow.
Choose one to view its connection settings. **Remove Account…** names the account
and asks for confirmation; it removes the saved registration, not server messages.
To replace settings, remove the registration and add it again.

Gmail presets `imap.gmail.com:993` (TLS) and `smtp.gmail.com:587` (STARTTLS).
Use a Google **app password**, not the ordinary Google password. App passwords
require an eligible account with 2-Step Verification; some organization/security
policies disallow them. POP3 can be configured with `pop.gmail.com:995` after
enabling POP in Gmail's settings. Google OAuth login is **not implemented**:
a mochiOS Desktop OAuth client has not been registered yet. The future flow must
use the system browser, random-port loopback redirect, state and PKCE; no embedded
login page or deprecated out-of-band authorization code flow.

- [Google app passwords](https://support.google.com/accounts/answer/185833)
- [Gmail POP configuration](https://support.google.com/mail/answer/7104828)
- [Google native application OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)

Select an account's mailbox and press **Get Mail**. With the combined mailbox
selected, receive targets the current sending account; its address is shown in
the message-list header.
Receive currently downloads that account's newest **100 Inbox messages**, up to
8 MiB per message and 32 MiB per receive. There is no background polling or older
mail pagination yet. Reading/searching/organizing uses the in-memory copy.

In the composer, click the **From** menu to choose an account; replies use the
received account, and each session draft remembers its sender. The From row has
its own selector, including in compact windows. **Send** submits to SMTP;
recipient addresses are comma-separated. SMTP acceptance is reported only after
the final DATA response. Delivery status cannot be inferred after a connection
loss there, so the UI reports an uncertain outcome and never automatically
retries. A successful submission does not guarantee recipient delivery.

## Interface

UI labels, status messages, errors and message fallbacks use English. Received
message content keeps its original language. The original reading-pane layout,
colors, fonts and icon assets are retained. VK Symbols in the toolbar, folder
list and disclosure controls use the shared compact icon token (16 logical px)
instead of stretching to fill the control's allocation. Compose uses the same
field frames for paint and input; setup shares its field geometry too. Native
menus are used for choosing a sending account and organizing messages.

## Credentials and transport

Configuration and passwords are serialized into **Mail-owned Keychain items**
(`org.mochios.mail.account.v1`), never plaintext config or log files. Account
removal deletes the corresponding item. Credentials are obtained for each job,
held in zeroing containers and dropped when the job ends. Form password storage
uses ViewKit's sensitive input state and is transferred/cleared on submission.
The service must be unlocked; there is no bypass when it is unavailable or locked.

Linux uses host TCP. mochiOS uses the existing network.service TCP IPC protocol;
the adapter verifies the built-in service's security context, request IDs and
bounded replies. The service enforces `net.connect`; Keychain enforces its own
caller identity and access rights. No kernel/network-service changes are needed.
Both use the same mochiOS TLS client and certificate/time/hostname verification.
**The current OS TLS policy supports TLS 1.3 only**; TLS 1.2-only mail servers
are not compatible. No insecure fallback, certificate acceptance switch or
plaintext authentication mode is provided. STARTTLS must succeed and prefetched
plaintext at the upgrade boundary is rejected. SMTP repeats EHLO after upgrade.

Network, Keychain and MIME work run off the UI thread. Socket operations have
15-second timeouts and each transport a 120-second deadline; Linux host DNS
resolution follows the host resolver's timeout policy. Responses, lines, MIME
nesting/part counts and the total live message body store (64 MiB) are bounded.
HTML, scripts and remote images are never loaded. Server errors and credentials
are not logged or interpolated into error labels.

## Current limits

- Inbox receiving and SMTP sending are connected. Folder/flag/read changes are
  **session-local** and not synchronized to the server. IMAP uses EXAMINE and
  BODY.PEEK; POP3 requires UIDL and never issues DELE. POP account configuration
  does not delete server mail when local Trash is used.
- UIDVALIDITY + UID and account identity isolate IMAP messages; UIDL identifies
  POP messages. Refresh preserves local state for retained identities and resets
  identities on UIDVALIDITY changes. Only the newest receive window is retained.
- Drafts, downloaded messages, local organization and the Sent display are
  session-only. SMTP does not currently append a copy to the server's Sent folder.
  Some providers, including Gmail, maintain their own sent copy.
- MIME/RFC 2047/2231 and Japanese charsets are decoded using mailparse.
  Only text/plain bodies are rendered. HTML-only mail displays an explicit
  unsupported notice. Attachment names are displayed; opening, saving and sending
  attachments are not implemented. Conversation grouping, reply threading headers,
  notification integration and remote folder discovery are still pending.
- `mmake preview-mail` starts the same executable-pinned, disposable Keychain
  daemon as the desktop preview unless an existing preview socket is supplied.
  **Preview account registrations disappear when that preview session ends**.
  Native account registrations persist in the user's encrypted vault. A standalone
  Mail preview no longer requires launching Keychain separately.

## Verification

Use `scripts/dev/preview-app.sh mail --test` with the workspace Cargo patches
(the root `mmake ui-test` target supplies them for all applications).
`mmake preview-mail` runs the UI; `mmake ui-check` builds the desktop preview.

Tests cover the responsive UI/navigation/drafts and native menus, account setup
steps, selective Gmail fields, nonoverlapping controls, credential validation,
sender-menu identity and bounded icon drawing, as well as IMAP
literal framing/UID matching, POP UIDL/dot unstuffing, SMTP multiline responses,
AUTH PLAIN/LOGIN and dot stuffing, uncertain delivery, command/header injection,
response bounds, Japanese MIME round trips, attachment names, recursive MIME
rejection, account identity and refresh state. Local TLS tests exercise real
sockets, large encrypted writes, hostname/root rejection and STARTTLS boundaries.
No real account credentials or external test email are used. UI inspection and
real-service end-to-end login/delivery remain manual verification.

Standards: [IMAP](https://www.rfc-editor.org/rfc/rfc9051),
[POP3](https://www.rfc-editor.org/rfc/rfc1939),
[SMTP submission](https://www.rfc-editor.org/rfc/rfc6409).
