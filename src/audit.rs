//! 監査ログ
//!
//! panic の代わりに、拒否・隔離・破損検出・復旧不能な局所失敗を
//! append-only な簡易リングバッファへ記録する。

use core::fmt;
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::vec::Vec;

use crate::interrupt::spinlock::SpinLock;

const AUDIT_CAPACITY: usize = 256;
const AUDIT_MSG_LEN: usize = 160;
const AUDIT_FILE_CAPACITY: usize = 64 * 1024;
const AUDIT_LINE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventKind {
    Deny,
    Fault,
    Revoke,
    Quarantine,
    Restart,
    Policy,
    Usercopy,
    Device,
    Exec,
    Ipc,
    Memory,
}

impl AuditEventKind {
    fn as_str(self) -> &'static str {
        match self {
            AuditEventKind::Deny => "Deny",
            AuditEventKind::Fault => "Fault",
            AuditEventKind::Revoke => "Revoke",
            AuditEventKind::Quarantine => "Quarantine",
            AuditEventKind::Restart => "Restart",
            AuditEventKind::Policy => "Policy",
            AuditEventKind::Usercopy => "Usercopy",
            AuditEventKind::Device => "Device",
            AuditEventKind::Exec => "Exec",
            AuditEventKind::Ipc => "Ipc",
            AuditEventKind::Memory => "Memory",
        }
    }
}

#[derive(Clone, Copy)]
pub struct AuditRecord {
    seq: u64,
    kind: AuditEventKind,
    len: usize,
    msg: [u8; AUDIT_MSG_LEN],
}

impl AuditRecord {
    const fn empty() -> Self {
        Self {
            seq: 0,
            kind: AuditEventKind::Fault,
            len: 0,
            msg: [0; AUDIT_MSG_LEN],
        }
    }

    fn write_message(&mut self, message: &str) {
        let bytes = message.as_bytes();
        let len = bytes.len().min(AUDIT_MSG_LEN);
        self.msg[..len].copy_from_slice(&bytes[..len]);
        if len < AUDIT_MSG_LEN {
            self.msg[len..].fill(0);
        }
        self.len = len;
    }

    pub fn message(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.len]).unwrap_or("<invalid-audit-utf8>")
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn kind(&self) -> AuditEventKind {
        self.kind
    }
}

impl fmt::Debug for AuditRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuditRecord")
            .field("seq", &self.seq)
            .field("kind", &self.kind)
            .field("message", &self.message())
            .finish()
    }
}

struct AuditFile {
    data: [u8; AUDIT_FILE_CAPACITY],
    start: usize,
    len: usize,
}

impl AuditFile {
    const fn new() -> Self {
        Self {
            data: [0; AUDIT_FILE_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        if bytes.len() >= AUDIT_FILE_CAPACITY {
            let tail = &bytes[bytes.len() - AUDIT_FILE_CAPACITY..];
            self.data.copy_from_slice(tail);
            self.start = 0;
            self.len = AUDIT_FILE_CAPACITY;
            return;
        }

        while self.len + bytes.len() > AUDIT_FILE_CAPACITY {
            self.start = (self.start + 1) % AUDIT_FILE_CAPACITY;
            self.len -= 1;
        }

        let mut write_idx = (self.start + self.len) % AUDIT_FILE_CAPACITY;
        for &b in bytes {
            self.data[write_idx] = b;
            write_idx = (write_idx + 1) % AUDIT_FILE_CAPACITY;
        }
        self.len += bytes.len();
    }

    fn append_record(&mut self, seq: u64, kind: AuditEventKind, message: &str) {
        let mut line = [0u8; AUDIT_LINE_CAPACITY];
        let mut writer = SliceWriter::new(&mut line);
        let _ = write!(
            &mut writer,
            "[AUDIT {} #{}] {}\n",
            kind.as_str(),
            seq,
            message
        );
        self.push_bytes(writer.as_bytes());
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> usize {
        if offset >= self.len || out.is_empty() {
            return 0;
        }
        let to_read = core::cmp::min(out.len(), self.len - offset);
        for i in 0..to_read {
            let idx = (self.start + offset + i) % AUDIT_FILE_CAPACITY;
            out[i] = self.data[idx];
        }
        to_read
    }

    fn size(&self) -> usize {
        self.len
    }
}

struct SliceWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.pos]
    }
}

