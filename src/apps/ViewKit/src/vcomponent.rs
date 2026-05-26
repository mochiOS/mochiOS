use std::collections::HashMap;

use html5ever::driver::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Rect, Transform};
use ui_layout::{
    AlignItems, Display, FlexDirection, ItemStyle, JustifyContent, LayoutEngine, LayoutNode,
    Length, Style,
};

#[derive(Clone, Debug, Default)]
pub struct VComponent {
    template: String,
    width: Option<u32>,
    height: Option<u32>,
    label: Option<String>,
    image: Option<String>,
    root_class: Option<String>,
    children: Vec<VComponent>,
}

impl VComponent {
    pub fn from_str(template: &str) -> Self {
        Self {
            template: template.to_string(),
            ..Default::default()
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = Some(h);
        self
    }

    pub fn label(mut self, s: String) -> Self {
        self.label = Some(s);
        self
    }

    pub fn image<T: Into<String>>(mut self, path: T) -> Self {
        self.image = Some(path.into());
        self
    }

    pub fn text<T: Into<String>>(self, s: T) -> Self {
        self.label(s.into())
    }

    pub fn class<T: Into<String>>(mut self, class: T) -> Self {
        self.root_class = Some(class.into());
        self
    }

    pub fn child(mut self, c: VComponent) -> Self {
        self.children.push(c);
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = VComponent>,
    {
        self.children.extend(children);
        self
    }
}

pub fn render_ui_element_to_pixmap(ui: &crate::ui::UIElement, width: u32, height: u32) -> Vec<u32> {
    render_ui_element_to_pixmap_with_asset_root(ui, width, height, None)
}

pub fn render_ui_element_to_pixmap_with_asset_root(
    ui: &crate::ui::UIElement,
    width: u32,
    height: u32,
    asset_root: Option<&str>,
) -> Vec<u32> {
    let vc = ui_to_vcomponent(ui);
    render_component_to_pixmap_with_asset_root(&vc, width, height, asset_root)
}

fn ui_to_vcomponent(ui: &crate::ui::UIElement) -> VComponent {
    use crate::ui::{ComponentContent, UIElement};

    match ui {
        UIElement::Custom(c) => {
            let tpl = crate::components::template_for(&c.name).unwrap_or("<div><Children /></div>");
            let mut vc = VComponent::from_str(tpl);
            if let Some(content) = &c.content {
                match content {
                    ComponentContent::String(s) => vc = vc.label(s.clone()),
                    ComponentContent::Image(p) => vc = vc.image(p.clone()),
                    ComponentContent::Typed { ty, value } => {
                        if ty.eq_ignore_ascii_case("string") {
                            vc = vc.label(value.clone());
                        } else if ty.eq_ignore_ascii_case("image") {
                            vc = vc.image(value.clone());
                        }
                    }
                }
            }
            for child in &c.children {
                vc = vc.child(ui_to_vcomponent(child));
            }
            vc
        }
        UIElement::Bundle(items) => {
            let mut vc = VComponent::from_str("<Children />");
            for child in items {
                vc = vc.child(ui_to_vcomponent(child));
            }
            vc
        }
        UIElement::Card(card) => {
            let mut vc =
                VComponent::from_str(crate::components::template_for("card").unwrap_or("<div><Children /></div>"));
            for child in &card.children {
                vc = vc.child(ui_to_vcomponent(child));
            }
            vc
        }
        UIElement::Text(t) => {
            let tpl = crate::components::template_for("text").unwrap_or("<div><content type=\"String\" /></div>");
            VComponent::from_str(tpl).label(t.text.clone())
        }
        UIElement::Button(b) => {
            let mut vc =
                VComponent::from_str(crate::components::template_for("button").unwrap_or("<div><Children /></div>"));
            vc = vc.child(VComponent::from_str(
                crate::components::template_for("text").unwrap_or("<div><content type=\"String\" /></div>"),
            ).label(b.label.clone()));
            vc
        }
    }
}

pub fn render_component_to_pixmap(component: &VComponent, width: u32, height: u32) -> Vec<u32> {
    render_component_to_pixmap_with_asset_root(component, width, height, None)
}

pub fn render_component_to_pixmap_with_asset_root(
    component: &VComponent,
    width: u32,
    height: u32,
    asset_root: Option<&str>,
) -> Vec<u32> {
    render_component_to_pixmap_with_asset_root_and_boxes(component, width, height, asset_root, &[])
        .0
}

pub fn render_component_to_pixmap_with_asset_root_and_boxes(
    component: &VComponent,
    width: u32,
    height: u32,
    asset_root: Option<&str>,
    capture_classes: &[&str],
) -> (Vec<u32>, HashMap<String, (u32, u32, u32, u32)>) {
    let mut pixmap = Pixmap::new(width, height).expect("pixmap");
    pixmap.fill(Color::from_rgba8(0, 0, 0, 0));

    let ctx = RenderContext::new(asset_root);
    let boxes = render_component_impl(
        &ctx,
        &mut pixmap,
        component,
        0.0,
        0.0,
        width as f32,
        height as f32,
        capture_classes,
    );

    // Convert Pixmap bytes -> ARGB u32 for Binder/Kagami.
    // tiny-skia Pixmap stores pixels in BGRA byte order.
    let mut out = vec![0u32; (width as usize).saturating_mul(height as usize)];
    let data = pixmap.data();
    for i in 0..out.len() {
        let off = i * 4;
        let b = data[off] as u32;
        let g = data[off + 1] as u32;
        let r = data[off + 2] as u32;
        let a = data[off + 3] as u32;
        out[i] = (a << 24) | (r << 16) | (g << 8) | b;
    }
    (out, boxes)
}

pub fn measure_component_boxes(
    component: &VComponent,
    width: u32,
    height: u32,
    asset_root: Option<&str>,
    capture_classes: &[&str],
) -> HashMap<String, (u32, u32, u32, u32)> {
    let mut pixmap = Pixmap::new(width, height).expect("pixmap");
    pixmap.fill(Color::from_rgba8(0, 0, 0, 0));

    let ctx = RenderContext::new(asset_root);
    render_component_impl(
        &ctx,
        &mut pixmap,
        component,
        0.0,
        0.0,
        width as f32,
        height as f32,
        capture_classes,
    )
}

struct RenderContext {
    asset_root: Option<String>,
    font: BitmapFont,
    image_cache: std::cell::RefCell<HashMap<String, image::RgbaImage>>,
}

impl RenderContext {
    fn new(asset_root: Option<&str>) -> Self {
        let font_bytes = include_bytes!("../../../../src/resources/system/fonts/ter-u12b.bdf");
        let font = BitmapFont::from_bdf(font_bytes);
        Self {
            asset_root: asset_root.map(|s| s.to_string()),
            font,
            image_cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            // mochiOS target: use swiftlib fs syscalls directly.
            match swiftlib::fs::read_file_via_fs(path, 16 * 1024 * 1024) {
                Ok(Some(v)) => Some(v),
                _ => None,
            }
        }
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        {
            std::fs::read(path).ok()
        }
    }

    fn resolve_asset_path(&self, src: &str) -> Option<String> {
        let src = src.trim();
        if src.is_empty() {
            return None;
        }
        if src.starts_with('/') {
            return Some(src.to_string());
        }
        if let Some(root) = &self.asset_root {
            return Some(format!("{}/{}", root.trim_end_matches('/'), src));
        }
        // best-effort fallback: interpret relative to root
        Some(format!("/{}", src))
    }
}

const FONT_HEIGHT: usize = 12;
const GLYPH_COUNT: usize = 96;
const ASCII_START: usize = 32;
const ASCII_END: usize = ASCII_START + GLYPH_COUNT;

struct BitmapFont {
    glyphs: [[u8; FONT_HEIGHT]; GLYPH_COUNT],
}

impl BitmapFont {
    fn from_bdf(data: &[u8]) -> Self {
        let mut glyphs = [[0u8; FONT_HEIGHT]; GLYPH_COUNT];
        parse_bdf(data, &mut glyphs);
        Self { glyphs }
    }

    fn glyph(&self, ch: u8) -> &[u8; FONT_HEIGHT] {
        let idx = if (ASCII_START as u8..ASCII_END as u8).contains(&ch) {
            (ch as usize) - ASCII_START
        } else {
            (b'?' as usize) - ASCII_START
        };
        &self.glyphs[idx]
    }
}

fn parse_bdf(data: &[u8], glyphs: &mut [[u8; FONT_HEIGHT]; GLYPH_COUNT]) {
    let text = core::str::from_utf8(data).unwrap_or("");
    let mut lines = text.lines();
    let mut encoding: Option<usize> = None;
    let mut in_bitmap = false;
    let mut row = 0usize;

    loop {
        let line = match lines.next() {
            Some(l) => l.trim(),
            None => break,
        };
        if line.starts_with("ENCODING ") {
            encoding = line[9..].trim().parse::<usize>().ok();
            in_bitmap = false;
            row = 0;
        } else if line == "BITMAP" {
            in_bitmap = true;
            row = 0;
        } else if line == "ENDCHAR" {
            in_bitmap = false;
            encoding = None;
            row = 0;
        } else if in_bitmap {
            if let Some(enc) = encoding {
                if (ASCII_START..ASCII_END).contains(&enc) && row < FONT_HEIGHT {
                    if let Ok(byte) = u8::from_str_radix(line, 16) {
                        glyphs[enc - ASCII_START][row] = byte;
                    }
                    row += 1;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ComputedStyle {
    layout: Style,
    bg: Option<u32>,
    border_radius: (f32, f32, f32, f32),
    text_align_center: bool,
    text_align_explicit: bool,
    padding: (f32, f32, f32, f32),
}

fn render_component_impl(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    component: &VComponent,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    capture_classes: &[&str],
) -> HashMap<String, (u32, u32, u32, u32)> {
    let (css, html) = split_style_and_html(&component.template);
    let css = css
        .replace(
            "CONTENT_W",
            &format!("{}px", component.width.unwrap_or(w as u32)),
        )
        .replace(
            "CONTENT_H",
            &format!("{}px", component.height.unwrap_or(h as u32)),
        );

    let class_styles = parse_class_css_rules(&css);

    let mut root = parse_html_fragment(&html);
    if let Some(cls) = component.root_class.as_deref() {
        apply_root_class(&mut root, cls);
    }

    let render_tree = build_render_tree(ctx, &class_styles, &root, component);

    layout_and_paint(ctx, pixmap, &render_tree, x, y, w, h, capture_classes)
}

fn apply_root_class(node: &mut HtmlNode, extra_class: &str) {
    let Some(extra_class) = extra_class.split_whitespace().next() else {
        return;
    };
    if extra_class.is_empty() {
        return;
    }
    if let HtmlNode::Element(el) = node {
        let key = "class".to_string();
        let cur = el.attrs.get("class").cloned().unwrap_or_default();
        if cur.split_whitespace().any(|c| c == extra_class) {
            return;
        }
        let next = if cur.is_empty() {
            extra_class.to_string()
        } else {
            format!("{} {}", cur, extra_class)
        };
        el.attrs.insert(key, next);
    }
}

#[derive(Clone, Debug)]
enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

#[derive(Clone, Debug, Default)]
struct HtmlElement {
    tag: String,
    attrs: HashMap<String, String>,
    children: Vec<HtmlNode>,
}

#[derive(Clone)]
enum RenderNodeKind {
    Element { tag: String, class: Option<String> },
    // A non-rendering node that exists only to splice multiple children into a parent.
    // Used for `<Children />` expansion so flex containers treat each child as a flex item.
    Fragment,
    Text { text: String },
    Image { src: String },
}

#[derive(Clone)]
struct RenderNode {
    kind: RenderNodeKind,
    style: ComputedStyle,
    children: Vec<RenderNode>,
}

fn split_style_and_html(template: &str) -> (String, String) {
    let mut css = String::new();
    let mut html = template.to_string();
    if let Some(start) = template.find("<style") {
        if let Some(end) = template.find("</style>") {
            // best-effort: extract between first '>' and </style>
            let head = &template[start..end];
            if let Some(gt) = head.find('>') {
                css = head[gt + 1..].to_string();
            }
            html = template[end + "</style>".len()..].to_string();
        }
    }
    (css, html)
}

fn parse_html_fragment(html: &str) -> HtmlNode {
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    let body = find_body(&dom.document).unwrap_or_else(|| dom.document.clone());
    let mut children = collect_html_children(&body);

    if children.len() == 1 {
        children.remove(0)
    } else {
        HtmlNode::Element(HtmlElement {
            tag: "div".to_string(),
            attrs: HashMap::new(),
            children,
        })
    }
}

fn find_body(handle: &Handle) -> Option<Handle> {
    match &handle.data {
        NodeData::Element { name, .. } if name.local.as_ref().eq_ignore_ascii_case("body") => {
            return Some(handle.clone());
        }
        _ => {}
    }

    for child in handle.children.borrow().iter() {
        if let Some(found) = find_body(child) {
            return Some(found);
        }
    }

    None
}

fn collect_html_children(handle: &Handle) -> Vec<HtmlNode> {
    handle
        .children
        .borrow()
        .iter()
        .filter_map(html_node_from_handle)
        .collect()
}

fn html_node_from_handle(handle: &Handle) -> Option<HtmlNode> {
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let mut map = HashMap::new();
            for attr in attrs.borrow().iter() {
                map.insert(
                    attr.name.local.to_string().to_ascii_lowercase(),
                    attr.value.to_string(),
                );
            }

            Some(HtmlNode::Element(HtmlElement {
                tag: name.local.to_string().to_ascii_lowercase(),
                attrs: map,
                children: collect_html_children(handle),
            }))
        }
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            if text.trim().is_empty() {
                None
            } else {
                Some(HtmlNode::Text(text))
            }
        }
        NodeData::Document => {
            let children = collect_html_children(handle);
            if children.is_empty() {
                None
            } else if children.len() == 1 {
                children.into_iter().next()
            } else {
                Some(HtmlNode::Element(HtmlElement {
                    tag: "div".to_string(),
                    attrs: HashMap::new(),
                    children,
                }))
            }
        }
        _ => None,
    }
}

fn get_attr(el: &HtmlElement, name: &str) -> Option<String> {
    el.attrs.get(&name.to_ascii_lowercase()).cloned()
}

fn build_render_tree(
    ctx: &RenderContext,
    class_styles: &HashMap<String, HashMap<String, String>>,
    node: &HtmlNode,
    component: &VComponent,
) -> RenderNode {
    fn inline_text_style(inherited_text_align_center: bool) -> ComputedStyle {
        let mut s = ComputedStyle::default();
        // Treat text nodes as block-level boxes so `text-align` can be emulated
        // by centering within the text node's own layout width.
        s.layout.display = Display::Block;
        s.text_align_center = inherited_text_align_center;
        s
    }

    fn inner(
        ctx: &RenderContext,
        class_styles: &HashMap<String, HashMap<String, String>>,
        node: &HtmlNode,
        component: &VComponent,
        inherited_text_align_center: bool,
    ) -> RenderNode {
        // Handle special tags
        if let HtmlNode::Element(el) = node {
            let tag_lower = el.tag.to_ascii_lowercase();
            if tag_lower == "children" {
                // expand component children
                let mut kids = Vec::new();
                for ch in &component.children {
                    let (css, html) = split_style_and_html(&ch.template);
                    let css = css
                        .replace("CONTENT_W", &format!("{}px", ch.width.unwrap_or(0)))
                        .replace("CONTENT_H", &format!("{}px", ch.height.unwrap_or(0)));
                    let class_styles_child = parse_class_css_rules(&css);
                    let root = parse_html_fragment(&html);
                    kids.push(inner(
                        ctx,
                        &class_styles_child,
                        &root,
                        ch,
                        inherited_text_align_center,
                    ));
                }
                let mut fragment_style = ComputedStyle::default();
                fragment_style.layout.display = Display::Block;
                fragment_style.layout.size.width = Length::Auto;
                fragment_style.layout.size.height = Length::Auto;
                return RenderNode {
                    kind: RenderNodeKind::Fragment,
                    style: fragment_style,
                    children: kids,
                };
            }
            if tag_lower == "content" {
                let ty = get_attr(el, "type").unwrap_or_default();
                if ty.eq_ignore_ascii_case("string") {
                    return RenderNode {
                        kind: RenderNodeKind::Text {
                            text: component.label.clone().unwrap_or_default(),
                        },
                        style: inline_text_style(inherited_text_align_center),
                        children: vec![],
                    };
                }
                if ty.eq_ignore_ascii_case("image") {
                    if let Some(path) = component.image.clone() {
                        if let Some(resolved) = ctx.resolve_asset_path(&path) {
                            let style = compute_style_for_node(el, class_styles);
                            return RenderNode {
                                kind: RenderNodeKind::Image { src: resolved },
                                style,
                                children: vec![],
                            };
                        }
                    }
                }
            }
            if tag_lower == "img" {
                let src = get_attr(el, "src").unwrap_or_default();
                if let Some(resolved) = ctx.resolve_asset_path(&src) {
                    let style = compute_style_for_node(el, class_styles);
                    return RenderNode {
                        kind: RenderNodeKind::Image { src: resolved },
                        style,
                        children: vec![],
                    };
                }
            }
        }

        // Text nodes
        if let HtmlNode::Text(text) = node {
            let text = text.trim();
            if !text.is_empty() {
                return RenderNode {
                    kind: RenderNodeKind::Text {
                        text: text.to_string(),
                    },
                    style: inline_text_style(inherited_text_align_center),
                    children: vec![],
                };
            }
        }

        // Generic element
        let mut children = Vec::new();
        let (tag, class, mut style) = match node {
            HtmlNode::Element(el) => {
                let style = compute_style_for_node(el, class_styles);
                (el.tag.clone(), get_attr(el, "class"), style)
            }
            HtmlNode::Text(_) => ("div".to_string(), None, ComputedStyle::default()),
        };

        // Inherit `text-align` like CSS.
        if !style.text_align_explicit {
            style.text_align_center = inherited_text_align_center;
        }
        let next_inherited_center = style.text_align_center;

        if let HtmlNode::Element(el) = node {
            for child in &el.children {
                let rendered = inner(
                    ctx,
                    class_styles,
                    child,
                    component,
                    next_inherited_center,
                );
                match rendered.kind {
                    RenderNodeKind::Fragment => children.extend(rendered.children),
                    _ => children.push(rendered),
                }
            }
        }

        RenderNode {
            kind: RenderNodeKind::Element { tag, class },
            style,
            children,
        }
    }

    inner(ctx, class_styles, node, component, false)
}

fn parse_class_css_rules(css: &str) -> HashMap<String, HashMap<String, String>> {
    // Very small CSS parser: only supports `.class { key: value; ... }`
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut rest = css;
    loop {
        let dot = match rest.find('.') {
            Some(i) => i,
            None => break,
        };
        rest = &rest[dot + 1..];
        let brace = match rest.find('{') {
            Some(i) => i,
            None => break,
        };
        let class = rest[..brace]
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        rest = &rest[brace + 1..];
        let end = match rest.find('}') {
            Some(i) => i,
            None => break,
        };
        let body = &rest[..end];
        rest = &rest[end + 1..];
        if class.is_empty() {
            continue;
        }
        let decls = map.entry(class).or_default();
        for part in body.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once(':') {
                decls.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
    }
    map
}

fn compute_style_for_node(
    el: &HtmlElement,
    class_styles: &HashMap<String, HashMap<String, String>>,
) -> ComputedStyle {
    let mut out = ComputedStyle::default();
    let class = get_attr(el, "class").unwrap_or_default();
    if class.is_empty() {
        return out;
    }
    let cls = class.split_whitespace().next().unwrap_or("");
    let decls = match class_styles.get(cls) {
        Some(d) => d,
        None => return out,
    };

    let mut style = Style::default();
    // Match CSS defaults: width/height are `auto`, not 0px.
    style.size.width = Length::Auto;
    style.size.height = Length::Auto;
    style.size.min_width = Length::Auto;
    style.size.max_width = Length::Auto;
    style.size.min_height = Length::Auto;
    style.size.max_height = Length::Auto;
    let mut padding = (0f32, 0f32, 0f32, 0f32);
    let mut saw_align_items = false;
    let mut saw_display_flex = false;
    let mut pending_flex_direction: Option<FlexDirection> = None;

    for (k, v) in decls {
        match k.as_str() {
            "background-color" => out.bg = Some(parse_color(v)),
            "border-radius" => out.border_radius = parse_border_radius(v),
            "display" => {
                if v.trim().eq_ignore_ascii_case("flex") {
                    saw_display_flex = true;
                }
            }
            "gap" => {
                if let Some(px) = parse_px(v) {
                    style.column_gap = Length::Px(px);
                    style.row_gap = Length::Px(px);
                }
            }
            "flex-direction" => {
                pending_flex_direction = Some(if v.trim().eq_ignore_ascii_case("column") {
                    FlexDirection::Column
                } else {
                    FlexDirection::Row
                });
            }
            "justify-content" => {
                style.justify_content = match v.trim() {
                    "center" => JustifyContent::Center,
                    "flex-end" | "end" => JustifyContent::End,
                    "space-between" => JustifyContent::SpaceBetween,
                    _ => JustifyContent::Start,
                };
            }
            "align-items" => {
                saw_align_items = true;
                style.align_items = match v.trim() {
                    "center" => AlignItems::Center,
                    "flex-end" | "end" => AlignItems::End,
                    "stretch" => AlignItems::Stretch,
                    _ => AlignItems::Start,
                };
            }
            "flex" => {
                // Support a small subset of CSS flex shorthand.
                // - `flex: 1` in CSS means `flex: 1 1 0%`.
                // - `flex: 0` means no growth.
                // We only implement numeric single-value form for now.
                if let Ok(n) = v.trim().parse::<f32>() {
                    style.item_style = ItemStyle {
                        flex_grow: n,
                        flex_shrink: 1.0,
                        flex_basis: Length::Percent(0.0),
                        ..Default::default()
                    };
                }
            }
            "width" => {
                style.size.width = parse_length(v);
            }
            "height" => {
                style.size.height = parse_length(v);
            }
            "padding" => {
                padding = parse_box_1_2(v);
            }
            "box-sizing" => {
                if v.trim().eq_ignore_ascii_case("border-box") {
                    style.box_sizing = ui_layout::BoxSizing::BorderBox;
                } else {
                    style.box_sizing = ui_layout::BoxSizing::ContentBox;
                }
            }
            "margin-left" => {
                let vt = v.trim();
                style.spacing.margin_left = if vt.eq_ignore_ascii_case("auto") {
                    Length::Auto
                } else {
                    parse_px(v).map(Length::Px).unwrap_or(Length::Px(0.0))
                };
            }
            "margin-right" => {
                let vt = v.trim();
                style.spacing.margin_right = if vt.eq_ignore_ascii_case("auto") {
                    Length::Auto
                } else {
                    parse_px(v).map(Length::Px).unwrap_or(Length::Px(0.0))
                };
            }
            "margin-top" => {
                let vt = v.trim();
                style.spacing.margin_top = if vt.eq_ignore_ascii_case("auto") {
                    Length::Auto
                } else {
                    parse_px(v).map(Length::Px).unwrap_or(Length::Px(0.0))
                };
            }
            "margin-bottom" => {
                let vt = v.trim();
                style.spacing.margin_bottom = if vt.eq_ignore_ascii_case("auto") {
                    Length::Auto
                } else {
                    parse_px(v).map(Length::Px).unwrap_or(Length::Px(0.0))
                };
            }
            "margin" => {
                // Support a small subset of CSS margin shorthand (1 or 2 values),
                // plus `auto` for centering.
                let parts: Vec<_> = v.split_whitespace().collect();
                if parts.is_empty() {
                    // noop
                } else if parts.len() == 1 {
                    let p = parts[0].trim();
                    let val = if p.eq_ignore_ascii_case("auto") {
                        Length::Auto
                    } else {
                        parse_px(p).map(Length::Px).unwrap_or(Length::Px(0.0))
                    };
                    style.spacing.margin_top = val.clone();
                    style.spacing.margin_right = val.clone();
                    style.spacing.margin_bottom = val.clone();
                    style.spacing.margin_left = val;
                } else {
                    let p0 = parts[0].trim();
                    let p1 = parts[1].trim();
                    let v0 = if p0.eq_ignore_ascii_case("auto") {
                        Length::Auto
                    } else {
                        parse_px(p0).map(Length::Px).unwrap_or(Length::Px(0.0))
                    };
                    let v1 = if p1.eq_ignore_ascii_case("auto") {
                        Length::Auto
                    } else {
                        parse_px(p1).map(Length::Px).unwrap_or(Length::Px(0.0))
                    };
                    style.spacing.margin_top = v0.clone();
                    style.spacing.margin_bottom = v0;
                    style.spacing.margin_left = v1.clone();
                    style.spacing.margin_right = v1;
                }
            }
            "text-align" => {
                out.text_align_explicit = true;
                out.text_align_center = v.trim().eq_ignore_ascii_case("center");
            }
            _ => {}
        }
    }

    // Resolve `display:flex` + `flex-direction` regardless of HashMap iteration order.
    if saw_display_flex || pending_flex_direction.is_some() {
        style.display = Display::Flex {
            flex_direction: pending_flex_direction.unwrap_or(FlexDirection::Row),
        };
    }

    // CSS flexbox default is `align-items: stretch`.
    if matches!(style.display, Display::Flex { .. }) && !saw_align_items {
        style.align_items = AlignItems::Stretch;
    }

    // If a node is not flex, `justify-content` does nothing in CSS.
    if !matches!(style.display, Display::Flex { .. }) {
        style.justify_content = JustifyContent::Start;
    }

    // Heuristic: for flex items with auto main-size, prefer shrink-to-content
    // instead of filling the entire container like block flow.
    // This makes `margin-left: auto` work as expected for right-aligned groups.
    if matches!(style.display, Display::Flex { .. }) == false {
        // noop (only affects flex items below)
    }

    out.padding = padding;
    out.layout = style;
    out
}

fn parse_length(v: &str) -> Length {
    let v = v.trim();
    if v.eq_ignore_ascii_case("fit-content") || v.eq_ignore_ascii_case("auto") {
        return Length::Auto;
    }
    if let Some(px) = parse_px(v) {
        return Length::Px(px);
    }
    if let Some(pct) = v.strip_suffix('%') {
        if let Ok(f) = pct.trim().parse::<f32>() {
            // ui_layout expects percent values like 100.0 for "100%".
            return Length::Percent(f);
        }
    }
    Length::Auto
}

fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let v = v.strip_suffix("px").unwrap_or(v);
    v.trim().parse::<f32>().ok()
}

fn parse_box_1_2(v: &str) -> (f32, f32, f32, f32) {
    // CSS padding shorthand: 1 or 2 values
    let parts: Vec<_> = v.split_whitespace().collect();
    if parts.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    if parts.len() == 1 {
        let a = parse_px(parts[0]).unwrap_or(0.0);
        return (a, a, a, a);
    }
    let v0 = parse_px(parts[0]).unwrap_or(0.0);
    let v1 = parse_px(parts[1]).unwrap_or(0.0);
    (v0, v1, v0, v1)
}

fn parse_border_radius(v: &str) -> (f32, f32, f32, f32) {
    // Minimal CSS `border-radius` shorthand (no `/` elliptical radii):
    // 1 value: all corners
    // 2 values: tl+br, tr+bl
    // 3 values: tl, tr+bl, br
    // 4 values: tl, tr, br, bl
    let parts: Vec<_> = v.split_whitespace().collect();
    let mut r = [0.0f32; 4];
    match parts.len() {
        0 => {}
        1 => {
            let a = parse_px(parts[0]).unwrap_or(0.0);
            r = [a, a, a, a];
        }
        2 => {
            let a = parse_px(parts[0]).unwrap_or(0.0);
            let b = parse_px(parts[1]).unwrap_or(0.0);
            r = [a, b, a, b];
        }
        3 => {
            let a = parse_px(parts[0]).unwrap_or(0.0);
            let b = parse_px(parts[1]).unwrap_or(0.0);
            let c = parse_px(parts[2]).unwrap_or(0.0);
            r = [a, b, c, b];
        }
        _ => {
            let a = parse_px(parts[0]).unwrap_or(0.0);
            let b = parse_px(parts[1]).unwrap_or(0.0);
            let c = parse_px(parts[2]).unwrap_or(0.0);
            let d = parse_px(parts[3]).unwrap_or(0.0);
            r = [a, b, c, d];
        }
    }
    (r[0], r[1], r[2], r[3])
}

fn parse_color_hex(s: &str) -> u32 {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 6 {
        if let Ok(v) = u32::from_str_radix(s, 16) {
            return 0xFF00_0000u32 | v;
        }
    }
    0xFF00_0000u32
}

fn parse_color(s: &str) -> u32 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_color_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(args) = lower.strip_prefix("rgba(").and_then(|x| x.strip_suffix(')')) {
        return parse_color_rgba_args(args).unwrap_or(0xFF00_0000u32);
    }
    if let Some(args) = lower.strip_prefix("rgb(").and_then(|x| x.strip_suffix(')')) {
        return parse_color_rgb_args(args).unwrap_or(0xFF00_0000u32);
    }
    0xFF00_0000u32
}

fn parse_color_rgb_args(args: &str) -> Option<u32> {
    let parts: Vec<_> = args.split(',').map(|p| p.trim()).collect();
    if parts.len() != 3 {
        return None;
    }
    let r = parse_color_u8(parts[0])?;
    let g = parse_color_u8(parts[1])?;
    let b = parse_color_u8(parts[2])?;
    Some(((0xFFu32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
}

fn parse_color_rgba_args(args: &str) -> Option<u32> {
    let parts: Vec<_> = args.split(',').map(|p| p.trim()).collect();
    if parts.len() != 4 {
        return None;
    }
    let r = parse_color_u8(parts[0])?;
    let g = parse_color_u8(parts[1])?;
    let b = parse_color_u8(parts[2])?;
    let a = parse_color_alpha(parts[3])?;
    Some(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
}

fn parse_color_u8(s: &str) -> Option<u8> {
    let v = s.trim().parse::<i32>().ok()?;
    Some(v.clamp(0, 255) as u8)
}

fn parse_color_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Ok(f) = s.parse::<f32>() {
        if (0.0..=1.0).contains(&f) {
            return Some((f * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        if (1.0..=255.0).contains(&f) {
            return Some(f.round().clamp(0.0, 255.0) as u8);
        }
    }
    None
}

fn layout_and_paint(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    node: &RenderNode,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    capture_classes: &[&str],
) -> HashMap<String, (u32, u32, u32, u32)> {
    let mut layout_root = to_layout_node(node);
    LayoutEngine::layout(&mut layout_root, w, h);
    let mut boxes = HashMap::new();
    if !capture_classes.is_empty() {
        capture_boxes(node, &layout_root, x, y, capture_classes, &mut boxes);
    }
    paint_from_layout(ctx, pixmap, node, &layout_root, x, y);
    boxes
}

fn capture_boxes(
    node: &RenderNode,
    layout: &LayoutNode,
    ox: f32,
    oy: f32,
    capture_classes: &[&str],
    out: &mut HashMap<String, (u32, u32, u32, u32)>,
) {
    let (bx, by, _bw, _bh) = match &layout.layout_boxes {
        ui_layout::LayoutBoxes::Single(b) => (b.border_box.x, b.border_box.y, b.border_box.width, b.border_box.height),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let ox = ox + bx;
    let oy = oy + by;

    if let RenderNodeKind::Element { class: Some(class), .. } = &node.kind {
        if capture_classes.iter().any(|t| *t == class.as_str()) && !out.contains_key(class) {
            let (_bx, _by, bw, bh) = match &layout.layout_boxes {
                ui_layout::LayoutBoxes::Single(b) => {
                    (b.border_box.x, b.border_box.y, b.border_box.width, b.border_box.height)
                }
                _ => (0.0, 0.0, 0.0, 0.0),
            };
            let x = ox.max(0.0) as u32;
            let y = oy.max(0.0) as u32;
            let w = bw.max(0.0) as u32;
            let h = bh.max(0.0) as u32;
            out.insert(class.clone(), (x, y, w, h));
        }
    }

    for (idx, child_layout) in layout.children.iter().enumerate() {
        if idx >= node.children.len() {
            break;
        }
        capture_boxes(
            &node.children[idx],
            child_layout,
            ox,
            oy,
            capture_classes,
            out,
        );
    }
}

fn to_layout_node(node: &RenderNode) -> LayoutNode {
    let mut style = node.style.layout.clone();
    // Apply padding into layout style
    style.spacing.padding_top = Length::Px(node.style.padding.0);
    style.spacing.padding_right = Length::Px(node.style.padding.1);
    style.spacing.padding_bottom = Length::Px(node.style.padding.2);
    style.spacing.padding_left = Length::Px(node.style.padding.3);

    // Give text nodes an intrinsic size so flex layout can distribute space
    // (e.g. `margin-left: auto` for right-aligned button groups).
    if let RenderNodeKind::Text { text } = &node.kind {
        // 8px monospace-ish glyphs with a small padding.
        let w = (text.bytes().len() as f32) * 8.0;
        // Keep width as `auto` so block layout can stretch it (needed for `text-align: center`),
        // but provide an intrinsic minimum so flex sizing doesn't collapse it.
        if matches!(style.size.width, Length::Auto) {
            style.size.min_width = Length::Px(w);
        } else {
            style.size.width = Length::Px(w);
        }
        style.size.height = Length::Px(FONT_HEIGHT as f32);
    }

    let children: Vec<_> = node.children.iter().map(to_layout_node).collect();
    if children.is_empty() {
        LayoutNode::new(style)
    } else {
        LayoutNode::with_children(style, children)
    }
}

fn paint_from_layout(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    node: &RenderNode,
    layout: &LayoutNode,
    ox: f32,
    oy: f32,
) {
    let (bx, by, bw, bh) = match &layout.layout_boxes {
        ui_layout::LayoutBoxes::Single(b) => (
            b.border_box.x,
            b.border_box.y,
            b.border_box.width,
            b.border_box.height,
        ),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let x = ox + bx;
    let y = oy + by;

    // Paint background
    if let Some(argb) = node.style.bg {
        let mut paint = Paint::default();
        let a = ((argb >> 24) & 0xFF) as u8;
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8) & 0xFF) as u8;
        let b = (argb & 0xFF) as u8;
        paint.set_color(Color::from_rgba8(r, g, b, a));
        if node.style.border_radius.0 > 0.0
            || node.style.border_radius.1 > 0.0
            || node.style.border_radius.2 > 0.0
            || node.style.border_radius.3 > 0.0
        {
            if let Some(path) = rounded_rect_path(x, y, bw, bh, node.style.border_radius) {
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        } else if let Some(rect) = Rect::from_xywh(x, y, bw, bh) {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    // Paint leaf content
    match &node.kind {
        RenderNodeKind::Text { text } => {
            paint_text(
                ctx,
                pixmap,
                text,
                x,
                y,
                bw,
                bh,
                node.style.text_align_center,
            );
        }
        RenderNodeKind::Image { src } => {
            paint_image(ctx, pixmap, src, x, y, bw, bh);
        }
        _ => {}
    }

    // Paint children
    let mut idx = 0usize;
    for child_layout in layout.children.iter() {
        if idx >= node.children.len() {
            break;
        }
        paint_from_layout(ctx, pixmap, &node.children[idx], child_layout, x, y);
        idx += 1;
    }
}

    fn rounded_rect_path(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        (rtl, rtr, rbr, rbl): (f32, f32, f32, f32),
    ) -> Option<tiny_skia::Path> {
        let rtl = rtl.max(0.0).min(w * 0.5).min(h * 0.5);
        let rtr = rtr.max(0.0).min(w * 0.5).min(h * 0.5);
        let rbr = rbr.max(0.0).min(w * 0.5).min(h * 0.5);
        let rbl = rbl.max(0.0).min(w * 0.5).min(h * 0.5);
        let mut pb = PathBuilder::new();
        pb.move_to(x + rtl, y);
        pb.line_to(x + w - rtr, y);
        pb.quad_to(x + w, y, x + w, y + rtr);
        pb.line_to(x + w, y + h - rbr);
        pb.quad_to(x + w, y + h, x + w - rbr, y + h);
        pb.line_to(x + rbl, y + h);
        pb.quad_to(x, y + h, x, y + h - rbl);
        pb.line_to(x, y + rtl);
        pb.quad_to(x, y, x + rtl, y);
        pb.close();
        pb.finish()
    }

fn paint_text(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    center: bool,
) {
    let mut cursor_x = x + 8.0;
    let baseline = y + (h * 0.5) - (FONT_HEIGHT as f32 * 0.5);
    if center {
        let est_w = (text.bytes().len() as f32) * 8.0;
        cursor_x = x + (w - est_w).max(0.0) * 0.5;
    }

    for ch in text.bytes() {
        let glyph = ctx.font.glyph(ch);
        for (row_idx, row_byte) in glyph.iter().enumerate() {
            let py = (baseline as i32) + row_idx as i32;
            if py < 0 {
                continue;
            }
            let py = py as u32;
            if py >= pixmap.height() {
                continue;
            }
            for bit in 0..8 {
                if (row_byte >> (7 - bit)) & 1 == 0 {
                    continue;
                }
                let px_i = cursor_x as i32 + bit as i32;
                if px_i < 0 {
                    continue;
                }
                let px_u = px_i as u32;
                if px_u >= pixmap.width() {
                    continue;
                }
                let off = ((py * pixmap.width() + px_u) * 4) as usize;
                let data = pixmap.data_mut();
                data[off] = 0x11;
                data[off + 1] = 0x11;
                data[off + 2] = 0x11;
                data[off + 3] = 0xFF;
            }
        }
        cursor_x += 8.0;
    }
}

fn paint_image(
    ctx: &RenderContext,
    pixmap: &mut Pixmap,
    src: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let img = {
        let mut cache = ctx.image_cache.borrow_mut();
        if let Some(i) = cache.get(src) {
            i.clone()
        } else {
            let data = match ctx.read_file(src) {
                Some(d) => d,
                None => return,
            };
            let decoded = match image::load_from_memory(&data) {
                Ok(d) => d.to_rgba8(),
                Err(_) => return,
            };
            cache.insert(src.to_string(), decoded.clone());
            decoded
        }
    };
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return;
    }
    // Bilinear scaling (helps keep AA on downscaled UI icons).
    let tw = w.max(1.0) as u32;
    let th = h.max(1.0) as u32;
    for yy in 0..th {
        for xx in 0..tw {
            let sx_f = (xx as f32 + 0.5) * (iw as f32) / (tw as f32) - 0.5;
            let sy_f = (yy as f32 + 0.5) * (ih as f32) / (th as f32) - 0.5;
            let sx0 = sx_f.floor() as i32;
            let sy0 = sy_f.floor() as i32;
            let fx = (sx_f - sx0 as f32).clamp(0.0, 1.0);
            let fy = (sy_f - sy0 as f32).clamp(0.0, 1.0);

            let sx0u = sx0.clamp(0, (iw as i32) - 1) as u32;
            let sy0u = sy0.clamp(0, (ih as i32) - 1) as u32;
            let sx1u = (sx0 + 1).clamp(0, (iw as i32) - 1) as u32;
            let sy1u = (sy0 + 1).clamp(0, (ih as i32) - 1) as u32;

            let p00 = img.get_pixel(sx0u, sy0u).0;
            let p10 = img.get_pixel(sx1u, sy0u).0;
            let p01 = img.get_pixel(sx0u, sy1u).0;
            let p11 = img.get_pixel(sx1u, sy1u).0;

            let w00 = (1.0 - fx) * (1.0 - fy);
            let w10 = fx * (1.0 - fy);
            let w01 = (1.0 - fx) * fy;
            let w11 = fx * fy;

            let mut p = [0u8; 4];
            for c in 0..4 {
                let v = (p00[c] as f32) * w00
                    + (p10[c] as f32) * w10
                    + (p01[c] as f32) * w01
                    + (p11[c] as f32) * w11;
                p[c] = v.round().clamp(0.0, 255.0) as u8;
            }
            let dx = x as i32 + xx as i32;
            let dy = y as i32 + yy as i32;
            if dx < 0 || dy < 0 {
                continue;
            }
            let dx = dx as u32;
            let dy = dy as u32;
            if dx >= pixmap.width() || dy >= pixmap.height() {
                continue;
            }
            let off = ((dy * pixmap.width() + dx) * 4) as usize;
            let data = pixmap.data_mut();
            // `image` gives RGBA; Pixmap is BGRA. Alpha-blend over existing pixels
            // so that transparent parts of icons don't punch holes through.
            let a = p[3] as u32;
            if a == 0 {
                continue;
            }
            let sb = p[2] as u32;
            let sg = p[1] as u32;
            let sr = p[0] as u32;
            if a == 255 {
                data[off] = sb as u8;
                data[off + 1] = sg as u8;
                data[off + 2] = sr as u8;
                data[off + 3] = 255;
                continue;
            }
            let db = data[off] as u32;
            let dg = data[off + 1] as u32;
            let dr = data[off + 2] as u32;
            let da = data[off + 3] as u32;

            // Porter-Duff "source over" with 8-bit alpha.
            let inv_a = 255 - a;
            let out_a = a + (da * inv_a + 127) / 255;
            let out_b = (sb * a + db * inv_a + 127) / 255;
            let out_g = (sg * a + dg * inv_a + 127) / 255;
            let out_r = (sr * a + dr * inv_a + 127) / 255;

            data[off] = out_b as u8;
            data[off + 1] = out_g as u8;
            data[off + 2] = out_r as u8;
            data[off + 3] = out_a.min(255) as u8;
        }
    }
}
