# Mail — native interface preview

This first stage is an interactive AppCore / ViewKit mail interface using sample
messages. Small windows follow Samsung Email's list → message → back navigation;
larger windows use the mailbox / list / reading-pane organization of macOS Mail.
Existing omochi fonts, colors, control states, continuous corners and button
motion are reused. The application icon is the existing
`docs/resources/appicons/Mail.app.png`, unchanged.

Run `mmake preview-mail` for a standalone Linux window, or open Mail from the
launcher in `mmake ui`. Default size: 1080 × 720. Proposed app-local breakpoints:

- Below 740px: one pane, mailbox drawer, large inbox heading, search and a
  bottom-right compose action. Opening a message replaces the list; Back restores
  the same list offset.
- 740–999px: message list and reader; mailboxes open in a modal drawer.
- At least 1000px: mailbox sidebar, message list and reader. Drag the dividers to
  change their widths; the reading pane retains a minimum 400px width.

Supported interactions include mailbox/account selection, live text search,
read/unread state, conversation expansion, flagging, moving sample messages into
Archive/Trash, and session-only independent composition/reply/forward drafts.
Message-list offsets are retained per mailbox, reader offsets per conversation.
Window resizing keeps selection, navigation and editor interaction states.
Ctrl/Super+F searches, Ctrl/Super+N composes; Escape dismisses the drawer, leaves
composition while retaining its draft, or returns to the list in compact mode.
Organization actions use the native context-menu bridge, not an app-drawn menu.

## Scope

There is no IMAP/SMTP, account authentication, network access, background sync,
HTML mail renderer, actual attachment data, delivery, or persistence across app
restarts. Sample addresses use the reserved `.test` domain. Sending is disabled
and the UI identifies its sample content. Text-only message rendering does not
yet support text selection; composition uses the native TextEditor with IME,
selection, clipboard editing and undo/redo. The manifest requests window/IPC
capabilities only.

Reply, reply-all, forward, archive, attachment, flag and send do not yet have
registered dedicated VK Symbols. Actions use text where implemented; missing
symbols are not drawn or substituted with invented icons. Existing Mail, Tray,
Pencil, Search, List, Trash, File and navigation symbols are reused.

Tests cover layout bounds at compact/regular/wide widths, mailbox filtering,
native search input, list/reader navigation and resize restoration, modal drawer
and floating-button input priority, independent drafts, native menu actions and
split resizing. Native OS build verification is separate from the Linux preview;
the existing SDK `sys/wait.h` issue may prevent it before Mail is compiled.
