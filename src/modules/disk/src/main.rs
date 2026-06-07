#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

#[repr(C)]
pub struct McxDiskOps {
    pub probe: extern "C" fn() -> i32,
    pub read_sector: extern "C" fn(disk_id: u32, lba: u64, buf: *mut u8, buf_len: usize) -> i32,
    pub write_sector:
        extern "C" fn(disk_id: u32, lba: u64, buf: *const u8, buf_len: usize) -> i32,
}

// ---- ATA PIO (最低限) -------------------------------------------------------

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
    value
}

#[inline]
unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

#[derive(Clone, Copy)]
struct AtaPorts {
    data: u16,
    error_features: u16,
    sector_count: u16,
    lba_low: u16,
    lba_mid: u16,
    lba_high: u16,
    drive_head: u16,
    status_command: u16,
    control: u16,
}

impl AtaPorts {
    const PRIMARY: Self = Self {
        data: 0x1F0,
        error_features: 0x1F1,
        sector_count: 0x1F2,
        lba_low: 0x1F3,
        lba_mid: 0x1F4,
        lba_high: 0x1F5,
        drive_head: 0x1F6,
        status_command: 0x1F7,
        control: 0x3F6,
    };

    const SECONDARY: Self = Self {
        data: 0x170,
        error_features: 0x171,
        sector_count: 0x172,
        lba_low: 0x173,
        lba_mid: 0x174,
        lba_high: 0x175,
        drive_head: 0x176,
        status_command: 0x177,
        control: 0x376,
    };
}

mod status {
    pub const ERR: u8 = 1 << 0;
    pub const DRQ: u8 = 1 << 3;
    pub const DRDY: u8 = 1 << 6;
    pub const BSY: u8 = 1 << 7;
}

mod command {
    pub const READ_SECTORS: u8 = 0x20;
    pub const WRITE_SECTORS: u8 = 0x30;
    pub const IDENTIFY: u8 = 0xEC;
    pub const FLUSH_CACHE: u8 = 0xE7;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriveType {
    Master,
    Slave,
}

#[derive(Clone, Copy)]
struct AtaDrive {
    ports: AtaPorts,
    drive_type: DriveType,
    sectors: u64,
    present: bool,
}

impl AtaDrive {
    const fn new(ports: AtaPorts, drive_type: DriveType) -> Self {
        Self {
            ports,
            drive_type,
            sectors: 0,
            present: false,
        }
    }

    fn select_drive(&self) {
        let dev = match self.drive_type {
            DriveType::Master => 0xA0u8,
            DriveType::Slave => 0xB0u8,
        };
        unsafe { outb(self.ports.drive_head, dev) };
    }

    fn wait_400ns(&self) {
        // Alternate status を4回読むと約400ns
        unsafe {
            let _ = inb(self.ports.control);
            let _ = inb(self.ports.control);
            let _ = inb(self.ports.control);
            let _ = inb(self.ports.control);
        }
    }

    fn read_status(&self) -> u8 {
        unsafe { inb(self.ports.status_command) }
    }

