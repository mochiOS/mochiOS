use crate::engine::{self, Mode, Settings, Value};
use crate::{preferences, units};
use appcore::prelude::*;
use appcore::viewkit::draw_command::DrawCommand;
use appcore::viewkit::event::{EventContext, EventResult, ViewEvent};
use appcore::viewkit::platform::Key;
use appcore::viewkit::view::{Constraints, MeasureContext, PaintContext};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Action {
    Insert(&'static str),
    Clear,
    Back,
    Equals,
    Negate,
    Reciprocal,
    Square,
    Not,
    Copy,
    History,
    ClearHistory,
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
#[derive(Clone)]
struct History {
    input: String,
    result: Value,
    settings: Settings,
    units: (usize, usize, usize),
}
struct Model {
    settings: Settings,
    start_mode: Mode,
    result: Option<Value>,
    answers: [Option<Value>; 4],
    drafts: [String; 4],
    evaluated: bool,
    error: Option<String>,
    status: Option<String>,
    history: Vec<History>,
    history_visible: bool,
    scroll: f32,
    category: usize,
    from: usize,
    to: usize,
    conversion_input: Option<String>,
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
}
fn index(mode: Mode) -> usize {
    match mode {
        Mode::Standard => 0,
        Mode::Scientific => 1,
        Mode::Convert => 2,
        Mode::Programmer => 3,
    }
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
    use Action::{Back, Clear, Copy, Equals, Insert, Negate, Not, Reciprocal, Square};
    if mode == Mode::Scientific {
        return vec![
            ("AC", Clear),
            ("(", Insert("(")),
            (")", Insert(")")),
            ("Back", Back),
            ("±", Negate),
            ("÷", Insert("/")),
            ("sin", Insert("sin(")),
            ("cos", Insert("cos(")),
            ("tan", Insert("tan(")),
            ("asin", Insert("asin(")),
            ("acos", Insert("acos(")),
            ("atan", Insert("atan(")),
            ("ln", Insert("ln(")),
            ("log", Insert("log(")),
            ("√", Insert("sqrt(")),
            ("x²", Square),
            ("xʸ", Insert("^")),
            ("n!", Insert("!")),
            ("7", Insert("7")),
            ("8", Insert("8")),
            ("9", Insert("9")),
            ("π", Insert("pi")),
            ("e", Insert("e")),
            ("×", Insert("*")),
            ("4", Insert("4")),
            ("5", Insert("5")),
            ("6", Insert("6")),
            ("1/x", Reciprocal),
            ("%", Insert("%")),
            ("−", Insert("-")),
            ("1", Insert("1")),
            ("2", Insert("2")),
            ("3", Insert("3")),
            ("exp", Insert("exp(")),
            ("Ans", Insert("Ans")),
            ("+", Insert("+")),
            ("0", Insert("0")),
            (".", Insert(".")),
            ("00", Insert("00")),
            ("abs", Insert("abs(")),
            ("Copy", Copy),
            ("=", Equals),
        ];
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
        ("AC", Clear),
        ("(", Insert("(")),
        (")", Insert(")")),
        ("Back", Back),
        ("%", Insert("%")),
        ("±", Negate),
        ("1/x", Reciprocal),
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
        ("Ans", Insert("Ans")),
        ("=", Equals),
    ]
}
fn keys(model: &Model, action: &Rc<Cell<Option<Action>>>) -> Vec<KeyButton> {
    key_specs(model.settings.mode)
        .into_iter()
        .map(|(label, a)| {
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
            let mut button = action_button(label, a, action)
                .size(ButtonSize::Large)
                .style(if matches!(a, Action::Equals) {
                    ButtonStyle::Accent
                } else {
                    ButtonStyle::Standard
                })
                .enabled(enabled);
            if matches!(a, Action::Back) {
                button = button.accessibility_label("Delete previous character");
            }
            if matches!(a, Action::Negate) {
                button = button.accessibility_label("Negate expression");
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
            error,
            status: None,
            history: vec![],
            history_visible: false,
            scroll: 0.0,
            category: 0,
            from: 0,
            to: 1,
            conversion_input: None,
        };
        let action = Rc::new(Cell::new(None));
        let input = TextFieldInteractionState::new();
        input.set_focused(true);
        let submit = action.clone();
        let field = TextField::with_interaction(input.clone())
            .placeholder("Enter an expression")
            .on_submit(move || submit.set(Some(Action::Equals)));
        let degrees = State::new(0);
        let angle = SegmentedControl::new(degrees.binding())
            .item(0, "Degrees")
            .item(1, "Radians");
        let signed = State::new(false);
        let signed_control = Checkbox::new(signed.binding()).label("Signed");
        let pickers = RefCell::new(pickers(&model, &action));
        let keys = RefCell::new(keys(&model, &action));
        let copy = action_button("Copy Result", Action::Copy, &action);
        let history = action_button("History", Action::History, &action);
        let default_mode =
            action_button("Use at Startup", Action::SetDefault, &action).style(ButtonStyle::Ghost);
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
        }
    }
    fn refresh_controls(&self) {
        let model = self.core.model.borrow();
        *self.core.pickers.borrow_mut() = pickers(&model, &self.core.action);
        *self.core.keys.borrow_mut() = keys(&model, &self.core.action);
    }
    fn current_settings(&self) -> Settings {
        let mut settings = self.core.model.borrow().settings;
        settings.degrees = self.core.degrees.get() == 0;
        settings.signed = self.core.signed.get();
        settings
    }
    fn calculate(&self, explicit: bool) {
        let text = self.core.input.value();
        let settings = self.current_settings();
        let mut model = self.core.model.borrow_mut();
        model.status = None;
        let previous = model.answers[index(settings.mode)];
        let result = engine::evaluate(&text, settings, previous).and_then(|value| {
            if settings.mode != Mode::Convert {
                return Ok(value);
            }
            let Value::Real(value) = value else {
                return Err("A real number is required".into());
            };
            units::convert(value, model.category, model.from, model.to).map(Value::Real)
        });
        if settings.mode == Mode::Convert {
            model.conversion_input = Some(text.clone());
        }
        match result {
            Ok(result) => {
                model.result = Some(result);
                model.error = None;
                if explicit {
                    model.answers[index(settings.mode)] = Some(result);
                    model.evaluated = true;
                    let history = History {
                        input: text,
                        result,
                        settings,
                        units: (model.category, model.from, model.to),
                    };
                    if model.history.last().is_none_or(|old| {
                        old.input != history.input
                            || old.settings != history.settings
                            || old.units != history.units
                            || old.result != history.result
                    }) {
                        model.history.push(history);
                        if model.history.len() > 50 {
                            model.history.remove(0);
                        }
                        self.core.history_buttons.borrow_mut().clear();
                    }
                }
            }
            Err(error) => {
                model.result = None;
                model.evaluated = false;
                model.error = explicit.then_some(error);
            }
        }
    }
    fn apply_action(&self, frames: &Frames, context: &mut EventContext<'_>) {
        let Some(action) = self.core.action.take() else {
            return;
        };
        match action {
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
                let text = if self.core.model.borrow().evaluated {
                    "Ans".to_owned()
                } else {
                    let text = self.core.input.value();
                    if text.is_empty() { "0".into() } else { text }
                };
                self.core.input.set_value(match action {
                    Action::Negate => format!("-({text})"),
                    Action::Reciprocal => format!("1/({text})"),
                    Action::Not => format!("~({text})"),
                    _ => format!("({text})^2"),
                });
                let mut model = self.core.model.borrow_mut();
                model.evaluated = false;
                model.result = None;
                model.error = None;
                self.core.input.set_focused(true);
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
                let mut model = self.core.model.borrow_mut();
                model.history_visible = !model.history_visible;
                model.scroll = 0.0;
            }
            Action::ClearHistory => {
                let mut model = self.core.model.borrow_mut();
                model.history.clear();
                model.scroll = 0.0;
                self.core.history_buttons.borrow_mut().clear();
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
                model.conversion_input = None;
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
                model.conversion_input = None;
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
                model.conversion_input = None;
                drop(model);
                self.refresh_controls();
            }
            Action::Swap => {
                let mut model = self.core.model.borrow_mut();
                let from = model.from;
                model.from = model.to;
                model.to = from;
                model.conversion_input = None;
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
                    model.history_visible = false;
                    model.conversion_input = Some(self.core.input.value());
                    drop(model);
                    self.refresh_controls();
                }
            }
        }
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
        let toolbar = Rect::new(
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            theme.layout.top_bar_height + if bounds.size.width < 500.0 { 36.0 } else { 0.0 },
        );
        let input = Rect::new(
            bounds.origin.x + 16.0,
            toolbar.origin.y + toolbar.size.height + 12.0,
            (bounds.size.width - 32.0).max(0.0),
            32.0,
        );
        let result = Rect::new(
            input.origin.x,
            input.origin.y + 46.0,
            input.size.width,
            44.0,
        );
        let context = Rect::new(
            input.origin.x,
            result.origin.y + 92.0,
            input.size.width,
            if mode == Mode::Convert {
                72.0
            } else if mode == Mode::Standard {
                0.0
            } else {
                28.0
            },
        );
        let y = context.origin.y + context.size.height + 16.0;
        let bottom = bounds.origin.y + bounds.size.height - 52.0;
        Self {
            bounds,
            toolbar,
            input,
            result,
            copy: Rect::new(
                result.origin.x + result.size.width - 108.0,
                result.origin.y + 52.0,
                108.0,
                28.0,
            ),
            context,
            keys: Rect::new(input.origin.x, y, input.size.width, (bottom - y).max(0.0)),
            message: Rect::new(input.origin.x, bottom + 12.0, input.size.width, 32.0),
        }
    }
    fn toolbar_button(&self, index: usize) -> Rect {
        let narrow = self.toolbar.size.width < 500.0;
        let y = self.toolbar.origin.y
            + (self.toolbar.size.height - if narrow { 36.0 } else { 0.0 } - 28.0) / 2.0;
        if narrow && index == 2 {
            return Rect::new(self.toolbar.origin.x + 16.0, y + 36.0, 136.0, 28.0);
        }
        match index {
            0 => Rect::new(self.toolbar.origin.x + 16.0, y, 146.0, 28.0),
            1 => Rect::new(
                self.toolbar.origin.x + self.toolbar.size.width - 100.0,
                y,
                84.0,
                28.0,
            ),
            _ => Rect::new(
                self.toolbar.origin.x + self.toolbar.size.width - 244.0,
                y,
                136.0,
                28.0,
            ),
        }
    }
    fn key(&self, index: usize, count: usize, mode: Mode) -> Rect {
        let columns = if matches!(mode, Mode::Scientific | Mode::Programmer) {
            6
        } else {
            4
        };
        let rows = count.div_ceil(columns);
        let w = (self.keys.size.width - 8.0 * (columns - 1) as f32).max(0.0) / columns as f32;
        let h = (self.keys.size.height - 8.0 * (rows - 1) as f32).max(0.0) / rows as f32;
        Rect::new(
            self.keys.origin.x + (index % columns) as f32 * (w + 8.0),
            self.keys.origin.y + (index / columns) as f32 * (h + 8.0),
            w,
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
        let needs_conversion = {
            let model = self.core.model.borrow();
            model.settings.mode == Mode::Convert
                && model.conversion_input.as_deref() != Some(&self.core.input.value())
        };
        if needs_conversion {
            self.calculate(false);
        }
        let settings = self.current_settings();
        let model = self.core.model.borrow();
        let frames = Frames::new(bounds, context.theme, settings.mode);
        Rectangle::new()
            .color(RectangleColor::Custom(context.theme.colors.background))
            .paint(bounds, context);
        Rectangle::new()
            .color(RectangleColor::Custom(context.theme.colors.surface_subtle))
            .paint(frames.toolbar, context);
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
            format!("{}{label}", if approximate { "≈ " } else { "" })
        };
        let role = if binary_lines || label.len() > 28 {
            TextRole::Body
        } else if label.len() > 16 {
            TextRole::TitleSmall
        } else {
            TextRole::TitleLarge
        };
        Text::styled(shown, role)
            .alignment(TextAlignment::End)
            .paint(frames.result, context);
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
        if model.history_visible {
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
            .unwrap_or_else(|| {
                if settings.mode == Mode::Programmer {
                    format!(
                        "{}-bit arithmetic · overflow wraps · startup: {}",
                        settings.bits,
                        model.start_mode.label()
                    )
                } else {
                    format!("Startup: {} · Enter to calculate", model.start_mode.label())
                }
            });
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
        let frames = Frames::new(bounds, context.theme(), mode);
        if let ViewEvent::KeyPressed { key, modifiers } = event {
            let action = match key {
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
        if let ViewEvent::TextInput { text } = event {
            if text == "=" {
                self.core.action.set(Some(Action::Equals));
                self.apply_action(&frames, context);
                return EventResult::Consumed;
            }
            self.prepare_input(text);
        }
        let before = self.core.input.value();
        let mut result = self.core.field.handle_event(frames.input, event, context);
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
        if self.core.model.borrow().history_visible {
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
        } else {
            let keys = self.core.keys.borrow();
            for (i, key) in keys.iter().enumerate() {
                result = result.merge(key.button.handle_event(
                    frames.key(i, keys.len(), mode),
                    event,
                    context,
                ));
            }
        }
        if self.core.input.value() != before {
            let mut model = self.core.model.borrow_mut();
            model.evaluated = false;
            model.result = None;
            model.error = None;
            model.status = None;
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
            model.evaluated = false;
            model.result = None;
        }
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
            Rect::new(0.0, 0.0, 600.0, 690.0)
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
        assert_eq!(f.result(), "7");
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
        f.action(Action::ClearHistory);
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
        assert_eq!(f.result(), "89.6");
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
