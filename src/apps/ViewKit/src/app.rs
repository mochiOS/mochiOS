use crate::components::VComponent;

#[cfg(all(target_os = "linux", target_env = "musl"))]
use crate::{AppControl, AppRunner, Redraw};

type UIBuilder = Box<dyn Fn() -> VComponent + Send + Sync>;

pub struct AppBuilder {
    width: u32,
    height: u32,
    ui_fn: Option<UIBuilder>,
}

pub struct ViewApp {
    width: u32,
    height: u32,
    ui_fn: UIBuilder,
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

    pub fn build(self) -> Result<ViewApp, String> {
        let ui_fn = self.ui_fn.ok_or("UI function not set".to_string())?;
        Ok(ViewApp {
            width: self.width,
            height: self.height,
            ui_fn,
        })
    }
}

impl ViewApp {
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    pub fn run(self) -> Result<(), String> {
        let runner = AppRunner::new(self.width as u16, self.height as u16)
            .map_err(|e| e.to_string())?;
        let ui_fn = self.ui_fn;
        runner
            .run(
                (),
                move |_| (ui_fn)(),
                |_, _| (AppControl::Continue, Redraw::No),
            )
            .map_err(|e| e.to_string())
    }

    #[cfg(not(all(target_os = "linux", target_env = "musl")))]
    pub fn run(self) -> Result<(), String> {
        let _ = (self.ui_fn)();
        Ok(())
    }
}
