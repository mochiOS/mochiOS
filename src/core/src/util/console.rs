//! シリアルポート出力
//!
//! シリアルポートを使用したシリアル出力

use core::fmt;
use spin::Mutex;
use x86_64::instructions::port::Port;

/// シリアルポート (COM1) - 割込み対応版
pub static SERIAL: Mutex<SerialPort> = Mutex::new(SerialPort::new(0x3F8));

/// 割込み無効化を伴うロック取得
fn lock_serial<F, R>(f: F) -> R
where
    F: FnOnce(&mut SerialPort) -> R,
{
    // 割込みを無効化してロック取得
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut serial = SERIAL.lock();
        f(&mut serial)
    })
}

/// UARTシリアルポート
pub struct SerialPort {
    /// データ
    data: Port<u8>,
    /// 割り込み有効化
    int_en: Port<u8>,
    /// FIFO制御
    fifo_ctrl: Port<u8>,
    /// ライン制御
    line_ctrl: Port<u8>,
    /// モデム制御
    modem_ctrl: Port<u8>,
    /// ラインステータス
    line_status: Port<u8>,
}

impl SerialPort {
    /// 新しいシリアルポートを作成
    const fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_en: Port::new(base + 1),
            fifo_ctrl: Port::new(base + 2),
            line_ctrl: Port::new(base + 3),
            modem_ctrl: Port::new(base + 4),
            line_status: Port::new(base + 5),
        }
    }

    /// シリアルポートを初期化
    pub fn init(&mut self) {
        unsafe {
            // 割り込み無効
            self.int_en.write(0x00);
            // ボーレート設定を有効化
            self.line_ctrl.write(0x80);
            // ボーレート = 38400 (divisor = 3)
            self.data.write(0x03);
            self.int_en.write(0x00);
            // 8ビット, パリティなし, 1ストップビット
            self.line_ctrl.write(0x03);
            // FIFOを有効化, クリア, 14バイトしきい値
            self.fifo_ctrl.write(0xC7);
            // データ端末レディ, リクエスト送信
            self.modem_ctrl.write(0x0B);
        }
    }

    /// 1バイト送信
    pub fn send_byte(&mut self, byte: u8) {
        unsafe {
            // 送信準備完了を待つ
            while self.line_status.read() & 0x20 == 0 {}
            self.data.write(byte);
        }
    }

    /// 文字列を送信
    pub fn send_str(&mut self, s: &str) {
        for byte in s.bytes() {
            self.send_byte(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.send_str(s);
        Ok(())
    }
}

/// シリアルポートを初期化
pub fn init() {
    lock_serial(|serial| serial.init());
}

/// シリアルポートに文字列を出力（割込み対応）
pub fn print(args: fmt::Arguments) {
    use core::fmt::Write;
    lock_serial(|serial| {
        let _ = serial.write_fmt(args);
    });
}

/// シリアル出力マクロ
#[macro_export]
macro_rules! sprint {
    ($($arg:tt)*) => {
        $crate::util::console::print(format_args!($($arg)*))
    };
}

/// 改行付きのシリアル出力マクロ
#[macro_export]
macro_rules! sprintln {
    () => ($crate::sprint!("\n"));
    ($($arg:tt)*) => {
        $crate::sprint!("{}\n", format_args!($($arg)*))
    };
}
