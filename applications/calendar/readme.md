# Calendar

A native ViewKit / AppCore calendar. The month grid and selected-day agenda use
standard omochi buttons, typography, colors, continuous corners and interaction
states. Event editing uses the shared Dialog, TextField, Checkbox and TextEditor.
The app icon is extracted from the user's existing Desktop mock SVG; existing
system icons and Dock styling are unchanged.

- Click a day to see its events; click an event to edit it.
- New Event (Ctrl/Super+N) creates a draft for the selected day.
- Previous/next month, Today, arrow keys, Page Up/Down and Home navigate dates.
- Events support a title, date (YYYY-MM-DD), all-day or same-day start/end times
  (24-hour HH:MM), location and multiline notes.
- Save persists the change; Cancel/Escape discards the draft. Deletion requires
  a separate confirmation. Cancelling that confirmation returns to the draft.
- Narrow windows stack the month and agenda; the agenda and shorter edit dialogs
  scroll. Existing theme tokens supply color, type and control geometry.

## Storage

Events are private local calendar data, stored at
`/libraries/applications/org.mochios.calendar/users/<USER>/events-v1` on mochiOS.
In desktop previews they live under `MOCHIOS_PREVIEW_CONFIG_ROOT/calendar`; an
independent Linux launch uses `~/.local/share/mochios/calendar`.
`mmake ui` deliberately discards its entire session, including its calendar data.

The bounded, versioned binary format rejects malformed dates, times, UTF-8,
duplicate IDs, truncation and excess data. Writes flush a temporary file, keep a
backup during publication, and recover an interrupted rename on the next open.
A storage lock and comparison against the loaded file prevent stale windows from
silently overwriting newer changes. Errors keep the edit visible and preserve
committed data. A process killed while holding the lock can leave `events-v1.lock`;
automatic stale-lock recovery and live cross-window synchronization are future work.

Events currently use floating civil dates and wall times, without recurrence,
multiple calendars, timezone conversion, reminders or network synchronization.
Linux's Today follows localtime; native Today matches Binder's current UTC clock.
Notifications are intentionally left for the later notification integration.

## Build

`mmake ui` includes Calendar in the real desktop application catalogue. The
right-hand desktop clock uses Binder's existing activation queue to open or
activate Calendar. `mmake preview-calendar` launches it independently on Linux;
`mmake calendar` builds the native binary, and rootfs staging packages Calendar.app
and its capability manifest. Sources currently belong to the root repository;
no remote Calendar repository is assumed.

Tests cover civil-date arithmetic, leap centuries and month grids, parsing,
storage round trips and corruption, concurrent-window conflicts, recovery, failed
saves, and native control flows for create/edit/confirmed deletion.
