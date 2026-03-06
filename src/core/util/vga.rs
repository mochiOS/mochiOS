//! フレームバッファ出力

use core::fmt;
use core::fmt::Write;
use spin::{Mutex, Once};

static FB_INFO: Once<FramebufferInfo> = Once::new();

/// フレームバッファ
#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    /// アドレス
    pub addr: u64,
    ///　横幅
    pub width: usize,
    /// 高さ
    pub height: usize,
    ///　行当たりのバイト数
    pub stride: usize,
}

/// フォント縦サイズ
const FONT_HEIGHT: usize = 16;
/// フォント横サイズ
const FONT_WIDTH: usize = 8;

/// フレームバッファライター
pub struct Writer {
    /// 縦
    col: usize,
    /// 横
    row: usize,
    /// 最大縦
    max_cols: usize,
    /// 最大横
    max_rows: usize,
}

impl Writer {
    /// 新規ライターを作成
    fn new(info: &FramebufferInfo) -> Self {
        let max_cols = info.width / FONT_WIDTH;
        let max_rows = info.height / FONT_HEIGHT;

        Self {
            col: 0,
            row: 0,
            max_cols,
            max_rows,
        }
    }

    /// ピクセルを描画
    fn put_pixel(&self, x: usize, y: usize, color: u32) {
        if let Some(info) = FB_INFO.get() {
            // 境界チェック
            if x >= info.width || y >= info.height {
                return;
            }

            let offset = y * info.stride + x;
            let fb_ptr = info.addr as *mut u32;

            unsafe {
                fb_ptr.add(offset).write_volatile(color);
            }
        }
    }

    /// 文字を描画
    fn draw_char(&self, ch: u8, x: usize, y: usize, fg: u32, bg: u32) {
        // TODO: 現状は短径描画してるだけなのでフォント描画するようにする
        for row in 0..FONT_HEIGHT {
            for col in 0..FONT_WIDTH {
                let is_char = ch != b' '
                    && (row == 0 || row == FONT_HEIGHT - 1 || col == 0 || col == FONT_WIDTH - 1);
                let color = if is_char { fg } else { bg };
                self.put_pixel(x + col, y + row, color);
            }
        }
    }

    /// 1バイト書き込み
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.col >= self.max_cols {
                    self.new_line();
                }

                let x = self.col * FONT_WIDTH;
                let y = self.row * FONT_HEIGHT;
                self.draw_char(byte, x, y, 0xFFFFFF, 0x000000); // 白文字、黒背景

                self.col += 1;
            }
        }
    }

    /// 文字列を書き込み
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0x3F), // 非対応文字は'?'
            }
        }
    }

    /// 改行処理
    fn new_line(&mut self) {
        // TODO: 現状画面クリアするだけなので作り直す
        self.row += 1;
        self.col = 0;
        if self.row >= self.max_rows {
            self.clear_screen();
        }
    }

    /// 画面全体をクリア
    pub fn clear_screen(&mut self) {
        if let Some(info) = FB_INFO.get() {
            let fb_ptr = info.addr as *mut u32;

            let total_pixels = info.height * info.width;
            unsafe {
                for i in 0..total_pixels {
                    fb_ptr.add(i).write_volatile(0x000000); // 黒
                }
            }
        }
        self.row = 0;
        self.col = 0;
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// グローバルライター
static WRITER: Once<Mutex<Writer>> = Once::new();

/// フレームバッファ情報を返す（syscall 用）
pub fn get_info() -> Option<FramebufferInfo> {
    FB_INFO.get().copied()
}

/// フレームバッファを初期化
pub fn init(addr: u64, width: usize, height: usize, stride: usize) {
    FB_INFO.call_once(|| FramebufferInfo {
        addr,
        width,
        height,
        stride,
    });

    if let Some(info) = FB_INFO.get() {
        WRITER.call_once(|| Mutex::new(Writer::new(info)));

        // 画面をクリア
        if let Some(writer) = WRITER.get() {
            writer.lock().clear_screen();
        }
    }
}

/// フレームバッファに文字列を出力
pub fn print(args: fmt::Arguments) {
    if let Some(writer) = WRITER.get() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let _ = writer.lock().write_fmt(args);
        });
    }
}

/// フレームバッファ出力マクロ
#[macro_export]
macro_rules! vprint {
    ($($arg:tt)*) => {
        $crate::util::vga::print(format_args!($($arg)*))
    };
}

/// 改行付きフレームバッファ出力マクロ
#[macro_export]
macro_rules! vprintln {
    () => ($crate::vprint!("\n"));
    ($($arg:tt)*) => {
        $crate::vprint!("{}\n", format_args!($($arg)*))
    };
}