    fn wait_not_busy(&self) -> bool {
        for _ in 0..1_000_000 {
            let st = self.read_status();
            if st & status::BSY == 0 {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn wait_drq_or_err(&self) -> bool {
        for _ in 0..1_000_000 {
            let st = self.read_status();
            if st & status::ERR != 0 {
                return false;
            }
            if st & status::DRQ != 0 {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn write_command(&self, cmd: u8) {
        unsafe { outb(self.ports.status_command, cmd) }
    }

    fn init(&mut self) -> bool {
        self.select_drive();
        self.wait_400ns();
        self.write_command(command::IDENTIFY);

        let st = self.read_status();
        if st == 0 {
            self.present = false;
            return false;
        }

        if !self.wait_not_busy() {
            self.present = false;
            return false;
        }
        if !self.wait_drq_or_err() {
            self.present = false;
            return false;
        }

        // IDENTIFY 256 words
        let mut identify = [0u16; 256];
        for w in &mut identify {
            *w = unsafe { inw(self.ports.data) };
        }

        let lba28 = ((identify[61] as u64) << 16) | identify[60] as u64;
        self.sectors = if lba28 != 0 {
            lba28
        } else {
            ((identify[103] as u64) << 48)
                | ((identify[102] as u64) << 32)
                | ((identify[101] as u64) << 16)
                | (identify[100] as u64)
        };
        self.present = true;
        true
    }

    fn write_lba28(&self, lba: u64) {
        unsafe {
            outb(self.ports.sector_count, 1);
            outb(self.ports.lba_low, (lba & 0xFF) as u8);
            outb(self.ports.lba_mid, ((lba >> 8) & 0xFF) as u8);
            outb(self.ports.lba_high, ((lba >> 16) & 0xFF) as u8);
            let head = match self.drive_type {
                DriveType::Master => 0xE0u8,
                DriveType::Slave => 0xF0u8,
            } | (((lba >> 24) & 0x0F) as u8);
            outb(self.ports.drive_head, head);
        }
    }

    fn read_sector(&self, lba: u64, out: &mut [u8; 512]) -> bool {
        if !self.present || lba >= (1 << 28) {
            return false;
        }
        self.select_drive();
        self.wait_400ns();
        self.write_lba28(lba);
        self.write_command(command::READ_SECTORS);
        if !self.wait_not_busy() || !self.wait_drq_or_err() {
            return false;
        }
        let words = unsafe {
            core::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut u16, 256)
        };
        for w in words.iter_mut() {
            *w = unsafe { inw(self.ports.data) };
        }
        true
    }

    fn write_sector(&self, lba: u64, input: &[u8; 512]) -> bool {
        if !self.present || lba >= (1 << 28) {
            return false;
        }
        self.select_drive();
        self.wait_400ns();
        self.write_lba28(lba);
        self.write_command(command::WRITE_SECTORS);
        if !self.wait_not_busy() || !self.wait_drq_or_err() {
            return false;
        }
        let words = unsafe { core::slice::from_raw_parts(input.as_ptr() as *const u16, 256) };
        for &w in words {
            unsafe { outw(self.ports.data, w) };
        }
        // flush はベストエフォート
        self.write_command(command::FLUSH_CACHE);
        let _ = self.wait_not_busy();
        true
    }
}

const MAX_DISKS: usize = 4;
static ATA_LOCK: AtomicBool = AtomicBool::new(false);
static INIT: AtomicBool = AtomicBool::new(false);
static mut DISKS: [AtaDrive; MAX_DISKS] = [
    AtaDrive::new(AtaPorts::PRIMARY, DriveType::Master),
    AtaDrive::new(AtaPorts::PRIMARY, DriveType::Slave),
    AtaDrive::new(AtaPorts::SECONDARY, DriveType::Master),
    AtaDrive::new(AtaPorts::SECONDARY, DriveType::Slave),
];

fn ensure_init() {
    // 初期化と I/O の両方で ATA コマンド列が競合しないようにロックする。
    while ATA_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            ATA_LOCK.store(false, Ordering::Release);
        }
    }
    let _g = Guard;

    if INIT.load(Ordering::Acquire) {
        return;
    }
    unsafe {
        for d in DISKS.iter_mut() {
            let _ = d.init();
        }
    }
    INIT.store(true, Ordering::Release);
}

extern "C" fn probe() -> i32 {
    ensure_init();
    0
}

extern "C" fn read_sector(disk_id: u32, lba: u64, buf: *mut u8, buf_len: usize) -> i32 {
    if buf.is_null() || buf_len < 512 {
        return -22; // EINVAL
    }
    ensure_init();
    let idx = disk_id as usize;
    if idx >= MAX_DISKS {
        return -22;
    }
    let mut tmp = [0u8; 512];
    // I/O もロックして ATA コマンド競合を防ぐ
    while ATA_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            ATA_LOCK.store(false, Ordering::Release);
        }
    }
    let _g = Guard;
    let ok = unsafe { DISKS[idx].read_sector(lba, &mut tmp) };
    if !ok {
        return -5; // EIO
    }
    unsafe {
        core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf, 512);
    }
    0
}

extern "C" fn write_sector(disk_id: u32, lba: u64, buf: *const u8, buf_len: usize) -> i32 {
    if buf.is_null() || buf_len < 512 {
        return -22;
    }
    ensure_init();
    let idx = disk_id as usize;
    if idx >= MAX_DISKS {
        return -22;
    }
    let mut tmp = [0u8; 512];
    unsafe {
        core::ptr::copy_nonoverlapping(buf, tmp.as_mut_ptr(), 512);
    }
    while ATA_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            ATA_LOCK.store(false, Ordering::Release);
        }
    }
    let _g = Guard;
    let ok = unsafe { DISKS[idx].write_sector(lba, &tmp) };
    if !ok {
        return -5;
    }
    0
}

static DISK_OPS: McxDiskOps = McxDiskOps {
    probe,
    read_sector,
    write_sector,
};

#[no_mangle]
pub extern "C" fn mochi_module_init() -> *const McxDiskOps {
    &DISK_OPS
}

#[used]
static KEEP_INIT_REF: extern "C" fn() -> *const McxDiskOps = mochi_module_init;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
