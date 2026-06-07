use std::any::Any;

/// OSからの入力イベント
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawOSEvent {
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u8, pressed: bool },
    Key { scan_code: u32, pressed: bool },
    Resize { width: u32, height: u32 },
    Quit,
}

/// Kome言語からプロパティ経由で渡される値の抽象化
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int(i32),
    String(String),
    None,
}

pub trait WindowBackend {
    /// ウィンドウを新規生成
    ///
    /// ### Args
    /// * `width`: 横幅
    /// * `height`: 縦幅
    /// * `title`: タイトル
    /// * `decoration`: デコレーションの有無（trueであり）
    fn create_window(&mut self, width: u32, height: u32, title: &str, decoration: bool);

    /// 画面を更新する（ピクセルデータをOSやコンポジタに送り出す）
    ///
    /// ### Args
    /// * `buffer`: ARGB（1ピクセル4バイト）のカラーデータ配列
    /// * `width`:  バッファの横幅
    /// * `height`: バッファの縦幅
    fn swap_buffers(&mut self, buffer: &[u32], width: u32, height: u32);

    /// ディスプレイサーバからのOSイベントを1つ取り出す
    ///
    /// ### Return
    /// `Option<RawOSEvent>`
    fn poll_os_event(&mut self) -> Option<RawOSEvent>;

    /// トレイトオブジェクトのダウンキャスト用
    fn as_any(&self) -> &dyn Any;
}

pub trait ComponentRenderer {
    /// HTML等のテンプレート文字列から、内部的なUIレイアウトツリーを登録・パースする
    /// Kome: `@viewKit Components { dock: "..." }`
    fn register_component(&mut self, name: &str, template_html: &str) -> Result<(), String>;

    /// Komeの`recipe`の評価結果を受け取り、画面のレイアウト・描画状態を更新する
    /// * `tree_delta_json` - Kome側から渡されるUIツリーの構造データ（json）
    fn update_ui_tree(&mut self, tree_delta_json: &str);

    /// 指定されたコンポーネントのアニメーション状態や状態遷移を更新する
    /// Kome: `.selected(i == index, appIcon.selected)`
    fn set_component_property(&mut self, component_id: &str, key: &str, value: PropertyValue);
}

pub trait ViewKitBackend: WindowBackend + ComponentRenderer {}
