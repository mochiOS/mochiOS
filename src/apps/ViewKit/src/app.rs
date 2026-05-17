use crate::components::VComponent;

pub type UIBuilder = Box<dyn Fn() -> VComponent + Send + Sync>;

pub struct AppBuilder {
    width: u32,
    height: u32,
    ui_fn: Option<UIBuilder>,
}

impl AppBuilder {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ui_fn: None,
        }
    }

    /// UIビルダー関数を設定（毎フレーム呼び出される）
    pub fn children<F>(mut self, ui_fn: F) -> Result<Self, String>
    where
        F: Fn() -> VComponent + Send + Sync + 'static,
    {
        self.ui_fn = Some(Box::new(ui_fn));
        Ok(self)
    }

    pub fn build(self) -> Result<UIBuilder, String> {
        self.ui_fn.ok_or("UI function not set".to_string())
    }
}
