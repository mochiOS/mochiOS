use crate::engine::{self, Mode, Settings, Value};
use crate::{
    editing,
    history::{self, History},
    preferences, units,
};
use appcore::prelude::*;
use appcore::viewkit::app::WindowId;
use appcore::viewkit::draw_command::DrawCommand;
use appcore::viewkit::event::{
    ContextMenuItem, ContextMenuRequest, EventContext, EventResult, ViewEvent,
};
use appcore::viewkit::platform::Key;
use appcore::viewkit::view::{Constraints, MeasureContext, PaintContext};
use appcore::viewkit::{close_window, request_new_window};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Action {
    Insert(&'static str),
    Clear,
    ClearEntry,
    Function(&'static str),
    More,
    Undo,
    Redo,
    CopyExpression,
    Delete(usize),
    CopyHistory(usize, bool),
    Back,
    Equals,
    Negate,
    Reciprocal,
    Square,
    Not,
    Copy,
    History,
    ClearHistory,
    ConfirmClearHistory,
    Recall(usize),
    Mode(Mode),
    Base(u32),
    Bits(u32),
    Category(usize),
    From(usize),
    To(usize),
    Swap,
    SetDefault,
}
struct Model {
    settings: Settings,
    start_mode: Mode,
    result: Option<Value>,
    answers: [Option<Value>; 4],
    drafts: [String; 4],
    evaluated: bool,
    repeat: [Option<engine::Repeat>; 4],
    repeat_input: bool,
    entry_cleared: bool,
    history_writable: bool,
    error: Option<String>,
    status: Option<String>,
    history: Vec<History>,
    scroll: f32,
    category: usize,
    from: usize,
    to: usize,
    preview: Option<(String, Settings, usize, usize, usize)>,
}
struct Pickers {
    mode: Picker,
    base: Picker,
    bits: Picker,
    category: Picker,
    from: Picker,
    to: Picker,
}
struct KeyButton {
    button: Button,
}
struct Core {
    model: RefCell<Model>,
    path: Option<PathBuf>,
    action: Rc<Cell<Option<Action>>>,
    input: TextFieldInteractionState,
    field: TextField,
    target: TextFieldInteractionState,
    target_field: TextField,
    target_editing: Cell<bool>,
    history_window: Cell<Option<WindowId>>,
    revision: State<u64>,
    undo: [UndoManager; 4],
    menu: RefCell<Vec<Action>>,
    menu_id: Cell<u64>,
    pickers: RefCell<Pickers>,
    degrees: State<usize>,
    angle: SegmentedControl,
    signed: State<bool>,
    signed_control: Checkbox,
    keys: RefCell<Vec<KeyButton>>,
    copy: Button,
    history: Button,
    default_mode: Button,
    clear_history: Button,
    swap: Button,
    history_buttons: RefCell<BTreeMap<usize, ButtonInteractionState>>,
}
#[derive(Clone)]
pub struct CalculatorView {
    core: Rc<Core>,
    history_only: bool,
}
fn index(mode: Mode) -> usize {
    match mode {
        Mode::Standard => 0,
        Mode::Scientific => 1,
        Mode::Convert => 2,
        Mode::Programmer => 3,
    }
}
fn grouped(value: &str) -> String {
    let end = value.find(['.', 'e', 'E']).unwrap_or(value.len());
    let (whole, rest) = value.split_at(end);
    let (sign, digits) = if let Some(n) = whole.strip_prefix('-') {
        ("-", n)
    } else {
        ("", whole)
    };
    if !digits.bytes().all(|c| c.is_ascii_digit()) {
        return value.into();
    }
    let mut out = sign.to_owned();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.push_str(rest);
    out
}
fn action_button(label: &str, action: Action, pending: &Rc<Cell<Option<Action>>>) -> Button {
    let pending = pending.clone();
    Button::new(label)
        .size(ButtonSize::Small)
        .on_click(move || pending.set(Some(action)))
}
fn pickers(model: &Model, action: &Rc<Cell<Option<Action>>>) -> Pickers {
    let mut mode = Picker::new(model.settings.mode.label());
    for choice in Mode::ALL {
        let pending = action.clone();
        mode = mode.option(choice.label(), move || {
            pending.set(Some(Action::Mode(choice)))
        });
    }
    let mut base = Picker::new(match model.settings.base {
        2 => "BIN",
        8 => "OCT",
        16 => "HEX",
        _ => "DEC",
    });
    for (label, n) in [("BIN", 2), ("OCT", 8), ("DEC", 10), ("HEX", 16)] {
        let pending = action.clone();
        base = base.option(label, move || pending.set(Some(Action::Base(n))));
    }
    let mut bits = Picker::new(format!("{} bit", model.settings.bits));
    for n in [8, 16, 32, 64] {
        let pending = action.clone();
        bits = bits.option(format!("{n} bit"), move || {
            pending.set(Some(Action::Bits(n)))
        });
    }
    let mut category = Picker::new(units::CATEGORIES[model.category].name);
    for (i, c) in units::CATEGORIES.iter().enumerate() {
        let pending = action.clone();
        category = category.option(c.name, move || pending.set(Some(Action::Category(i))));
    }
    let entries = units::CATEGORIES[model.category].units;
    let mut from = Picker::new(entries[model.from].name);
    let mut to = Picker::new(entries[model.to].name);
    for (i, unit) in entries.iter().enumerate() {
        let pending = action.clone();
        from = from.option(unit.name, move || pending.set(Some(Action::From(i))));
        let pending = action.clone();
        to = to.option(unit.name, move || pending.set(Some(Action::To(i))));
    }
    Pickers {
        mode,
        base,
        bits,
        category,
        from,
        to,
    }
}
fn key_specs(mode: Mode) -> Vec<(&'static str, Action)> {
    use Action::{Back, Clear, Copy, Equals, Insert, Negate, Not};
    if mode == Mode::Scientific {
        let mut specs = vec![
            ("sin", Action::Function("sin")),
            ("cos", Action::Function("cos")),
            ("tan", Action::Function("tan")),
            ("ln", Action::Function("ln")),
            ("log", Action::Function("log")),
            ("√", Action::Function("sqrt")),
        ];
        specs.extend(key_specs(Mode::Standard));
        return specs;
    }
    if mode == Mode::Programmer {
        return vec![
            ("AC", Clear),
            ("(", Insert("(")),
            (")", Insert(")")),
            ("NOT", Not),
            ("<<", Insert(" << ")),
            (">>", Insert(" >> ")),
            ("A", Insert("A")),
            ("B", Insert("B")),
            ("C", Insert("C")),
            ("D", Insert("D")),
            ("E", Insert("E")),
            ("F", Insert("F")),
            ("7", Insert("7")),
            ("8", Insert("8")),
            ("9", Insert("9")),
            ("AND", Insert(" and ")),
            ("OR", Insert(" or ")),
            ("XOR", Insert(" xor ")),
            ("4", Insert("4")),
            ("5", Insert("5")),
            ("6", Insert("6")),
            ("+", Insert("+")),
            ("−", Insert("-")),
            ("mod", Insert(" mod ")),
            ("1", Insert("1")),
            ("2", Insert("2")),
            ("3", Insert("3")),
            ("×", Insert("*")),
            ("÷", Insert("/")),
            ("Back", Back),
            ("0", Insert("0")),
            ("00", Insert("00")),
            ("Ans", Insert("Ans")),
            ("±", Negate),
            ("Copy", Copy),
            ("=", Equals),
        ];
    }
    vec![
        ("C", Action::ClearEntry),
        ("±", Negate),
        ("%", Insert("%")),
        ("÷", Insert("/")),
        ("7", Insert("7")),
        ("8", Insert("8")),
        ("9", Insert("9")),
        ("×", Insert("*")),
        ("4", Insert("4")),
        ("5", Insert("5")),
        ("6", Insert("6")),
        ("−", Insert("-")),
        ("1", Insert("1")),
        ("2", Insert("2")),
        ("3", Insert("3")),
        ("+", Insert("+")),
        ("0", Insert("0")),
        (".", Insert(".")),
        ("=", Equals),
    ]
}
fn keys(
    model: &Model,
    action: &Rc<Cell<Option<Action>>>,
    states: &[ButtonInteractionState],
) -> Vec<KeyButton> {
    key_specs(model.settings.mode)
        .into_iter()
        .enumerate()
        .map(|(i, (label, a))| {
            let enabled = if model.settings.mode == Mode::Programmer
                && label.len() == 1
                && label.as_bytes()[0].is_ascii_alphanumeric()
            {
                label
                    .chars()
                    .next()
                    .unwrap()
                    .to_digit(model.settings.base)
                    .is_some()
            } else {
                true
            };
            let label =
                if matches!(a, Action::ClearEntry) && (model.evaluated || model.entry_cleared) {
                    "AC"
                } else {
                    label
                };
            let pending = action.clone();
            let mut button = Button::with_interaction_and_label(
                states.get(i).cloned().unwrap_or_default(),
                label,
            )
            .on_click(move || pending.set(Some(a)))
            .size(ButtonSize::Large)
            .style(if matches!(a, Action::Equals) {
                ButtonStyle::Accent
            } else {
                if matches!(
                    a,
                    Action::Insert("+" | "-" | "*" | "/" | "%")
                        | Action::ClearEntry
                        | Action::Negate
                        | Action::Function(_)
                ) {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Standard
                }
            })
            .enabled(enabled);
            if matches!(a, Action::Back) {
                button = button.accessibility_label("Delete previous character");
            }
            if matches!(a, Action::Negate) {
                button = button.accessibility_label("Change sign of current number");
            }
            KeyButton { button }
        })
        .collect()
}
impl CalculatorView {
    pub fn new() -> Self {
        let path = preferences::path();
        let (path, mode, error) = match path {
            Ok(path) => {
                let loaded = preferences::load(&path);
                match loaded {
                    Ok(mode) => (Some(path), mode, None),
                    Err(error) => (Some(path), Mode::Standard, Some(error)),
                }
            }
            Err(error) => (None, Mode::Standard, Some(error)),
        };
        Self::with_preferences(path, mode, error)
    }
    fn with_preferences(path: Option<PathBuf>, mode: Mode, error: Option<String>) -> Self {
        let (saved_history, history_error) = match path.as_ref().map(|p| history::load(p)) {
            Some(Ok(entries)) => (entries, None),
            Some(Err(e)) => (vec![], Some(e)),
            None => (vec![], None),
        };
        let history_writable = history_error.is_none();
        let model = Model {
            settings: Settings {
                mode,
                ..Settings::default()
            },
            start_mode: mode,
            result: None,
            answers: [None; 4],
            drafts: std::array::from_fn(|_| String::new()),
            evaluated: false,
            repeat: [None; 4],
            repeat_input: false,
            entry_cleared: true,
            history_writable,
            error: error.or(history_error),
            status: None,
            history: saved_history,
            scroll: 0.0,
            category: 0,
            from: 0,
            to: 1,
            preview: None,
        };
        let action = Rc::new(Cell::new(None));
        let input = TextFieldInteractionState::new();
        input.set_focused(true);
        let submit = action.clone();
        let field = TextField::with_interaction(input.clone())
            .placeholder("0")
            .bordered(false)
            .on_submit(move || submit.set(Some(Action::Equals)));
        let target = TextFieldInteractionState::new();
        let target_field = TextField::with_interaction(target.clone())
            .placeholder("0")
            .bordered(false);
        let degrees = State::new(0);
        let angle = SegmentedControl::new(degrees.binding())
            .item(0, "Degrees")
            .item(1, "Radians");
        let signed = State::new(false);
        let signed_control = Checkbox::new(signed.binding()).label("Signed");
        let pickers = RefCell::new(pickers(&model, &action));
        let keys = RefCell::new(keys(&model, &action, &[]));
        let copy = action_button("Back", Action::Back, &action).style(ButtonStyle::Ghost);
        let history = action_button("History", Action::History, &action).style(ButtonStyle::Ghost);
        let default_mode = action_button("More", Action::More, &action).style(ButtonStyle::Ghost);
        let clear_history =
            action_button("Clear History", Action::ClearHistory, &action).style(ButtonStyle::Ghost);
        let swap = action_button("Swap", Action::Swap, &action);
        Self {
            core: Rc::new(Core {
                model: RefCell::new(model),
                path,
                action,
                input,
                field,
                target,
                target_field,
                target_editing: Cell::new(false),
                history_window: Cell::new(None),
                revision: State::new(0),
                undo: std::array::from_fn(|_| UndoManager::new()),
                menu: RefCell::new(vec![]),
                menu_id: Cell::new(0x43414c4300000000),
                pickers,
                degrees,
                angle,
                signed,
                signed_control,
                keys,
                copy,
                history,
                default_mode,
                clear_history,
                swap,
                history_buttons: RefCell::new(BTreeMap::new()),
            }),
            history_only: false,
        }
    }
    fn refresh_controls(&self) {
        let model = self.core.model.borrow();
        *self.core.pickers.borrow_mut() = pickers(&model, &self.core.action);
        drop(model);
        self.refresh_keys();
    }
    fn refresh_keys(&self) {
        let model = self.core.model.borrow();
        let states: Vec<_> = self
            .core
            .keys
            .borrow()
            .iter()
            .map(|key| key.button.interaction().clone())
            .collect();
        *self.core.keys.borrow_mut() = keys(&model, &self.core.action, &states);
    }
    fn current_settings(&self) -> Settings {
        let mut settings = self.core.model.borrow().settings;
        settings.degrees = self.core.degrees.get() == 0;
        settings.signed = self.core.signed.get();
        settings
    }
    fn persist_history(&self) {
        let mut model = self.core.model.borrow_mut();
        if !model.history_writable {
            return;
        }
        if let Some(path) = &self.core.path {
            if let Err(e) = history::save(path, &model.history) {
                model.error = Some(e);
            }
        }
    }
    pub fn history_view(&self) -> Self {
        Self {
            core: self.core.clone(),
            history_only: true,
        }
    }
    pub fn closing(&self, window: WindowId) {
        if window == WindowId::PRIMARY {
            if let Some(id) = self.core.history_window.take() {
                close_window(id);
            }
        } else if self.core.history_window.get() == Some(window) {
            self.core.history_window.set(None);
        }
    }
    fn calculate(&self, explicit: bool) {
        let settings = self.current_settings();
        let mut model = self.core.model.borrow_mut();
        let slot = index(settings.mode);
        model.preview = Some((
            self.core.input.value(),
            settings,
            model.category,
            model.from,
            model.to,
        ));
        let previous = model.answers[slot];
        let mut text = editing::complete(&self.core.input.value());
        let parsed = if explicit && model.evaluated && settings.mode != Mode::Convert {
            match (model.repeat[slot], model.result) {
                (Some(repeat), Some(left)) => {
                    text = repeat.expression(left);
                    repeat.apply(left, settings).map(|v| (v, Some(repeat)))
                }
                _ => engine::evaluate_with_repeat(&text, settings, previous),
            }
        } else {
            engine::evaluate_with_repeat(&text, settings, previous)
        };
        let result = parsed.and_then(|(mut value, mut repeat)| {
            if explicit && model.repeat_input && repeat.is_none() && settings.mode != Mode::Convert
            {
                if let Some(last) = model.repeat[slot] {
                    text = last.expression(value);
                    value = last.apply(value, settings)?;
                    repeat = Some(last);
                }
            }
            if settings.mode == Mode::Convert {
                let Value::Real(v) = value else {
                    return Err("A real number is required".into());
                };
                value = Value::Real(units::convert(v, model.category, model.from, model.to)?);
            }
            Ok((value, repeat))
        });
        if settings.mode == Mode::Convert {}
        match result {
            Ok((result, repeat)) => {
                model.result = Some(result);
                model.error = None;
                model.status = None;
                if settings.mode == Mode::Convert && !self.core.target_editing.get() {
                    self.core.target.set_value(result.display(settings));
                }
                if explicit {
                    let canonical = engine::materialize_answer(&text, previous);
                    self.core.input.set_value(text);
                    model.answers[slot] = Some(result);
                    model.repeat[slot] = repeat;
                    model.repeat_input = false;
                    model.evaluated = true;
                    let id = model
                        .history
                        .iter()
                        .map(|e| e.id)
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1);
                    let entry = History {
                        id,
                        input: canonical,
                        result,
                        settings,
                        units: (model.category, model.from, model.to),
                    };
                    if model.history.last().is_none_or(|e| {
                        e.input != entry.input
                            || e.settings != entry.settings
                            || e.units != entry.units
                            || e.result != entry.result
                    }) {
                        model.history.push(entry);
                        if model.history.len() > history::LIMIT {
                            model.history.remove(0);
                        }
                        self.core.history_buttons.borrow_mut().clear();
                    }
                }
            }
            Err(e) => {
                model.result = None;
                model.evaluated = false;
                model.error = explicit.then_some(e);
            }
        }
        drop(model);
        if explicit {
            self.refresh_keys();
            self.persist_history();
            self.changed();
        }
    }
    fn reverse_conversion(&self) {
        let settings = self.current_settings();
        let mut model = self.core.model.borrow_mut();
        let converted = engine::evaluate(&self.core.target.value(), settings, None).and_then(|v| {
            let Value::Real(n) = v else {
                return Err("A real number is required".into());
            };
            units::convert(n, model.category, model.to, model.from)
                .map(|source| (Value::Real(source), v))
        });
        match converted {
            Ok((source, target)) => {
                self.core.input.set_value(source.literal());
                model.result = Some(target);

                model.error = None;
            }
            Err(e) => {
                model.result = None;
                model.error = Some(e);
            }
        }
        model.evaluated = false;
        model.repeat_input = false;
    }
    fn changed(&self) {
        self.core
            .revision
            .set(self.core.revision.get().wrapping_add(1));
    }
    fn edit_operand(&self, action: Action, frames: &Frames, context: &mut EventContext<'_>) {
        if self.core.model.borrow().evaluated {
            if let Some(value) = self.core.model.borrow().result {
                self.core.input.set_value(value.literal());
            }
        }
        let text = self.core.input.value();
        let selection = self.core.input.selection_range();
        let range = if self.core.model.borrow().evaluated {
            0..text.len()
        } else {
            editing::operand(&text, selection)
        };
        let value = if range.is_empty() {
            "0"
        } else {
            &text[range.clone()]
        };
        let replacement = match action {
            Action::ClearEntry => "0".into(),
            Action::Negate => editing::negate(value),
            Action::Reciprocal => format!("1/({value})"),
            Action::Square => format!("({value})^2"),
            Action::Not => format!("~({value})"),
            Action::Function(name) => format!("{name}({value})"),
            _ => return,
        };
        self.core.input.set_focused(true);
        let start = range.start;
        self.core.input.set_selection(range);
        self.core.field.handle_event(
            frames.input,
            &ViewEvent::TextInput { text: replacement },
            context,
        );
        if matches!(action, Action::ClearEntry) {
            self.core.input.set_selection(start..start + 1);
        }
        let mut model = self.core.model.borrow_mut();
        model.evaluated = false;
        model.repeat_input = false;
        model.entry_cleared = false;
        model.result = None;
        model.error = None;
    }
    fn show_menu(&self, position: Point, row: Option<usize>, context: &mut EventContext<'_>) {
        let mut options = vec![
            ("Undo", Action::Undo),
            ("Redo", Action::Redo),
            ("Copy Result", Action::Copy),
            ("Copy Expression", Action::CopyExpression),
            ("Clear All", Action::Clear),
            ("Use This Mode at Startup", Action::SetDefault),
        ];
        if let Some(row) = row {
            options = vec![
                ("Copy Expression", Action::CopyHistory(row, false)),
                ("Copy Result", Action::CopyHistory(row, true)),
                ("Delete Calculation", Action::Delete(row)),
            ];
        } else {
            options.extend([
                ("Previous Answer", Action::Insert("Ans")),
                ("Open Parenthesis", Action::Insert("(")),
                ("Close Parenthesis", Action::Insert(")")),
            ]);
            if self.current_settings().mode != Mode::Programmer {
                options.push(("Reciprocal", Action::Reciprocal));
            }
            if self.current_settings().mode == Mode::Scientific {
                options.extend([
                    ("Inverse sine", Action::Function("asin")),
                    ("Inverse cosine", Action::Function("acos")),
                    ("Inverse tangent", Action::Function("atan")),
                    ("Exponential", Action::Function("exp")),
                    ("Absolute value", Action::Function("abs")),
                    ("Square", Action::Square),
                    ("Power", Action::Insert("^")),
                    ("Factorial", Action::Insert("!")),
                    ("π", Action::Insert("pi")),
                    ("e", Action::Insert("e")),
                ]);
            }
        }
        self.core
            .menu_id
            .set(self.core.menu_id.get().wrapping_add(1));
        *self.core.menu.borrow_mut() = options.iter().map(|(_, a)| *a).collect();
        context.show_context_menu(ContextMenuRequest {
            request_id: self.core.menu_id.get(),
            position,
            items: options
                .iter()
                .enumerate()
                .map(|(i, (label, a))| ContextMenuItem {
                    command_id: i as u32,
                    label: (*label).into(),
                    enabled: match a {
                        Action::Copy => self.core.model.borrow().result.is_some(),
                        Action::Undo => {
                            self.core.undo[index(self.current_settings().mode)].can_undo()
                        }
                        Action::Redo => {
                            self.core.undo[index(self.current_settings().mode)].can_redo()
                        }
                        _ => true,
                    },
                    checked: false,
                    destructive: matches!(a, Action::Delete(_) | Action::Clear),
                    separator: false,
                })
                .collect(),
        });
    }
    fn apply_action(&self, frames: &Frames, context: &mut EventContext<'_>) {
        let Some(action) = self.core.action.take() else {
            return;
        };
        let original = self.core.input.value();
        if self.core.target_editing.get() && self.current_settings().mode == Mode::Convert {
            let event = match action {
                Action::Insert(text) => Some(ViewEvent::TextInput { text: text.into() }),
                Action::Back => Some(ViewEvent::Backspace),
                Action::ClearEntry => {
                    self.core
                        .target
                        .set_selection(0..self.core.target.value().len());
                    Some(ViewEvent::TextInput { text: "0".into() })
                }
                Action::Negate => {
                    let value = self.core.target.value();
                    self.core.target.set_selection(0..value.len());
                    Some(ViewEvent::TextInput {
                        text: editing::negate(&value),
                    })
                }
                _ => None,
            };
            if let Some(event) = event {
                self.core.target.set_focused(true);
                self.core
                    .target_field
                    .handle_event(frames.result, &event, context);
                self.reverse_conversion();
                self.record_input(original);
                self.changed();
                context.request_redraw_in(frames.bounds);
                return;
            }
        }
        self.core.target_editing.set(false);
        self.core.target.set_focused(false);
        match action {
            Action::Undo | Action::Redo => {
                let slot = index(self.current_settings().mode);
                if matches!(action, Action::Undo) {
                    self.core.undo[slot].undo();
                } else {
                    self.core.undo[slot].redo();
                }
                let mut m = self.core.model.borrow_mut();
                m.evaluated = false;
                m.repeat_input = false;
                m.preview = None;
                m.result = None;
                m.error = None;
            }
            Action::More => self.show_menu(
                Point::new(
                    frames.toolbar_button(2).origin.x,
                    frames.toolbar.origin.y + frames.toolbar.size.height,
                ),
                None,
                context,
            ),
            Action::CopyExpression | Action::CopyHistory(_, _) => {
                let value = match action {
                    Action::CopyHistory(row, result) => {
                        self.core.model.borrow().history.get(row).map(|e| {
                            if result {
                                e.result.display(e.settings)
                            } else {
                                e.input.clone()
                            }
                        })
                    }
                    _ => Some(self.core.input.value()),
                };
                if let Some(value) = value {
                    if let Err(e) = preferences::copy(&value) {
                        self.core.model.borrow_mut().error = Some(e);
                    }
                }
            }
            Action::Delete(row) => {
                let mut model = self.core.model.borrow_mut();
                if row < model.history.len() {
                    model.history.remove(row);
                }
                model.scroll = 0.0;
                self.core.history_buttons.borrow_mut().clear();
                drop(model);
                self.persist_history();
            }
            Action::ClearEntry => {
                if self.core.model.borrow().evaluated || self.core.model.borrow().entry_cleared {
                    self.core.action.set(Some(Action::Clear));
                    self.apply_action(frames, context);
                    return;
                }
                self.edit_operand(action, frames, context);
                self.core.model.borrow_mut().entry_cleared = true;
            }
            Action::Function(_) => self.edit_operand(action, frames, context),
            Action::Insert(text) => {
                self.prepare_input(text);
                self.core.model.borrow_mut().result = None;
                self.core.input.set_focused(true);
                self.core.field.handle_event(
                    frames.input,
                    &ViewEvent::TextInput { text: text.into() },
                    context,
                );
            }
            Action::Clear => {
                self.core.input.clear();
                let mut model = self.core.model.borrow_mut();
                model.result = None;
                model.evaluated = false;
                model.repeat = [None; 4];
                model.repeat_input = false;
                model.entry_cleared = true;
                model.error = None;
                model.status = None;
                self.core.input.set_focused(true);
            }
            Action::Back => {
                self.core.input.set_focused(true);
                self.core
                    .field
                    .handle_event(frames.input, &ViewEvent::Backspace, context);
                let mut model = self.core.model.borrow_mut();
                model.evaluated = false;
                model.result = None;
                model.error = None;
            }
            Action::Equals => self.calculate(true),
            Action::Negate | Action::Reciprocal | Action::Square | Action::Not => {
                self.edit_operand(action, frames, context)
            }
            Action::Copy => {
                let settings = self.current_settings();
                let value = self.core.model.borrow().result.map(|v| v.display(settings));
                if let Some(value) = value {
                    let mut model = self.core.model.borrow_mut();
                    match preferences::copy(&value) {
                        Ok(()) => {
                            model.status = Some("Result copied".into());
                            model.error = None;
                        }
                        Err(error) => model.error = Some(error),
                    }
                }
            }
            Action::History => {
                if let Some(window) = self.core.history_window.take() {
                    close_window(window);
                } else {
                    self.core.history_window.set(Some(request_new_window()));
                }
            }
            Action::ClearHistory => {
                *self.core.menu.borrow_mut() = vec![Action::ConfirmClearHistory];
                self.core
                    .menu_id
                    .set(self.core.menu_id.get().wrapping_add(1));
                context.show_context_menu(ContextMenuRequest {
                    request_id: self.core.menu_id.get(),
                    position: frames.keys.origin,
                    items: vec![ContextMenuItem {
                        command_id: 0,
                        label: "Delete All Calculations?".into(),
                        enabled: true,
                        checked: false,
                        destructive: true,
                        separator: false,
                    }],
                });
            }
            Action::ConfirmClearHistory => {
                let mut model = self.core.model.borrow_mut();
                model.history.clear();
                model.history_writable = true;
                model.scroll = 0.0;
                self.core.history_buttons.borrow_mut().clear();
                drop(model);
                self.persist_history();
            }
            Action::Mode(mode) => {
                let mut model = self.core.model.borrow_mut();
                let old = index(model.settings.mode);
                model.drafts[old] = self.core.input.value();
                model.settings.mode = mode;
                self.core.input.set_value(model.drafts[index(mode)].clone());
                model.result = model.answers[index(mode)];
                model.evaluated = false;
                model.error = None;
                model.status = None;

                drop(model);
                self.refresh_controls();
            }
            Action::Base(base) | Action::Bits(base) => {
                let settings = self.current_settings();
                let parsed = engine::evaluate(
                    &self.core.input.value(),
                    settings,
                    self.core.model.borrow().answers[index(Mode::Programmer)],
                )
                .ok();
                let mut model = self.core.model.borrow_mut();
                if !self.core.input.value().is_empty() && parsed.is_none() {
                    model.error =
                        Some("Finish the expression before changing base or word size".into());
                    context.request_redraw_in(frames.bounds);
                    return;
                }
                if matches!(action, Action::Base(_)) {
                    model.settings.base = base;
                } else {
                    model.settings.bits = base;
                }
                if let Some(Value::Word(word)) = parsed {
                    let word = word & engine::mask(model.settings.bits);
                    model.result = Some(Value::Word(word));
                    self.core.input.set_value(engine::format_word(
                        word,
                        model.settings.base,
                        model.settings.bits,
                        false,
                    ));
                    model.answers[index(Mode::Programmer)] = Some(Value::Word(word));
                }
                model.error = None;
                model.evaluated = false;
                drop(model);
                self.refresh_controls();
            }
            Action::Category(category) => {
                let mut model = self.core.model.borrow_mut();
                model.category = category;
                model.from = 0;
                model.to = 1;

                drop(model);
                self.refresh_controls();
            }
            Action::From(unit) | Action::To(unit) => {
                let mut model = self.core.model.borrow_mut();
                if matches!(action, Action::From(_)) {
                    model.from = unit;
                } else {
                    model.to = unit;
                }

                drop(model);
                self.refresh_controls();
            }
            Action::Swap => {
                let mut model = self.core.model.borrow_mut();
                let from = model.from;
                model.from = model.to;
                model.to = from;
                if let Some(result) = model.result {
                    self.core.input.set_value(result.literal());
                }

                model.evaluated = false;
                drop(model);
                self.refresh_controls();
            }
            Action::SetDefault => {
                let mode = self.core.model.borrow().settings.mode;
                let result = self
                    .core
                    .path
                    .as_ref()
                    .ok_or_else(|| "No preferences location is available".into())
                    .and_then(|path| preferences::save(path, mode));
                let mut model = self.core.model.borrow_mut();
                match result {
                    Ok(()) => {
                        model.start_mode = mode;
                        model.status = Some(format!("{} will open at startup", mode.label()));
                        model.error = None;
                    }
                    Err(error) => model.error = Some(error),
                }
            }
            Action::Recall(row) => {
                let entry = self.core.model.borrow().history.get(row).cloned();
                if let Some(entry) = entry {
                    self.core.input.set_value(entry.input);
                    self.core
                        .degrees
                        .set(if entry.settings.degrees { 0 } else { 1 });
                    self.core.signed.set(entry.settings.signed);
                    let mut model = self.core.model.borrow_mut();
                    model.settings = entry.settings;
                    model.result = Some(entry.result);
                    model.answers[index(entry.settings.mode)] = Some(entry.result);
                    model.category = entry.units.0;
                    model.from = entry.units.1;
                    model.to = entry.units.2;
                    model.evaluated = true;
                    model.error = None;
                    model.repeat = [None; 4];
                    model.repeat_input = false;

                    drop(model);
                    self.refresh_controls();
                }
            }
        }
        if matches!(
            action,
            Action::Insert(_)
                | Action::Clear
                | Action::ClearEntry
                | Action::Function(_)
                | Action::Negate
                | Action::Square
                | Action::Reciprocal
                | Action::Not
                | Action::Back
        ) {
            self.record_input(original);
        }
        self.refresh_keys();
        self.changed();
        context.request_redraw_in(frames.bounds);
    }
}
struct Frames {
    bounds: Rect,
    toolbar: Rect,
    input: Rect,
    result: Rect,
    copy: Rect,
    context: Rect,
    keys: Rect,
    message: Rect,
}
impl Frames {
    fn new(bounds: Rect, theme: &Theme, mode: Mode) -> Self {
        let _ = theme;
        let x = bounds.origin.x + 12.0;
        let w = (bounds.size.width - 24.0).max(0.0);
        let toolbar = Rect::new(bounds.origin.x, bounds.origin.y, bounds.size.width, 40.0);
        let input = Rect::new(x, bounds.origin.y + 48.0, w, 32.0);
        let result = Rect::new(x, bounds.origin.y + 84.0, w, 48.0);
        let context = Rect::new(
            x,
            bounds.origin.y + 142.0,
            w,
            match mode {
                Mode::Convert => 68.0,
                Mode::Standard => 0.0,
                _ => 28.0,
            },
        );
        let y = context.origin.y + context.size.height + 12.0;
        let bottom = bounds.origin.y + bounds.size.height - 30.0;
        Self {
            bounds,
            toolbar,
            input,
            result,
            copy: Rect::new(x + w - 196.0, bounds.origin.y + 6.0, 52.0, 28.0),
            context,
            keys: Rect::new(x, y, w, (bottom - y).max(0.0)),
            message: Rect::new(x, bottom + 6.0, w, 20.0),
        }
    }
    fn toolbar_button(&self, index: usize) -> Rect {
        let x = self.toolbar.origin.x;
        let w = self.toolbar.size.width;
        let y = self.toolbar.origin.y + 6.0;
        match index {
            0 => Rect::new(x + 12.0, y, (w - 220.0).max(80.0), 28.0),
            1 => Rect::new(x + w - 148.0, y, 76.0, 28.0),
            _ => Rect::new(x + w - 68.0, y, 56.0, 28.0),
        }
    }
    fn key(&self, index: usize, count: usize, mode: Mode) -> Rect {
        let mut area = self.keys;
        let mut index = index;
        if mode == Mode::Scientific {
            let h = ((area.size.height - 7.0 * 6.0) / 7.0).max(0.0);
            if index < 6 {
                let w = ((area.size.width - 12.0) / 3.0).max(0.0);
                return Rect::new(
                    area.origin.x + (index % 3) as f32 * (w + 6.0),
                    area.origin.y + (index / 3) as f32 * (h + 6.0),
                    w,
                    h,
                );
            }
            index -= 6;
            area.origin.y += 2.0 * (h + 6.0);
            area.size.height = (area.size.height - 2.0 * (h + 6.0)).max(0.0);
        }
        let columns = if mode == Mode::Programmer { 6 } else { 4 };
        let rows = if columns == 6 { count.div_ceil(6) } else { 5 };
        let w = ((area.size.width - 6.0 * (columns - 1) as f32) / columns as f32).max(0.0);
        let h = ((area.size.height - 6.0 * (rows - 1) as f32) / rows as f32).max(0.0);
        let (slot, span) = if columns == 4 && index >= 16 {
            (
                index + usize::from(index > 16),
                if index == 16 { 2.0 } else { 1.0 },
            )
        } else {
            (index, 1.0)
        };
        Rect::new(
            area.origin.x + (slot % columns) as f32 * (w + 6.0),
            area.origin.y + (slot / columns) as f32 * (h + 6.0),
            w * span + 6.0 * (span - 1.0),
            h,
        )
    }
    fn angle(&self) -> Rect {
        Rect::new(
            self.context.origin.x,
            self.context.origin.y,
            self.context.size.width.min(200.0),
            28.0,
        )
    }
    fn units(&self, index: usize) -> Rect {
        let half = ((self.context.size.width - 76.0) / 2.0).max(0.0);
        match index {
            0 => Rect::new(
                self.context.origin.x,
                self.context.origin.y,
                200.0_f32.min(self.context.size.width),
                28.0,
            ),
            1 => Rect::new(
                self.context.origin.x,
                self.context.origin.y + 40.0,
                half,
                28.0,
            ),
            2 => Rect::new(
                self.context.origin.x + half + 76.0,
                self.context.origin.y + 40.0,
                half,
                28.0,
            ),
            _ => Rect::new(
                self.context.origin.x + half + 8.0,
                self.context.origin.y + 40.0,
                60.0,
                28.0,
            ),
        }
    }
}
impl View for CalculatorView {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(c.maximum)
    }
    fn paint(&self, bounds: Rect, context: &mut PaintContext<'_>) {
        let settings = self.current_settings();
        let needs_preview = {
            let m = self.core.model.borrow();
            !self.history_only
                && !m.evaluated
                && !self.core.target_editing.get()
                && m.preview.as_ref()
                    != Some(&(self.core.input.value(), settings, m.category, m.from, m.to))
        };
        if needs_preview {
            self.calculate(false);
        }
        let model = self.core.model.borrow();
        let mut frames = Frames::new(bounds, context.theme, settings.mode);
        if self.history_only {
            frames.keys = Rect::new(
                bounds.origin.x + 12.0,
                bounds.origin.y + 48.0,
                (bounds.size.width - 24.0).max(0.0),
                (bounds.size.height - 84.0).max(0.0),
            );
        }
        Rectangle::new()
            .color(RectangleColor::Custom(context.theme.colors.background))
            .paint(bounds, context);
        let _ = self.core.revision.get();
        if !self.history_only {
            self.core
                .pickers
                .borrow()
                .mode
                .paint(frames.toolbar_button(0), context);
            self.core.history.paint(frames.toolbar_button(1), context);
            self.core
                .default_mode
                .paint(frames.toolbar_button(2), context);
            self.core.field.paint(frames.input, context);
            let label = model
                .result
                .map(|value| value.display(settings))
                .unwrap_or_else(|| {
                    if self.core.input.value().is_empty() {
                        "0".into()
                    } else {
                        "—".into()
                    }
                });
            let approximate = model.result.is_some_and(Value::approximate);
            let binary_lines =
                settings.mode == Mode::Programmer && settings.base == 2 && label.len() > 34;
            let shown = if binary_lines {
                let digits = label.strip_prefix("0b").unwrap_or(&label);
                let split = digits.len().saturating_sub(32);
                format!("0b{}\n{}", &digits[..split], &digits[split..])
            } else {
                let label = if settings.mode == Mode::Programmer {
                    label
                } else {
                    grouped(&label)
                };
                format!("{}{label}", if approximate { "≈ " } else { "" })
            };
            let role = TextRole::TitleLarge;
            if settings.mode == Mode::Convert {
                self.core.target_field.paint(frames.result, context);
            } else {
                let text = Text::styled(shown, role).alignment(TextAlignment::End);
                let measured = text.measure_unbounded(context.text_measurer);
                let ratio = (frames.result.size.width / measured.width.max(1.0)).min(1.0);
                let size = context.typography.style(role).size * ratio;
                context.display_list.push(DrawCommand::PushClip {
                    rect: frames.result,
                });
                let size = size.max(context.typography.style(TextRole::Caption).size);
                let line_height = if binary_lines {
                    frames.result.size.height / 2.0
                } else {
                    context.typography.style(role).line_height
                };
                text.font_size(size)
                    .line_height(line_height)
                    .paint(frames.result, context);
                context.display_list.push(DrawCommand::PopClip);
            }
            self.core.copy.paint(frames.copy, context);
            match settings.mode {
                Mode::Scientific => self.core.angle.paint(frames.angle(), context),
                Mode::Programmer => {
                    let pickers = self.core.pickers.borrow();
                    pickers.base.paint(
                        Rect::new(frames.context.origin.x, frames.context.origin.y, 92.0, 28.0),
                        context,
                    );
                    pickers.bits.paint(
                        Rect::new(
                            frames.context.origin.x + 104.0,
                            frames.context.origin.y,
                            108.0,
                            28.0,
                        ),
                        context,
                    );
                    self.core.signed_control.paint(
                        Rect::new(
                            frames.context.origin.x + 224.0,
                            frames.context.origin.y,
                            (frames.context.size.width - 224.0).max(0.0),
                            28.0,
                        ),
                        context,
                    );
                }
                Mode::Convert => {
                    let pickers = self.core.pickers.borrow();
                    pickers.category.paint(frames.units(0), context);
                    pickers.from.paint(frames.units(1), context);
                    pickers.to.paint(frames.units(2), context);
                    self.core.swap.paint(frames.units(3), context);
                }
                _ => {}
            }
        }
        if self.history_only {
            self.core.clear_history.paint(
                Rect::new(
                    frames.keys.origin.x + frames.keys.size.width - 120.0,
                    frames.keys.origin.y,
                    120.0,
                    28.0,
                ),
                context,
            );
            Text::styled("History", TextRole::TitleSmall).paint(
                Rect::new(
                    frames.keys.origin.x,
                    frames.keys.origin.y,
                    (frames.keys.size.width - 136.0).max(0.0),
                    28.0,
                ),
                context,
            );
            let body = Rect::new(
                frames.keys.origin.x,
                frames.keys.origin.y + 40.0,
                frames.keys.size.width,
                (frames.keys.size.height - 40.0).max(0.0),
            );
            context
                .display_list
                .push(DrawCommand::PushClip { rect: body });
            if model.history.is_empty() {
                Text::body("Your calculations will appear here.")
                    .color(context.theme.colors.text_secondary)
                    .paint(body, context);
            }
            let start = (model.scroll / 72.0).floor() as usize;
            for (visual, entry) in model
                .history
                .iter()
                .rev()
                .enumerate()
                .skip(start)
                .take((body.size.height / 72.0).ceil() as usize + 1)
            {
                let index = model.history.len() - 1 - visual;
                let frame = Rect::new(
                    body.origin.x,
                    body.origin.y + visual as f32 * 72.0 - model.scroll,
                    body.size.width,
                    64.0,
                );
                self.history_button(index, entry).paint(frame, context);
            }
            context.display_list.push(DrawCommand::PopClip);
        } else {
            let keys = self.core.keys.borrow();
            for (i, key) in keys.iter().enumerate() {
                key.button
                    .paint(frames.key(i, keys.len(), settings.mode), context);
            }
        }
        let message = model
            .error
            .clone()
            .or_else(|| model.status.clone())
            .unwrap_or_default();
        Text::caption(message)
            .color(if model.error.is_some() {
                context.theme.colors.destructive
            } else {
                context.theme.colors.text_secondary
            })
            .paint(frames.message, context);
    }
    fn handle_event(
        &self,
        bounds: Rect,
        event: &ViewEvent,
        context: &mut EventContext<'_>,
    ) -> EventResult {
        let mode = self.core.model.borrow().settings.mode;
        let mut frames = Frames::new(bounds, context.theme(), mode);
        if self.history_only {
            frames.keys = Rect::new(
                bounds.origin.x + 12.0,
                bounds.origin.y + 48.0,
                (bounds.size.width - 24.0).max(0.0),
                (bounds.size.height - 84.0).max(0.0),
            );
        }
        if let ViewEvent::ContextMenuResult {
            request_id,
            command_id,
        } = event
        {
            if *request_id == self.core.menu_id.get() {
                let action =
                    command_id.and_then(|id| self.core.menu.borrow().get(id as usize).copied());
                self.core.menu.borrow_mut().clear();
                self.core.action.set(action);
                self.apply_action(&frames, context);
                return EventResult::Consumed;
            }
        }
        if let ViewEvent::PointerPressed {
            position,
            button: appcore::viewkit::platform::PointerButton::Secondary,
        } = event
        {
            let row = if self.history_only && frames.keys.contains(*position) {
                let model = self.core.model.borrow();
                let visual =
                    ((position.y - frames.keys.origin.y - 40.0 + model.scroll) / 72.0).floor();
                if visual >= 0.0 {
                    model.history.len().checked_sub(visual as usize + 1)
                } else {
                    None
                }
            } else {
                None
            };
            self.show_menu(*position, row, context);
            return EventResult::Consumed;
        }
        if self.history_only {
            return self.history_event(&frames, event, context);
        }
        if mode == Mode::Convert && !self.history_only {
            if let ViewEvent::PointerPressed { position, .. } = event {
                if frames.input.contains(*position) {
                    self.core.target_editing.set(false);
                    self.core.target.set_focused(false);
                } else if frames.result.contains(*position) {
                    self.core.target_editing.set(true);
                    self.core.input.set_focused(false);
                }
            }
            let source_before = self.core.input.value();
            let before = self.core.target.value();
            let result = self
                .core
                .target_field
                .handle_event(frames.result, event, context);
            if self.core.target.value() != before {
                self.reverse_conversion();
                self.record_input(source_before);
                self.changed();
                context.request_redraw_in(bounds);
                return EventResult::Consumed;
            }
            if result == EventResult::Consumed {
                return result;
            }
        }
        if let ViewEvent::KeyPressed { key, modifiers } = event {
            let action = match key {
                Key::Character('z' | 'Z') if modifiers.shortcut() => Some(
                    if modifiers.bits() & appcore::viewkit::platform::KeyModifiers::SHIFT != 0 {
                        Action::Redo
                    } else {
                        Action::Undo
                    },
                ),
                Key::Character('y' | 'Y') if modifiers.shortcut() => Some(Action::Redo),
                Key::Enter => Some(Action::Equals),
                Key::Character('1') if modifiers.shortcut() => Some(Action::Mode(Mode::Standard)),
                Key::Character('2') if modifiers.shortcut() => Some(Action::Mode(Mode::Scientific)),
                Key::Character('3') if modifiers.shortcut() => Some(Action::Mode(Mode::Convert)),
                Key::Character('4') if modifiers.shortcut() => Some(Action::Mode(Mode::Programmer)),
                Key::Character('h' | 'H') if modifiers.shortcut() => Some(Action::History),
                Key::Escape => Some(Action::Clear),
                Key::Character('c' | 'C')
                    if modifiers.shortcut()
                        && modifiers.bits() & appcore::viewkit::platform::KeyModifiers::SHIFT
                            != 0 =>
                {
                    Some(Action::Copy)
                }
                _ => None,
            };
            if let Some(action) = action {
                self.core.action.set(Some(action));
                self.apply_action(&frames, context);
                return EventResult::Consumed;
            }
        }
        let before = self.core.input.value();
        if let ViewEvent::TextInput { text } = event {
            if text == "=" {
                self.core.action.set(Some(Action::Equals));
                self.apply_action(&frames, context);
                return EventResult::Consumed;
            }
            self.prepare_input(text);
        }
        let mut result = if self.history_only {
            EventResult::Ignored
        } else {
            self.core.field.handle_event(frames.input, event, context)
        };
        {
            let pickers = self.core.pickers.borrow();
            result = result.merge(pickers.mode.handle_event(
                frames.toolbar_button(0),
                event,
                context,
            ));
            match mode {
                Mode::Programmer => {
                    result = result
                        .merge(pickers.base.handle_event(
                            Rect::new(frames.context.origin.x, frames.context.origin.y, 92.0, 28.0),
                            event,
                            context,
                        ))
                        .merge(pickers.bits.handle_event(
                            Rect::new(
                                frames.context.origin.x + 104.0,
                                frames.context.origin.y,
                                108.0,
                                28.0,
                            ),
                            event,
                            context,
                        ))
                        .merge(self.core.signed_control.handle_event(
                            Rect::new(
                                frames.context.origin.x + 224.0,
                                frames.context.origin.y,
                                (frames.context.size.width - 224.0).max(0.0),
                                28.0,
                            ),
                            event,
                            context,
                        ));
                }
                Mode::Scientific => {
                    result =
                        result.merge(self.core.angle.handle_event(frames.angle(), event, context))
                }
                Mode::Convert => {
                    result = result
                        .merge(
                            pickers
                                .category
                                .handle_event(frames.units(0), event, context),
                        )
                        .merge(pickers.from.handle_event(frames.units(1), event, context))
                        .merge(pickers.to.handle_event(frames.units(2), event, context))
                        .merge(self.core.swap.handle_event(frames.units(3), event, context));
                }
                _ => {}
            }
        }
        result = result
            .merge(
                self.core
                    .history
                    .handle_event(frames.toolbar_button(1), event, context),
            )
            .merge(
                self.core
                    .default_mode
                    .handle_event(frames.toolbar_button(2), event, context),
            );
        result = result.merge(self.core.copy.handle_event(frames.copy, event, context));
        let keys = self.core.keys.borrow();
        for (i, key) in keys.iter().enumerate() {
            result = result.merge(key.button.handle_event(
                frames.key(i, keys.len(), mode),
                event,
                context,
            ));
        }
        drop(keys);
        if self.core.input.value() != before {
            self.record_input(before);
            let mut model = self.core.model.borrow_mut();
            model.evaluated = false;
            model.entry_cleared = false;
            model.result = None;
            model.error = None;
            model.status = None;
            drop(model);
            self.refresh_keys();
            context.request_redraw_in(bounds);
        }
        if self.core.action.get().is_some() {
            self.apply_action(&frames, context);
            result = EventResult::Consumed;
        }
        result
    }
}
impl CalculatorView {
    fn record_input(&self, before: String) {
        let after = self.core.input.value();
        if before == after {
            return;
        }
        let undo = self.core.input.clone();
        let redo = self.core.input.clone();
        self.core.undo[index(self.current_settings().mode)].register(
            "Edit Calculation",
            move || undo.set_value(before.clone()),
            move || redo.set_value(after.clone()),
        );
    }
    fn history_event(
        &self,
        frames: &Frames,
        event: &ViewEvent,
        context: &mut EventContext<'_>,
    ) -> EventResult {
        let mut result = EventResult::Ignored;
        result = result.merge(self.core.clear_history.handle_event(
            Rect::new(
                frames.keys.origin.x + frames.keys.size.width - 120.0,
                frames.keys.origin.y,
                120.0,
                28.0,
            ),
            event,
            context,
        ));
        let body = Rect::new(
            frames.keys.origin.x,
            frames.keys.origin.y + 40.0,
            frames.keys.size.width,
            (frames.keys.size.height - 40.0).max(0.0),
        );
        let model = self.core.model.borrow();
        for (visual, entry) in model
            .history
            .iter()
            .rev()
            .enumerate()
            .skip((model.scroll / 72.0).floor() as usize)
            .take((body.size.height / 72.0).ceil() as usize + 1)
        {
            let index = model.history.len() - 1 - visual;
            let frame = Rect::new(
                body.origin.x,
                body.origin.y + visual as f32 * 72.0 - model.scroll,
                body.size.width,
                64.0,
            );
            let leave = ViewEvent::PointerLeft;
            let row_event = if event.position().is_some_and(|p| !body.contains(p)) {
                &leave
            } else {
                event
            };
            result = result.merge(
                self.history_button(index, entry)
                    .handle_event(frame, row_event, context),
            );
        }
        let maximum = (model.history.len() as f32 * 72.0 - body.size.height).max(0.0);
        drop(model);
        if let ViewEvent::Scroll {
            position, delta_y, ..
        } = event
        {
            if body.contains(*position) {
                let mut model = self.core.model.borrow_mut();
                model.scroll = (model.scroll - *delta_y * 36.0).clamp(0.0, maximum);
                context.request_redraw_in(body);
                result = EventResult::Consumed;
            }
        }
        if self.core.action.get().is_some() {
            self.apply_action(frames, context);
            result = EventResult::Consumed;
        }
        result
    }
    fn prepare_input(&self, text: &str) {
        let mut model = self.core.model.borrow_mut();
        if model.evaluated {
            let continuation = matches!(
                text.trim(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "^"
                    | "%"
                    | "!"
                    | "and"
                    | "or"
                    | "xor"
                    | "<<"
                    | ">>"
                    | "mod"
            );
            self.core
                .input
                .set_value(if continuation { "Ans" } else { "" });
            model.repeat_input = !continuation;
            model.evaluated = false;
            model.result = None;
        }
        model.entry_cleared = false;
        model.error = None;
        model.status = None;
    }
    fn history_button(&self, index: usize, entry: &History) -> Button {
        let state = self
            .core
            .history_buttons
            .borrow_mut()
            .entry(index)
            .or_default()
            .clone();
        let action = self.core.action.clone();
        Button::with_interaction_and_label(state, "")
            .content(HistoryRow(entry.clone()))
            .style(ButtonStyle::Ghost)
            .accessibility_label(format!(
                "{} = {}",
                entry.input,
                entry.result.display(entry.settings)
            ))
            .on_click(move || action.set(Some(Action::Recall(index))))
    }
}
struct HistoryRow(History);
impl View for HistoryRow {
    fn measure(&self, c: Constraints, _: &mut MeasureContext<'_>) -> Size {
        c.constrain(Size::new(c.maximum.width, 56.0))
    }
    fn paint(&self, b: Rect, c: &mut PaintContext<'_>) {
        Text::caption(format!(
            "{} · {}",
            self.0.settings.mode.label(),
            self.0.input
        ))
        .color(c.theme.colors.text_secondary)
        .paint(
            Rect::new(
                b.origin.x + 4.0,
                b.origin.y + 4.0,
                (b.size.width - 8.0).max(0.0),
                20.0,
            ),
            c,
        );
        Text::styled(self.0.result.display(self.0.settings), TextRole::TitleSmall)
            .alignment(TextAlignment::End)
            .paint(
                Rect::new(
                    b.origin.x + 4.0,
                    b.origin.y + 28.0,
                    (b.size.width - 8.0).max(0.0),
                    24.0,
                ),
                c,
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Number;
    use appcore::viewkit::draw_command::DisplayList;
    use appcore::viewkit::platform::{KeyModifiers, PointerButton};
    use appcore::viewkit::typography::TextMeasurer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture {
        view: CalculatorView,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                view: CalculatorView::with_preferences(None, Mode::Standard, None),
            }
        }
        fn bounds() -> Rect {
            Rect::new(0.0, 0.0, 360.0, 540.0)
        }
        fn event(&self, event: ViewEvent) {
            let theme = Theme::current();
            let mut measurer = TextMeasurer::new();
            let mut context = EventContext::new(&theme, &theme.typography, &mut measurer);
            self.view.handle_event(Self::bounds(), &event, &mut context);
        }
        fn paint(&self) {
            let theme = Theme::current();
            let mut measurer = TextMeasurer::new();
            let mut list = DisplayList::new();
            self.view.paint(
                Self::bounds(),
                &mut PaintContext::new(&mut list, &theme, &theme.typography, &mut measurer),
            );
        }
        fn click(&self, frame: Rect) {
            self.paint();
            let position = Point::new(
                frame.origin.x + frame.size.width / 2.0,
                frame.origin.y + frame.size.height / 2.0,
            );
            self.event(ViewEvent::PointerPressed {
                position,
                button: PointerButton::Primary,
            });
            self.event(ViewEvent::PointerReleased {
                position,
                button: PointerButton::Primary,
            });
        }
        fn key(&self, label: &str) {
            let model = self.view.core.model.borrow();
            let mode = model.settings.mode;
            let specs = key_specs(mode);
            let index = specs.iter().position(|(text, _)| *text == label).unwrap();
            let frames = Frames::new(Self::bounds(), &Theme::current(), mode);
            drop(model);
            self.click(frames.key(index, specs.len(), mode));
        }
        fn enter(&self) {
            self.event(ViewEvent::KeyPressed {
                key: Key::Enter,
                modifiers: KeyModifiers::default(),
            });
        }
        fn type_text(&self, text: &str) {
            self.event(ViewEvent::TextInput { text: text.into() });
        }
        fn mode(&self, n: char) {
            self.event(ViewEvent::KeyPressed {
                key: Key::Character(n),
                modifiers: KeyModifiers::from_bits(KeyModifiers::CONTROL),
            });
        }
        fn result(&self) -> String {
            self.view
                .core
                .model
                .borrow()
                .result
                .unwrap()
                .display(self.view.current_settings())
        }
        fn action(&self, action: Action) {
            let frames = Frames::new(
                Self::bounds(),
                &Theme::current(),
                self.view.core.model.borrow().settings.mode,
            );
            let theme = Theme::current();
            let mut measurer = TextMeasurer::new();
            let mut context = EventContext::new(&theme, &theme.typography, &mut measurer);
            self.view.core.action.set(Some(action));
            self.view.apply_action(&frames, &mut context);
        }
    }
    #[test]
    fn clicked_keys_enter_and_exact_result_continuation() {
        let f = Fixture::new();
        for key in ["0", ".", "1", "+", "0", ".", "2"] {
            f.key(key);
        }
        f.enter();
        assert_eq!(f.result(), "0.3");
        assert_eq!(f.view.core.model.borrow().history.len(), 1);
        f.key("×");
        f.key("1");
        f.key("0");
        f.enter();
        assert_eq!(f.result(), "3");
        f.key("7");
        f.enter();
        assert_eq!(f.result(), "70");
    }
    #[test]
    fn keyboard_errors_can_be_corrected_without_losing_input() {
        let f = Fixture::new();
        f.type_text("1/0");
        f.enter();
        assert!(f.view.core.model.borrow().error.is_some());
        assert_eq!(f.view.core.input.value(), "1/0");
        f.event(ViewEvent::Backspace);
        f.type_text("2");
        f.enter();
        assert_eq!(f.result(), "0.5");
        assert!(f.view.core.model.borrow().error.is_none());
    }
    #[test]
    fn mode_drafts_history_recall_and_scientific_angles() {
        let f = Fixture::new();
        f.type_text("(2+3)");
        f.mode('2');
        f.type_text("sin(30)");
        f.enter();
        assert_eq!(f.result(), "0.5");
        f.mode('1');
        assert_eq!(f.view.core.input.value(), "(2+3)");
        f.enter();
        assert_eq!(f.result(), "5");
        f.action(Action::Recall(0));
        assert_eq!(f.view.core.model.borrow().settings.mode, Mode::Scientific);
        assert_eq!(f.result(), "0.5");
        assert_eq!(f.view.core.degrees.get(), 0);
        f.action(Action::ConfirmClearHistory);
        assert!(f.view.core.model.borrow().history.is_empty());
        let frames = Frames::new(Fixture::bounds(), &Theme::current(), Mode::Scientific);
        let angle = frames.angle();
        f.click(Rect::new(
            angle.origin.x + 140.0,
            angle.origin.y,
            20.0,
            angle.size.height,
        ));
        assert_eq!(f.view.core.degrees.get(), 1);
        f.action(Action::Clear);
        f.type_text("sin(pi/2)");
        f.enter();
        assert_eq!(f.result(), "1");
    }
    #[test]
    fn programmer_keys_filter_digits_and_reformat_without_reinterpreting_values() {
        let f = Fixture::new();
        f.mode('4');
        f.action(Action::Bits(8));
        f.action(Action::Base(16));
        f.key("F");
        f.key("F");
        f.enter();
        assert_eq!(f.result(), "0xFF");
        f.action(Action::Base(10));
        assert_eq!(f.result(), "255");
        f.view.core.signed.set(true);
        assert_eq!(f.result(), "-1");
        f.action(Action::Base(2));
        f.action(Action::Clear);
        f.key("7");
        assert!(f.view.core.input.value().is_empty());
        f.key("1");
        f.enter();
        assert_eq!(f.result(), "0b1");
    }
    #[test]
    fn converters_update_from_native_input_and_unit_choices() {
        let f = Fixture::new();
        f.mode('3');
        f.type_text("1");
        f.paint();
        assert_eq!(f.result(), "100");
        f.action(Action::Category(2));
        f.action(Action::From(1));
        f.action(Action::To(0));
        f.action(Action::Clear);
        f.type_text("32");
        f.paint();
        assert_eq!(f.result(), "0");
        f.enter();
        assert_eq!(f.view.core.model.borrow().history.len(), 1);
        f.action(Action::Swap);
        f.paint();
        assert_eq!(f.result(), "32");
    }
    #[test]
    fn chosen_start_mode_is_persisted_only_on_explicit_request() {
        let directory = std::env::temp_dir().join(format!(
            "calculator-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("mode");
        let f = Fixture {
            view: CalculatorView::with_preferences(Some(path.clone()), Mode::Standard, None),
        };
        f.mode('2');
        assert!(!path.exists());
        f.action(Action::SetDefault);
        assert_eq!(preferences::load(&path).unwrap(), Mode::Scientific);
        assert_eq!(f.view.core.model.borrow().start_mode, Mode::Scientific);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn all_keypad_frames_are_contained_and_the_existing_icon_decodes() {
        let theme = Theme::current();
        for mode in Mode::ALL {
            for width in [360.0, 480.0, 600.0, 900.0] {
                let bounds = Rect::new(0.0, 0.0, width, 690.0);
                let frames = Frames::new(bounds, &theme, mode);
                let keys = key_specs(mode);
                for i in 0..keys.len() {
                    let key = frames.key(i, keys.len(), mode);
                    assert!(key.origin.x >= 0.0 && key.origin.y >= 0.0);
                    assert!(key.origin.x + key.size.width <= width + 0.01);
                    assert!(key.origin.y + key.size.height <= 690.0);
                }
            }
        }
        assert!(SvgData::decode(include_bytes!("../appicon.svg")).is_ok());
    }
    #[test]
    fn repeated_equals_percent_and_operand_edits_match_keypad_intent() {
        let f = Fixture::new();
        f.type_text("1*2");
        f.enter();
        f.enter();
        assert_eq!(f.result(), "4");
        f.enter();
        assert_eq!(f.result(), "8");
        f.action(Action::Clear);
        f.type_text("100+15%");
        f.enter();
        f.type_text("150");
        f.enter();
        assert_eq!(f.result(), "172.5");
        f.action(Action::Clear);
        f.type_text("12+3");
        f.key("±");
        f.enter();
        assert_eq!(f.result(), "9");
        f.action(Action::Clear);
        f.type_text("12+34");
        f.key("C");
        assert_eq!(f.view.core.input.value(), "12+0");
        f.key("5");
        f.enter();
        assert_eq!(f.result(), "17");
        f.key("C");
        assert!(f.view.core.input.value().is_empty());
        f.mode('2');
        f.type_text("30");
        f.key("sin");
        f.enter();
        assert_eq!(f.result(), "0.5");
        f.action(Action::Undo);
        assert_eq!(f.view.core.input.value(), "30");
        f.action(Action::Redo);
        assert_eq!(f.view.core.input.value(), "sin(30)");
    }
    #[test]
    fn conversion_can_be_edited_in_reverse_and_swapped_without_changing_quantity() {
        let f = Fixture::new();
        f.mode('3');
        f.type_text("1");
        f.paint();
        let frames = Frames::new(Fixture::bounds(), &Theme::current(), Mode::Convert);
        f.click(frames.result);
        f.view.core.target.set_selection(0..3);
        f.type_text("250");
        assert_eq!(f.view.core.input.value(), "(5/2)");
        assert_eq!(f.result(), "250");
        f.view.core.target.set_selection(0..3);
        f.key("5");
        assert_eq!(f.result(), "5");
        assert_eq!(f.view.core.input.value(), "(1/20)");
        f.view.core.target.set_selection(0..1);
        f.type_text("250");
        f.action(Action::Swap);
        f.paint();
        assert_eq!(f.result(), "2.5");
        assert_eq!(f.view.core.input.value(), "250");
    }
    #[test]
    fn history_survives_restart_keeps_exact_answers_and_stays_independent_of_keypad() {
        let directory = std::env::temp_dir().join(format!(
            "calculator-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("mode");
        let f = Fixture {
            view: CalculatorView::with_preferences(Some(path.clone()), Mode::Standard, None),
        };
        f.type_text("1/3");
        f.enter();
        f.type_text("*");
        f.type_text("3");
        f.enter();
        assert_eq!(f.result(), "1");
        let restored = CalculatorView::with_preferences(Some(path.clone()), Mode::Standard, None);
        assert_eq!(restored.core.model.borrow().history.len(), 2);
        assert_eq!(
            restored.core.model.borrow().history[1].result,
            Some(Value::Real(Number::ONE)).unwrap()
        );
        let history = f.view.history_view();
        let before = f.view.core.input.value();
        let theme = Theme::current();
        let mut measurer = TextMeasurer::new();
        let mut ctx = EventContext::new(&theme, &theme.typography, &mut measurer);
        history.handle_event(
            Fixture::bounds(),
            &ViewEvent::TextInput { text: "999".into() },
            &mut ctx,
        );
        assert_eq!(f.view.core.input.value(), before);
        f.type_text("7");
        f.enter();
        assert_eq!(f.result(), "21");
        f.action(Action::Delete(0));
        assert_eq!(history::load(&path).unwrap().len(), 2);
        f.action(Action::ClearHistory);
        assert_eq!(history::load(&path).unwrap().len(), 2);
        f.action(Action::ConfirmClearHistory);
        assert!(history::load(&path).unwrap().is_empty());
        std::fs::write(path.with_file_name("history-v1"), b"damaged").unwrap();
        let bad = Fixture {
            view: CalculatorView::with_preferences(Some(path.clone()), Mode::Standard, None),
        };
        bad.type_text("2+2");
        bad.enter();
        assert_eq!(bad.result(), "4");
        assert_eq!(
            std::fs::read(path.with_file_name("history-v1")).unwrap(),
            b"damaged"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn compact_keys_do_not_overlap_and_decimal_grouping_preserves_fraction_and_exponent() {
        assert_eq!(grouped("-1234567.125"), "-1,234,567.125");
        assert_eq!(grouped("1.2e100"), "1.2e100");
        for mode in Mode::ALL {
            let frames = Frames::new(Fixture::bounds(), &Theme::current(), mode);
            let count = key_specs(mode).len();
            for i in 0..count {
                let a = frames.key(i, count, mode);
                assert!(a.size.height >= 32.0);
                assert!(a.origin.y + a.size.height <= 540.0);
                for j in i + 1..count {
                    let b = frames.key(j, count, mode);
                    let overlaps = a.origin.x < b.origin.x + b.size.width
                        && b.origin.x < a.origin.x + a.size.width
                        && a.origin.y < b.origin.y + b.size.height
                        && b.origin.y < a.origin.y + a.size.height;
                    assert!(!overlaps, "{mode:?}: {i}/{j}");
                }
            }
        }
    }
    #[test]
    fn preview_copy_writes_the_shared_clipboard_slot() {
        if std::env::var_os("CALCULATOR_COPY_CHILD").is_some() {
            let f = Fixture::new();
            f.type_text("0.1+0.2");
            f.enter();
            f.action(Action::Copy);
            assert_eq!(
                f.view.core.model.borrow().result,
                Some(Value::Real(Number::fraction(3, 10).unwrap()))
            );
            let root = std::env::var_os("MOCHIOS_PREVIEW_CONFIG_ROOT").unwrap();
            assert_eq!(
                std::fs::read_to_string(PathBuf::from(root).join("clipboard.txt")).unwrap(),
                "0.3"
            );
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "calculator-copy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::tests::preview_copy_writes_the_shared_clipboard_slot",
            ])
            .env("CALCULATOR_COPY_CHILD", "1")
            .env("MOCHIOS_PREVIEW_CONFIG_ROOT", &directory)
            .status()
            .unwrap();
        let _ = std::fs::remove_dir_all(directory);
        assert!(status.success());
    }
}
