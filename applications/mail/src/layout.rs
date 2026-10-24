use crate::model::Page;
use appcore::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Compact,
    Regular,
    Wide,
}
#[derive(Clone, Copy)]
pub struct Layout {
    pub width: Width,
    pub bounds: Rect,
    pub sidebar: Option<Rect>,
    pub list: Option<Rect>,
    pub reader: Option<Rect>,
    pub composer: bool,
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(0.0), h.max(0.0))
}
impl Layout {
    // Local layout proposals: 280px list + 400px reader + spacing; not global design tokens.
    pub fn new(
        b: Rect,
        page: Page,
        sidebar_hidden: bool,
        sidebar_width: f32,
        list_width: f32,
    ) -> Self {
        let width = if b.size.width < 740.0 {
            Width::Compact
        } else if b.size.width < 1000.0 {
            Width::Regular
        } else {
            Width::Wide
        };
        if page == Page::Compose {
            return Self {
                width,
                bounds: b,
                sidebar: None,
                list: None,
                reader: Some(b),
                composer: true,
            };
        }
        if width == Width::Compact {
            return Self {
                width,
                bounds: b,
                sidebar: None,
                list: (page != Page::Reader).then_some(b),
                reader: (page == Page::Reader).then_some(b),
                composer: false,
            };
        }
        let sw = if width == Width::Wide && !sidebar_hidden {
            sidebar_width
                .clamp(160.0, 240.0)
                .min((b.size.width - 680.0).max(0.0))
        } else {
            0.0
        };
        let lw = list_width
            .clamp(260.0, 400.0)
            .min((b.size.width - sw - 400.0 - if sw > 0.0 { 2.0 } else { 1.0 }).max(0.0));
        let list_x = b.origin.x + sw + if sw > 0.0 { 1.0 } else { 0.0 };
        let reader_x = list_x + lw + 1.0;
        Self {
            width,
            bounds: b,
            sidebar: (sw > 0.0).then(|| rect(b.origin.x, b.origin.y, sw, b.size.height)),
            list: Some(rect(list_x, b.origin.y, lw, b.size.height)),
            reader: Some(rect(
                reader_x,
                b.origin.y,
                b.origin.x + b.size.width - reader_x,
                b.size.height,
            )),
            composer: false,
        }
    }
    pub fn content(b: Rect, top: f32) -> Rect {
        rect(
            b.origin.x,
            b.origin.y + top.min(b.size.height),
            b.size.width,
            b.size.height - top,
        )
    }
    pub fn list_body(&self) -> Option<Rect> {
        self.list.map(|b| {
            Self::content(
                b,
                if self.width == Width::Compact {
                    154.0
                } else {
                    116.0
                },
            )
        })
    }
    pub fn floating_compose(&self) -> Option<Rect> {
        if self.width != Width::Compact || self.list.is_none() {
            return None;
        }
        let b = self.bounds;
        Some(rect(
            b.origin.x + (b.size.width - 116.0).max(8.0),
            b.origin.y + (b.size.height - 66.0).max(8.0),
            100.0,
            40.0,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widths_keep_readable_panes_and_compact_navigation() {
        for w in [
            320.0, 400.0, 739.0, 740.0, 900.0, 999.0, 1000.0, 1080.0, 1600.0,
        ] {
            for page in [Page::List, Page::Reader, Page::Compose] {
                let b = Rect::new(24.0, 12.0, w, 640.0);
                let l = Layout::new(b, page, false, 240.0, 400.0);
                for r in [l.sidebar, l.list, l.reader].into_iter().flatten() {
                    assert!(
                        r.origin.x >= b.origin.x
                            && r.origin.x + r.size.width <= b.origin.x + w + 0.01
                    );
                    assert!(r.size.width > 0.0);
                }
                if page == Page::Compose {
                    assert!(l.composer && l.list.is_none());
                } else if w < 740.0 {
                    assert_eq!(l.reader.is_some(), page == Page::Reader);
                    assert_eq!(l.list.is_some(), page != Page::Reader);
                } else {
                    assert!(l.reader.unwrap().size.width >= 400.0);
                }
            }
        }
    }
}
