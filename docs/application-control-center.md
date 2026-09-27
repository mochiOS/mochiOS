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

The only application action in format 1 is `open-application`. It activates the
registering application when it is running, or launches it when it is not. Binder
does not load application code into the system panel and does not accept commands,
paths, or target bundle IDs from the registration file.

Users can hide, restore, and reorder registered and built-in items with Control
Center's Edit mode. These choices are stored in the signed-in user's configuration
directory. Removing an application also removes its items from the panel without
discarding the relative order of remaining items.