impl fmt::Write for SliceWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len().saturating_sub(self.pos);
        if bytes.len() > remaining {
            let take = remaining;
            self.buf[self.pos..self.pos + take].copy_from_slice(&bytes[..take]);
            self.pos += take;
            return Err(fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

static AUDIT_LOG: SpinLock<[AuditRecord; AUDIT_CAPACITY]> =
    SpinLock::new([AuditRecord::empty(); AUDIT_CAPACITY]);
static AUDIT_FILE: SpinLock<AuditFile> = SpinLock::new(AuditFile::new());
static AUDIT_SEQ: AtomicUsize = AtomicUsize::new(1);
static AUDIT_FLUSH_TICK: AtomicUsize = AtomicUsize::new(0);

pub fn log(kind: AuditEventKind, message: &str) {
    let seq = AUDIT_SEQ.fetch_add(1, Ordering::Relaxed) as u64;
    let idx = (seq as usize) % AUDIT_CAPACITY;
    {
        let mut log = AUDIT_LOG.lock();
        let slot = &mut log[idx];
        slot.seq = seq;
        slot.kind = kind;
        slot.write_message(message);
    }
    {
        let mut file = AUDIT_FILE.lock();
        file.append_record(seq, kind, message);
    }
    maybe_flush_to_disk(kind);
    crate::warn!("[AUDIT {:?} #{seq}] {}", kind, message);
}

pub fn snapshot_into(out: &mut [AuditRecord]) -> usize {
    if out.is_empty() {
        return 0;
    }
    let next_seq = AUDIT_SEQ.load(Ordering::Acquire) as u64;
    let start_seq = next_seq.saturating_sub(AUDIT_CAPACITY as u64).max(1);
    let log = AUDIT_LOG.lock();
    let mut written = 0;
    for seq in start_seq..next_seq {
        if written >= out.len() {
            break;
        }
        let idx = (seq as usize) % AUDIT_CAPACITY;
        if log[idx].seq == seq {
            out[written] = log[idx];
            written += 1;
        }
    }
    written
}

pub fn file_size() -> usize {
    AUDIT_FILE.lock().size()
}

pub fn read_file_at(offset: usize, out: &mut [u8]) -> usize {
    AUDIT_FILE.lock().read_at(offset, out)
}

fn should_force_flush(kind: AuditEventKind) -> bool {
    matches!(
        kind,
        AuditEventKind::Fault
            | AuditEventKind::Quarantine
            | AuditEventKind::Memory
            | AuditEventKind::Deny
            | AuditEventKind::Policy
    )
}

fn maybe_flush_to_disk(kind: AuditEventKind) {
    if !crate::kmod::fs::is_loaded() {
        return;
    }

    let tick = AUDIT_FLUSH_TICK.fetch_add(1, Ordering::Relaxed);
    if !should_force_flush(kind) && tick % 16 != 0 {
        return;
    }
    flush_to_disk();
}

pub fn flush_to_disk() {
    if !crate::kmod::fs::is_loaded() {
        return;
    }

    let mut snapshot = Vec::new();
    {
        let file = AUDIT_FILE.lock();
        snapshot.resize(file.size(), 0);
        let _ = file.read_at(0, &mut snapshot);
    }
    if snapshot.len() < AUDIT_FILE_CAPACITY {
        snapshot.resize(AUDIT_FILE_CAPACITY, 0);
    }
    let _ = crate::kmod::fs::write_all("/log/audit.log", 0, &snapshot);
}
