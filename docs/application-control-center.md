# Control Center application items

An installed application can add shortcuts to Binder's Control Center from its
single bundle `manifest.toml`.

```toml
[[application.control_center_items]]
id = "open-edit"
title = "Edit"
symbol = "pencil"
action = "open-application"
```

The item is declared alongside the bundle's `[package]` and `[application]`
metadata. `id` is local to the registering application and may contain lowercase letters,
digits, and hyphens. `symbol` must name an installed VK Symbols asset. Binder
currently accepts at most eight items from one application.

The `open-application` action activates the registering application when it is
running, or launches it when it is not. Binder does not load application code into
the system panel and does not accept commands, paths, or target bundle IDs from the
registration file.

An item can instead open a compact card inside Control Center:

```toml
[[application.control_center_items]]
id = "status"
title = "Status"
symbol = "info"
action = "show-card"
```

The application publishes the card values at runtime through AppCore and declares
the `control-center.register` capability. Re-publishing replaces the current values.

```rust
ControlCenterItem::register("org.example.app", "status")
    .card(
        ControlCenterCard::new("Status")
            .row("State", "Ready")
            .row("Version", "1.0"),
    )
    .publish()?;
```

Cards are declarative and limited to four compact rows. Binder renders them with
standard ViewKit components; application code is never loaded into the shell.

The Control Center's Edit button opens a normal, foreground desktop window. Users
can hide or restore registered and built-in items, move them with the arrow buttons,
or drag one row onto another row to reorder it. Each change is saved immediately in
the signed-in user's configuration directory. Removing an application removes its
items from the panel and the next editor launch cleans its stale preferences without
discarding the relative order of remaining items.
