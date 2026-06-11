#![no_std]

/// システムコール番号 (Linux x86_64 互換 + mochiOS 拡張)
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Read = 0,
    Write = 1,
    Open = 2,
    Close = 3,
    Stat = 4,
    Fstat = 5,
    Lstat = 6,
    Poll = 7,
    Lseek = 8,
    Mmap = 9,
    Mprotect = 10,
    Munmap = 11,
    Brk = 12,
    RtSigaction = 13,
    RtSigprocmask = 14,
    RtSigreturn = 15,
    Ioctl = 16,
    Readv = 19,
    Writev = 20,
    Access = 21,
    Pipe = 22,
    Select = 23,
    Dup = 32,
    Dup2 = 33,
    Nanosleep = 35,
    GetPid = 39,
    Fork = 57,
    Clone = 56,
    Execve = 59,
    Exit = 60,
    Wait = 61,
    Kill = 62,
    Uname = 63,
    Fcntl = 72,
    Fsync = 74,
    Fdatasync = 75,
    Truncate = 76,
    Ftruncate = 77,
    Getcwd = 79,
    Unlink = 87,
    Readlink = 89,
    Getuid = 102,
    Getgid = 104,
    Geteuid = 107,
    Getegid = 108,
    Setpgid = 109,
    GetPpid = 110,
    Setsid = 112,
    Getpgid = 121,
    Getsid = 124,
    Sigaltstack = 131,
    Statfs = 137,
    ArchPrctl = 158,
    GetTid = 186,
    Tkill = 200,
    Futex = 202,
    SetTidAddress = 218,
    ClockGettime = 228,
    ExitGroup = 231,
    Tgkill = 234,
    Openat = 257,
    Newfstatat = 262,
    Unlinkat = 263,
    Renameat = 264,
    Readlinkat = 267,
    Faccessat = 269,
    Pselect6 = 270,
    Ppoll = 271,
    SetRobustList = 273,
    Pipe2 = 293,
    Prlimit64 = 302,
    Getrandom = 318,
    Getrlimit = 97,
    Getdents64 = 217,

    Yield = 512,
    GetTicks = 513,
    IpcSend = 514,
    IpcRecv = 515,
    Exec = 516,
    Sleep = 517,
    FindProcessByName = 518,
    Log = 519,
    PortIn = 520,
    PortOut = 521,
    Mkdir = 522,
    Rmdir = 523,
    Readdir = 524,
    Chdir = 525,
    KeyboardRead = 526,
    GetThreadPrivilege = 527,
    GetFramebufferInfo = 528,
    MapFramebuffer = 529,
    ExecFromBuffer = 530,
    SetConsoleCursor = 531,
    GetConsoleCursor = 532,
    IpcRecvWait = 533,
    KeyboardReadTap = 534,
    MouseRead = 535,
    MapPhysicalRange = 536,
    VirtToPhys = 537,
    PortInWords = 538,
    PortOutWords = 539,
    KeyboardInject = 540,
    MouseInject = 541,
    ExecFromBufferNamed = 542,
    ExecFromBufferNamedArgs = 543,
    ExecFromBufferNamedArgsWithRequester = 544,
    ExecFromFsStream = 545,
    MapPhysicalPages = 546,
    GetPhysicalAddr = 547,
    AllocSharedPages = 548,
    UnmapPages = 549,
    IpcSendPages = 550,
    MouseReadWait = 551,
    ListProcesses = 552,
    CheckThreadCapability = 553,
    ExecWithCapabilities = 554,
    BlockRead = 555,
    BlockWrite = 556,
    KeyboardReadWait = 557,
    CheckGravityExist = 999,
}

/// 成功
pub const SUCCESS: u64 = 0;
/// 操作が許可されていない
pub const EPERM: u64 = (-1i64) as u64;
/// ファイルが見つからない
pub const ENOENT: u64 = (-2i64) as u64;
/// プロセスが見つからない
pub const ESRCH: u64 = (-3i64) as u64;
/// I/Oエラー
pub const EIO: u64 = (-5i64) as u64;
/// デバイスが見つからない
pub const ENXIO: u64 = (-6i64) as u64;
/// 不正なファイルディスクリプタ
pub const EBADF: u64 = (-9i64) as u64;
/// 受信/送信できない（キュー空/満杯）
pub const EAGAIN: u64 = (-11i64) as u64;
/// メモリ不足
pub const ENOMEM: u64 = (-12i64) as u64;
/// アクセス権がない
pub const EACCES: u64 = (-13i64) as u64;
/// 不正なアドレス
pub const EFAULT: u64 = (-14i64) as u64;
/// ファイルが既に存在する
pub const EEXIST: u64 = (-17i64) as u64;
/// ディレクトリではない
pub const ENOTDIR: u64 = (-20i64) as u64;
/// 無効な引数
pub const EINVAL: u64 = (-22i64) as u64;
/// ファイルディスクリプタが多すぎる
pub const EMFILE: u64 = (-24i64) as u64;
/// デバイスでない
pub const ENOTTY: u64 = (-25i64) as u64;
/// パイプが壊れている
pub const EPIPE: u64 = (-32i64) as u64;
/// 引数が範囲外
pub const ERANGE: u64 = (-34i64) as u64;
/// 未実装
pub const ENOSYS: u64 = (-38i64) as u64;
/// データがない / ノンブロッキングで読み出しできない
pub const ENODATA: u64 = (-61i64) as u64;
/// 操作がサポートされていない
pub const ENOTSUP: u64 = (-95i64) as u64;
