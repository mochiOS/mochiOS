use crate::model::{self, Id, Kind, Model};
use appcore::prelude::*;
use appcore::viewkit::event::{EventContext, EventResult, ViewEvent};
use appcore::viewkit::view::{Constraints, MeasureContext, PaintContext};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Select(Id),
    Toggle(Id),
    Refresh,
    Back,
}
struct Core {
    model: RefCell<Model>,
    pending: Rc<Cell<Option<Action>>>,
    buttons: RefCell<BTreeMap<Action, ButtonInteractionState>>,
    tree_scroll: ScrollState,
    detail_scroll: ScrollState,
}
impl Core {
    fn button(&self, label: &str, action: Action) -> Button {
        let interaction = self.buttons.borrow_mut().entry(action).or_default().clone();
        let pending = self.pending.clone();
        Button::with_interaction_and_label(interaction, label)
            .size(ButtonSize::Small)
            .style(ButtonStyle::Ghost)
            .on_click(move || pending.set(Some(action)))
    }
    fn apply(&self) -> bool {
        let Some(action) = self.pending.take() else {
            return false;
        };
        let mut m = self.model.borrow_mut();
        match action {
            Action::Select(id) => {
                if m.select(id) {
                    self.detail_scroll.reset();
                }
            }
            Action::Toggle(id) => m.toggle(id),
            Action::Back => m.detail_open = false,
            Action::Refresh => m.refresh(),
        }
        true
    }
    fn tree(self: &Rc<Self>) -> Scroll {
        Scroll::new(self.tree_scroll.clone()).content(Tree(self.clone()))
    }
    fn detail(self: &Rc<Self>) -> Scroll {
        Scroll::new(self.detail_scroll.clone()).content(Detail(self.clone()))
    }
}
#[derive(Clone)]
pub struct DiskView {
    core: Rc<Core>,
}
#[derive(Clone, Copy)]
struct Frames {
    toolbar: Rect,
    tree: Option<Rect>,
    detail: Option<Rect>,
    footer: Rect,
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(0.0), h.max(0.0))
}
impl Frames {
    fn new(b: Rect, detail_open: bool) -> Self {
        let top = b.size.height.min(48.0);
        let footer_height = (b.size.height - top).clamp(0.0, 40.0);
        let body = rect(
            b.origin.x,
            b.origin.y + top,
            b.size.width,
            b.size.height - top - footer_height,
        );
        let compact = b.size.width < 700.0;
        let (tree, detail) = if compact {
            ((!detail_open).then_some(body), detail_open.then_some(body))
        } else {
            (
                Some(rect(body.origin.x, body.origin.y, 232.0, body.size.height)),
                Some(rect(
                    body.origin.x + 232.0,
                    body.origin.y,
                    body.size.width - 232.0,
                    body.size.height,
                )),
            )
        };
        Self {
            toolbar: rect(b.origin.x, b.origin.y, b.size.width, top),
            tree,
            detail,
            footer: rect(
                b.origin.x,
                body.origin.y + body.size.height,
                b.size.width,
                footer_height,
            ),
        }
    }
}
fn fill(b: Rect, color: Color, c: &mut PaintContext<'_>) {
    Rectangle::new()
        .color(RectangleColor::Custom(color))
        .paint(b, c);
}
impl DiskView {
    pub fn new() -> Self {
        Self::with_model(Model::new())
    }
    fn with_model(model: Model) -> Self {
        Self {
            core: Rc::new(Core {
                model: RefCell::new(model),
                pending: Rc::new(Cell::new(None)),
                buttons: RefCell::new(BTreeMap::new()),
                tree_scroll: ScrollState::new(),
                detail_scroll: ScrollState::new(),
            }),
        }
    }
    fn toolbar(&self, b: Rect, compact_detail: bool) -> Vec<(Rect, Button)> {
        let mut buttons = vec![];
        if compact_detail {
            buttons.push((
                rect(b.origin.x + 12.0, b.origin.y + 9.0, 96.0, 30.0),
                self.core
                    .button("Disks", Action::Back)
                    .leading_symbol(SymbolName::ChevronLeft),
            ));
        }
        if b.size.width >= 700.0 {
            let m = self.core.model.borrow();
            let mounted = m
                .selected
                .and_then(|id| m.node(id))
                .is_some_and(|n| n.mount.is_some());
            for (i, label) in [
                if mounted { "Unmount" } else { "Mount" },
                "Partition",
                "Erase",
            ]
            .into_iter()
            .enumerate()
            {
                buttons.push((
                    rect(
                        b.origin.x + b.size.width - 368.0 + i as f32 * 84.0,
                        b.origin.y + 9.0,
                        80.0,
                        30.0,
                    ),
                    Button::new(label)
                        .size(ButtonSize::Small)
                        .style(ButtonStyle::Ghost)
                        .enabled(false)
                        .accessibility_label(format!(
                            "{label}. Storage management is not connected"
                        )),
                ));
            }
        }
        buttons.push((
            rect(
                b.origin.x + b.size.width - 104.0,
                b.origin.y + 9.0,
                92.0,
                30.0,
            ),
            self.core
                .button("Refresh", Action::Refresh)
                .leading_symbol(SymbolName::Refresh)
                .enabled(self.core.model.borrow().preview),
        ));
        buttons
    }
}
impl View for DiskView {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(c.maximum)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.core.model.borrow();
        let f = Frames::new(b, m.detail_open);
        fill(b, c.theme.colors.background, c);
        fill(f.toolbar, c.theme.colors.surface_subtle, c);
        let compact_detail = b.size.width < 700.0 && m.detail_open;
        if !compact_detail {
            Text::label("Disk Utility").paint(
                rect(
                    b.origin.x + 16.0,
                    b.origin.y + 12.0,
                    b.size.width - 132.0,
                    24.0,
                ),
                c,
            );
        }
        for (bounds, button) in self.toolbar(f.toolbar, compact_detail) {
            button.paint(bounds, c);
        }
        if m.nodes.is_empty() {
            let title = rect(
                b.origin.x + 24.0,
                b.origin.y + 112.0,
                b.size.width - 48.0,
                72.0,
            );
            Text::styled("Disk inspection is not connected", TextRole::TitleSmall).paint(title, c);
            Text::body("This build does not yet connect Disk Utility to the storage service.")
                .color(c.theme.colors.text_secondary)
                .paint(
                    rect(
                        title.origin.x,
                        title.origin.y + 88.0,
                        title.size.width,
                        96.0,
                    ),
                    c,
                );
        } else {
            if let Some(tree) = f.tree {
                fill(tree, c.theme.colors.surface_subtle, c);
                self.core.tree().paint(tree, c);
            }
            if let Some(detail) = f.detail {
                self.core.detail().paint(detail, c);
            }
        }
        fill(f.footer, c.theme.colors.surface_subtle, c);
        Text::caption(if m.preview {
            "Preview · Sample disks. Disk changes are disabled."
        } else {
            "Storage connection unavailable. No disk changes are made."
        })
        .color(c.theme.colors.text_secondary)
        .paint(
            rect(
                f.footer.origin.x + 12.0,
                f.footer.origin.y + 4.0,
                f.footer.size.width - 24.0,
                f.footer.size.height - 8.0,
            ),
            c,
        );
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        let detail_open = self.core.model.borrow().detail_open;
        let f = Frames::new(b, detail_open);
        if b.size.width < 700.0
            && detail_open
            && matches!(
                e,
                ViewEvent::KeyPressed {
                    key: appcore::viewkit::platform::Key::Escape,
                    ..
                }
            )
        {
            self.core.pending.set(Some(Action::Back));
        }
        let mut result = EventResult::Ignored;
        for (bounds, button) in self.toolbar(f.toolbar, b.size.width < 700.0 && detail_open) {
            result = result.merge(button.handle_event(bounds, e, c));
        }
        if !self.core.model.borrow().nodes.is_empty() {
            if let Some(tree) = f.tree {
                result = result.merge(self.core.tree().handle_event(tree, e, c));
            }
            if let Some(detail) = f.detail {
                result = result.merge(self.core.detail().handle_event(detail, e, c));
            }
        }
        if self.core.apply() {
            c.request_redraw_in(b);
            return EventResult::Consumed;
        }
        result
    }
}
struct Tree(Rc<Core>);
impl Tree {
    fn content(&self, theme: &Theme) -> VStack {
        let m = self.0.model.borrow();
        let mut stack = VStack::new()
            .alignment(StackAlignment::Stretch)
            .gap(StackGap::ExtraSmall);
        for (id, depth) in m.visible() {
            let n = m.node(id).unwrap();
            let children = m.children(id).next().is_some();
            let mut row = HStack::new()
                .alignment(StackAlignment::Center)
                .gap(StackGap::None);
            if children {
                row = row.child(
                    self.0
                        .button("", Action::Toggle(id))
                        .leading_symbol(if m.expanded.contains(&id) {
                            SymbolName::ChevronDown
                        } else {
                            SymbolName::ChevronRight
                        })
                        .accessibility_label(format!(
                            "{} {}",
                            if m.expanded.contains(&id) {
                                "Collapse"
                            } else {
                                "Expand"
                            },
                            n.name
                        ))
                        .frame(28.0, 28.0),
                );
            } else {
                row = row.child(
                    Spacer::new()
                        .into_stack_child()
                        .width(28.0)
                        .height(28.0)
                        .flex_grow(0.0),
                );
            }
            let selection = self
                .0
                .button(&n.name, Action::Select(id))
                .accessibility_label(format!(
                    "{}, {}, {}",
                    n.name,
                    n.kind.label(),
                    model::bytes(n.bytes)
                ))
                .accessibility_role(AccessibilityRole::ListItem)
                .accessibility_selected(m.selected == Some(id))
                .style(ButtonStyle::Custom {
                    background: if m.selected == Some(id) {
                        theme.list.selected_background
                    } else {
                        Color::TRANSPARENT
                    },
                    hovered_background: theme.list.hovered_background,
                    border: Color::TRANSPARENT,
                    hovered_border: Color::TRANSPARENT,
                    foreground: theme.list.foreground,
                })
                .alignment(ZStackAlignment::Leading);
            let selection = match n.kind {
                Kind::Disk => selection.leading_symbol(SymbolName::Internaldrive),
                Kind::Volume => selection.leading_symbol(SymbolName::Folder),
                _ => selection,
            };
            row = row.child(selection.layout().flex_grow(1.0));
            stack = stack.child(
                Padding::only(
                    if depth == 0 {
                        theme.spacing.medium
                    } else {
                        0.0
                    },
                    theme.spacing.small,
                    0.0,
                    theme.spacing.small + depth as f32 * theme.spacing.medium,
                )
                .content(row)
                .layout()
                .height(if depth == 0 { 44.0 } else { 32.0 }),
            );
        }
        stack
    }
}
impl View for Tree {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        self.content(ctx.theme).measure(c, ctx)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.content(c.theme).paint(b, c);
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        self.content(c.theme()).handle_event(b, e, c)
    }
}
struct Detail(Rc<Core>);
fn info(label: &str, value: impl Into<String>, theme: &Theme) -> HStack {
    HStack::new()
        .alignment(StackAlignment::Center)
        .gap(StackGap::Medium)
        .child(
            Text::caption(label)
                .color(theme.colors.text_secondary)
                .layout()
                .width(120.0)
                .flex_shrink(0.0),
        )
        .child(Text::body(value.into()).layout().flex_grow(1.0))
}
impl Detail {
    fn content(&self, theme: &Theme) -> Padding<VStack> {
        let m = self.0.model.borrow();
        let mut stack = VStack::new()
            .alignment(StackAlignment::Stretch)
            .gap(StackGap::Medium);
        let Some(n) = m.selected.and_then(|id| m.node(id)) else {
            return Padding::all(theme.spacing.large)
                .content(stack.child(Text::label("Select a disk or volume")));
        };
        stack = stack
            .child(Text::styled(&n.name, TextRole::TitleMedium))
            .child(
                Text::caption(format!("{} · {}", n.kind.label(), model::bytes(n.bytes)))
                    .color(theme.colors.text_secondary),
            );
        if n.kind == Kind::Disk {
            stack = stack.child(Text::label("Partition layout")).child(
                PartitionMap {
                    core: self.0.clone(),
                    disk: n.id,
                }
                .layout()
                .height(48.0),
            );
            for part in m.children(n.id) {
                stack = stack.child(
                    self.0
                        .button(
                            &format!("{}   ·   {}", part.name, model::bytes(part.bytes)),
                            Action::Select(part.id),
                        )
                        .alignment(ZStackAlignment::Leading),
                );
            }
            stack = stack
                .child(Divider::new())
                .child(info(
                    "Connection",
                    if n.removable { "Removable" } else { "Internal" },
                    theme,
                ))
                .child(info("Partition map", "GPT (sample)", theme));
        }
        if n.kind == Kind::Volume {
            stack = stack.child(Text::label("Storage usage"));
            if let (Some(used), Some(free)) = (n.used, n.free()) {
                stack = stack
                    .child(ProgressBar::new(if n.bytes == 0 {
                        0.0
                    } else {
                        used as f32 / n.bytes as f32
                    }))
                    .child(info("Used", model::bytes(used), theme))
                    .child(info("Available", model::bytes(free), theme));
            } else {
                stack = stack.child(Text::body("Usage unavailable"));
            }
            stack = stack
                .child(Divider::new())
                .child(info("File system", n.format.unwrap_or("Unknown"), theme))
                .child(info("Mount point", n.mount.unwrap_or("Not mounted"), theme));
        }
        if n.kind == Kind::Unallocated {
            stack = stack.child(Text::body("This space is outside the disk's partitions."));
        }
        if n.kind == Kind::Partition {
            stack = stack.child(Text::label("Volumes"));
            for volume in m.children(n.id) {
                stack = stack.child(
                    self.0
                        .button(&volume.name, Action::Select(volume.id))
                        .leading_symbol(SymbolName::Folder)
                        .alignment(ZStackAlignment::Leading),
                );
            }
        }
        stack = stack.child(info("Capacity", model::bytes(n.bytes), theme));
        if n.kind != Kind::Unallocated {
            stack = stack.child(info(
                "Access",
                if n.read_only {
                    "Read-only"
                } else {
                    "Read/write (sample)"
                },
                theme,
            ));
        }
        if let Some(disk) = m.disk_for(n.id).filter(|d| d.id != n.id) {
            stack = stack.child(info("Disk", &disk.name, theme));
        }
        stack = stack.child(Divider::new()).child(Text::label("Disk management"))
            .child(Text::caption("Mounting, erasing and partition editing will be available after storage management is connected.")
                .color(theme.colors.text_secondary));
        Padding::all(theme.spacing.large).content(stack)
    }
}
impl View for Detail {
    fn measure(&self, c: Constraints, ctx: &mut MeasureContext<'_>) -> Size {
        self.content(ctx.theme).measure(c, ctx)
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        self.content(c.theme).paint(b, c);
    }
    fn handle_event(&self, b: Rect, e: &ViewEvent, c: &mut EventContext<'_>) -> EventResult {
        self.content(c.theme()).handle_event(b, e, c)
    }
}
struct PartitionMap {
    core: Rc<Core>,
    disk: Id,
}
impl View for PartitionMap {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(Size::new(c.maximum.width, 48.0))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        let m = self.core.model.borrow();
        let Some(disk) = m.node(self.disk).filter(|d| d.bytes > 0) else {
            return;
        };
        let mut x = b.origin.x;
        for part in m.children(disk.id) {
            let width = b.size.width * (part.bytes as f64 / disk.bytes as f64) as f32;
            let color = if part.kind == Kind::Unallocated {
                c.theme.colors.surface_subtle
            } else if part.read_only {
                c.theme.colors.text_secondary
            } else {
                c.theme.colors.accent
            };
            fill(rect(x, b.origin.y + 8.0, width, 24.0), color, c);
            if x > b.origin.x && width > 1.0 {
                fill(
                    rect(x, b.origin.y + 8.0, 1.0, 24.0),
                    c.theme.colors.background,
                    c,
                );
            }
            x += width;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use appcore::viewkit::draw_command::DisplayList;
    use appcore::viewkit::typography::TextMeasurer;
    #[test]
    fn compact_and_wide_frames_never_overlap_the_toolbar_or_footer() {
        for width in [320.0, 699.0, 700.0, 960.0, 1600.0] {
            for open in [false, true] {
                let b = rect(24.0, 12.0, width, 480.0);
                let f = Frames::new(b, open);
                for body in [f.tree, f.detail].into_iter().flatten() {
                    assert!(body.origin.y >= f.toolbar.origin.y + f.toolbar.size.height);
                    assert!(body.origin.y + body.size.height <= f.footer.origin.y);
                    assert!(body.origin.x + body.size.width <= b.origin.x + width);
                }
                assert_eq!(f.tree.is_some(), width >= 700.0 || !open);
                assert_eq!(f.detail.is_some(), width >= 700.0 || open);
            }
        }
    }
    #[test]
    fn compact_back_button_preserves_the_selected_volume() {
        use appcore::viewkit::platform::PointerButton;
        let view = DiskView::with_model(Model::sample());
        view.core.model.borrow_mut().select(41);
        let b = rect(0.0, 0.0, 360.0, 480.0);
        let theme = Theme::LIGHT;
        let mut measurer = TextMeasurer::new();
        let mut c = EventContext::new(&theme, &theme.typography, &mut measurer);
        for e in [
            ViewEvent::PointerPressed {
                position: Point::new(40.0, 24.0),
                button: PointerButton::Primary,
            },
            ViewEvent::PointerReleased {
                position: Point::new(40.0, 24.0),
                button: PointerButton::Primary,
            },
        ] {
            view.handle_event(b, &e, &mut c);
        }
        assert!(!view.core.model.borrow().detail_open);
        assert_eq!(view.core.model.borrow().selected, Some(41));
        assert!(
            view.core
                .model
                .borrow()
                .visible()
                .iter()
                .any(|(id, _)| *id == 41)
        );
    }
    #[test]
    fn unconnected_backend_is_empty_and_never_becomes_a_sample_on_refresh() {
        let mut m = Model::sample();
        m.nodes.clear();
        m.selected = None;
        m.preview = false;
        m.refresh();
        assert!(m.nodes.is_empty());
        let view = DiskView::with_model(m);
        let mut list = DisplayList::new();
        let theme = Theme::DARK;
        let mut measurer = TextMeasurer::new();
        view.paint(
            rect(0.0, 0.0, 320.0, 480.0),
            &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer),
        );
        assert!(!list.commands().is_empty());
    }
    #[test]
    fn preview_paints_all_node_kinds_at_both_widths() {
        let view = DiskView::with_model(Model::sample());
        let theme = Theme::LIGHT;
        for width in [320.0, 960.0] {
            for id in [1, 10, 11, 41, 50, 111] {
                view.core.model.borrow_mut().select(id);
                let mut list = DisplayList::new();
                let mut measurer = TextMeasurer::new();
                view.paint(
                    rect(0.0, 0.0, width, 480.0),
                    &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer),
                );
                assert!(!list.commands().is_empty());
            }
        }
    }
}
