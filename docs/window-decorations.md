# Integrated window decorations

AppCore uses the existing `App::window` / `App::window_for` creation hooks.
`WindowOptions` remains the window builder; there is no second window lifecycle
or title-bar-specific state store.

```rust
use mochios_appcore::prelude::*;
use mochios_appcore::window::{TitleBar, drag_region};

fn window_options() -> WindowOptions {
    WindowOptions::new("Application")
        .size(1000.0, 700.0)
        .titlebar(
            TitleBar::new()
                .height(44.0)
                .background(RectangleColor::SubtleSurface)
                .separator(true)
                .leading(Button::new("Menu").on_click(|| {}))
                .center(drag_region(Text::new("Application")))
                .trailing(Button::new("Settings").on_click(|| {})),
        )
}
```

This uses existing Rust ViewKit builders, which accept views directly rather
than a separate closure-based UI builder. Return the options from the normal
App hook and call `mochios_appcore::run::<Application>()`.

The shared defaults are a 32px logical title bar, 28px circular control
backgrounds and 14px symbols. The three controls, their gaps and trailing inset
reserve 94 logical pixels. `viewkit::decoration::metrics` and
`appcore::window::metrics` expose the same allocation-free definitions used by
the compositor. Explicit taller title bars remain supported. Physical minimum
dimensions and reserved widths round outwards using the negotiated scale.

## Modes and validation

- `DecorationMode::System` is the compatibility default.
- `titlebar(...)` implicitly selects `Integrated`.
- Explicit `System` together with `titlebar(...)` is an error in either builder
  order. `WindowOptions::validate()` and initial `run()` report it.
- Heights must be finite and within 1–4096 logical pixels. The window system
  decides the actual minimum height and reserves the system control strip.
- Fullscreen, secure overlays and system modals remain separate properties;
  they suppress title-bar presentation without creating a Client mode.
- Integrated chrome is implemented for mochiOS and the process-isolated Linux
  Binder preview. Standalone Linux/winit and Windows windows retain System
  decorations; initial Integrated requests there return Unsupported.
- Later negotiation errors are delivered once through `App::decoration_failed`.
  The previous protected platform geometry remains effective. A changed request
  can be retried; failed requests are not reported as successful negotiations.

## Dynamic views

Build the title bar from application-owned `State`, `Binding` and the ordinary
component InteractionState values, just like body content. Keep those states
outside `window_for`. Standard button, text-field, editor, slider, range-slider,
switch, scroll, tabs and segmented-control interaction states carry stable
identities, including the buttons retained by `TabsInteractionState`.
Insertion, deletion, hiding and reordering use the normal rebuild mechanism.
No native window is recreated for these changes. Retain a base `MenuButton`
and clone it during rebuilds; its open menu keeps the exact displayed action
snapshot even if the next view contains a different item list.

Use `Identified::new(ComponentInstanceId(...), view)` for custom stateful views.
Its component ID must remain unique within a window and stable across moves.
Identities are namespaced separately from automatic interaction-state IDs.
The accessibility/focus dispatcher follows identities when bounds move and
clears focus when an identity disappears. Retired view trees receive lifecycle
cleanup after reconciliation: removed controls release focus, pointer arming,
selection/drag state and IME using their existing focus-loss handlers. A release
requested by a removed keyboard-capturing component is applied by the runtime.

Mode, height, separator, background, resize policy and DPI are negotiated only
when their configuration changes. Text, tab selection and ordinary component
updates do not send a decoration configuration request. State/geometry changes
are delivered through `App::decoration_changed` and trigger layout/redraw.

See `libraries/viewkit/examples/title_bar.rs` for standard tabs, add/close,
reordering, menus and drag regions without custom widgets.

## Input and responsibility boundaries

ViewKit lays out the three lanes inside the custom region. Standard component
overlays can extend over the content, while the window system always protects
the reserved control strip. System
buttons are drawn by Binder's existing decorator in a separate, server-owned
surface. In Integrated mode that surface covers only the reserved trailing
strip, above the client surface, with an opaque background. Client drawing and
input cannot replace those controls. The compositor enforces ownership and
minimum dimensions; the app never receives `window.decorate` authority.

`DragRegion` first dispatches to its child. A consumed button/input press or
component-specific drag wins. Only an unconsumed primary press requests a
window move. `DragRegion::including_controls()` instead keeps ordinary button
and tab clicks, then promotes a held press to a window move after the shared
motion threshold. The runtime cancels child tracking before moving, suppresses
the subsequent click, and discards pending moves on release or focus loss.
This explicit policy is used by Terra's title bar; the default priority is
unchanged. The retained press serial stays private to the runtime and backend.
Explicit handlers may borrow
`EventContext::window_interaction_token()` and pass it to
`mochios_appcore::window::begin_move` or `begin_resize`.

The token is scoped to the current event dispatch. The compositor verifies
client ownership, the target surface, a live primary press, one-use serial,
window state and resize policy. Release invalidates outstanding serials. The
Linux preview performs the same ownership and active-press admission in its
existing parent/child transport before using Binder's existing move/resize
states. A fabricated, stale or replayed serial does not start a gesture.

## Existing protocol extensions

No additional service is introduced. In this checkout the window-manager
responsibility described as Kagami is implemented by `services/compositor` and
Binder's decorator; AppCore continues to use the shared ViewKit runtime bridge.

- Compositor opcode 14 configures the caller's own toplevel surface:
  surface token, System/Integrated, physical height, RGBA background, scale in
  1/1024 units, separator and resizable flags. The reply gives actual geometry
  and the existing window state value.
- Existing move/resize opcodes 105/106 additionally accept an owned content
  surface token and its active input serial. Privileged decorator requests
  retain their original admission rules.
- Pointer events retain the existing 24-byte prefix and append the one-use
  serial. Window-targeted event 15 reports actual decoration geometry/state.
- Decorator metadata retains its existing 112-byte prefix, followed by mode,
  height, reserved width, background and separator. A fixed-point scale at
  offset 132 extends the metadata to 136 bytes; Binder uses that explicit scale
  rather than inferring it from a rounded control width. Older metadata keeps
  its legacy 112px-width scale fallback. Deploy Binder, ViewKit and compositor
  from the same build when changing the shared geometry defaults.
- The private Linux preview transport adds decoration, layout and input-token
  messages; this does not create a new OS application ABI.

Window control buttons, maximize/minimize/restore, shadows and outer continuous
corners continue to use the existing window-manager implementation. Borderless
and fullscreen are not decoration modes. No Client mode is provided.
