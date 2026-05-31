//! capability（権限）定義と集合型
//!
//! 外部表現は文字列（manifest 等）で扱い、カーネル内部では enum として保持する。
//! 文字列のまま全処理すると typo や比較の取り違えが起きやすく、また高速化もしづらいため、
//! ここで変換を集中管理する。

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};

/// capability（権限）
///
/// 文字列名は `Capability::as_str()` / `Capability::from_str()` で相互変換する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    FsReadUserDocuments,
    FsWriteUserDocuments,
    FsReadUserDownloads,
    FsWriteUserDownloads,
    FsReadUserDesktop,
    FsWriteUserDesktop,
    FsReadUserPictures,
    FsWriteUserPictures,
    FsReadUserMusic,
    FsWriteUserMusic,
    FsReadUserVideos,
    FsWriteUserVideos,
    FsReadUser,
    FsWriteUser,
    FsReadTmp,
    FsWriteTmp,
    FsReadRemovable,
    FsWriteRemovable,
    FsReadAll,
    FsWriteAll,

    NetConnect,
    NetListen,
    NetRaw,

    IpcClient,
    IpcServer,

    ProcessSpawn,
    ProcessInspect,
    ProcessKill,

    WindowCreate,
    WindowOverlay,
    WindowCapture,

    DisplayRead,
    DisplayCapture,

    InputKeyboard,
    InputKeyboardGlobal,
    InputPointer,
    InputPointerGlobal,
    InputGamepad,

    AudioPlayback,
    AudioRecord,

    ClipboardRead,
    ClipboardWrite,

    NotificationSend,

    CameraAccess,
    MicrophoneAccess,
    LocationAccess,

    BluetoothAccess,
    UsbAccess,
    SerialAccess,

    PowerShutdown,
    PowerReboot,
    PowerSuspend,

    SystemTimeRead,
    SystemTimeSet,
    SystemInfoRead,
    SystemLogsRead,

    PackageInstall,
    PackageRemove,
    PackageUpdate,

    ServiceRegister,
    ServiceControl,

    VmCreate,
    VmControl,

    KernelModuleLoad,
    KernelDebug,

    DeviceGpu,
    DeviceAudio,
    DeviceInput,
    DeviceStorage,
    DeviceNet,

    AccountSelfRead,
    AccountSelfModify,
    AccountOtherRead,
    AccountOtherModify,

    SettingsRead,
    SettingsWrite,

    Unsandboxed,

    DeveloperDebug,
    DeveloperProfile,
    DeveloperTracing,
}

impl Capability {
    /// 文字列名へ変換する
    pub fn as_str(&self) -> &'static str {
        use Capability::*;
        match self {
            FsReadUserDocuments => "fs.read.user.documents",
            FsWriteUserDocuments => "fs.write.user.documents",
            FsReadUserDownloads => "fs.read.user.downloads",
            FsWriteUserDownloads => "fs.write.user.downloads",
            FsReadUserDesktop => "fs.read.user.desktop",
            FsWriteUserDesktop => "fs.write.user.desktop",
            FsReadUserPictures => "fs.read.user.pictures",
            FsWriteUserPictures => "fs.write.user.pictures",
            FsReadUserMusic => "fs.read.user.music",
            FsWriteUserMusic => "fs.write.user.music",
            FsReadUserVideos => "fs.read.user.videos",
            FsWriteUserVideos => "fs.write.user.videos",
            FsReadUser => "fs.read.user",
            FsWriteUser => "fs.write.user",
            FsReadTmp => "fs.read.tmp",
            FsWriteTmp => "fs.write.tmp",
            FsReadRemovable => "fs.read.removable",
            FsWriteRemovable => "fs.write.removable",
            FsReadAll => "fs.read.all",
            FsWriteAll => "fs.write.all",

            NetConnect => "net.connect",
            NetListen => "net.listen",
            NetRaw => "net.raw",

            IpcClient => "ipc.client",
            IpcServer => "ipc.server",

            ProcessSpawn => "process.spawn",
            ProcessInspect => "process.inspect",
            ProcessKill => "process.kill",

            WindowCreate => "window.create",
            WindowOverlay => "window.overlay",
            WindowCapture => "window.capture",

            DisplayRead => "display.read",
            DisplayCapture => "display.capture",

            InputKeyboard => "input.keyboard",
            InputKeyboardGlobal => "input.keyboard.global",
            InputPointer => "input.pointer",
            InputPointerGlobal => "input.pointer.global",
            InputGamepad => "input.gamepad",

            AudioPlayback => "audio.playback",
            AudioRecord => "audio.record",

            ClipboardRead => "clipboard.read",
            ClipboardWrite => "clipboard.write",

            NotificationSend => "notification.send",

            CameraAccess => "camera.access",
            MicrophoneAccess => "microphone.access",
            LocationAccess => "location.access",

            BluetoothAccess => "bluetooth.access",
            UsbAccess => "usb.access",
            SerialAccess => "serial.access",

            PowerShutdown => "power.shutdown",
            PowerReboot => "power.reboot",
            PowerSuspend => "power.suspend",

            SystemTimeRead => "system.time.read",
            SystemTimeSet => "system.time.set",
            SystemInfoRead => "system.info.read",
            SystemLogsRead => "system.logs.read",

            PackageInstall => "package.install",
            PackageRemove => "package.remove",
            PackageUpdate => "package.update",

            ServiceRegister => "service.register",
            ServiceControl => "service.control",

            VmCreate => "vm.create",
            VmControl => "vm.control",

            KernelModuleLoad => "kernel.module.load",
            KernelDebug => "kernel.debug",

            DeviceGpu => "device.gpu",
            DeviceAudio => "device.audio",
            DeviceInput => "device.input",
            DeviceStorage => "device.storage",
            DeviceNet => "device.net",

            AccountSelfRead => "account.self.read",
            AccountSelfModify => "account.self.modify",
            AccountOtherRead => "account.other.read",
            AccountOtherModify => "account.other.modify",

            SettingsRead => "settings.read",
            SettingsWrite => "settings.write",

            Unsandboxed => "unsandboxed",

            DeveloperDebug => "developer.debug",
            DeveloperProfile => "developer.profile",
            DeveloperTracing => "developer.tracing",
        }
    }

    /// 文字列名から変換する（不明な文字列は `None`）
    pub fn from_str(s: &str) -> Option<Self> {
        use Capability::*;
        let cap = match s {
            "fs.read.user.documents" => FsReadUserDocuments,
            "fs.write.user.documents" => FsWriteUserDocuments,
            "fs.read.user.downloads" => FsReadUserDownloads,
            "fs.write.user.downloads" => FsWriteUserDownloads,
            "fs.read.user.desktop" => FsReadUserDesktop,
            "fs.write.user.desktop" => FsWriteUserDesktop,
            "fs.read.user.pictures" => FsReadUserPictures,
            "fs.write.user.pictures" => FsWriteUserPictures,
            "fs.read.user.music" => FsReadUserMusic,
            "fs.write.user.music" => FsWriteUserMusic,
            "fs.read.user.videos" => FsReadUserVideos,
            "fs.write.user.videos" => FsWriteUserVideos,
            "fs.read.user" => FsReadUser,
            "fs.write.user" => FsWriteUser,
            "fs.read.tmp" => FsReadTmp,
            "fs.write.tmp" => FsWriteTmp,
            "fs.read.removable" => FsReadRemovable,
            "fs.write.removable" => FsWriteRemovable,
            "fs.read.all" => FsReadAll,
            "fs.write.all" => FsWriteAll,

            "net.connect" => NetConnect,
            "net.listen" => NetListen,
            "net.raw" => NetRaw,

            "ipc.client" => IpcClient,
            "ipc.server" => IpcServer,

            "process.spawn" => ProcessSpawn,
            "process.inspect" => ProcessInspect,
            "process.kill" => ProcessKill,

            "window.create" => WindowCreate,
            "window.overlay" => WindowOverlay,
            "window.capture" => WindowCapture,

            "display.read" => DisplayRead,
            "display.capture" => DisplayCapture,

            "input.keyboard" => InputKeyboard,
            "input.keyboard.global" => InputKeyboardGlobal,
            "input.pointer" => InputPointer,
            "input.pointer.global" => InputPointerGlobal,
            "input.gamepad" => InputGamepad,

            "audio.playback" => AudioPlayback,
            "audio.record" => AudioRecord,

            "clipboard.read" => ClipboardRead,
            "clipboard.write" => ClipboardWrite,

            "notification.send" => NotificationSend,

            "camera.access" => CameraAccess,
            "microphone.access" => MicrophoneAccess,
            "location.access" => LocationAccess,

            "bluetooth.access" => BluetoothAccess,
            "usb.access" => UsbAccess,
            "serial.access" => SerialAccess,

            "power.shutdown" => PowerShutdown,
            "power.reboot" => PowerReboot,
            "power.suspend" => PowerSuspend,

            "system.time.read" => SystemTimeRead,
            "system.time.set" => SystemTimeSet,
            "system.info.read" => SystemInfoRead,
            "system.logs.read" => SystemLogsRead,

            "package.install" => PackageInstall,
            "package.remove" => PackageRemove,
            "package.update" => PackageUpdate,

            "service.register" => ServiceRegister,
            "service.control" => ServiceControl,

            "vm.create" => VmCreate,
            "vm.control" => VmControl,

            "kernel.module.load" => KernelModuleLoad,
            "kernel.debug" => KernelDebug,

            "device.gpu" => DeviceGpu,
            "device.audio" => DeviceAudio,
            "device.input" => DeviceInput,
            "device.storage" => DeviceStorage,
            "device.net" => DeviceNet,

            "account.self.read" => AccountSelfRead,
            "account.self.modify" => AccountSelfModify,
            "account.other.read" => AccountOtherRead,
            "account.other.modify" => AccountOtherModify,

            "settings.read" => SettingsRead,
            "settings.write" => SettingsWrite,

            "unsandboxed" => Unsandboxed,

            "developer.debug" => DeveloperDebug,
            "developer.profile" => DeveloperProfile,
            "developer.tracing" => DeveloperTracing,

            _ => return None,
        };
        Some(cap)
    }
}

/// `parent` が `child` を含意するか（階層継承）
///
/// ここでの含意は「より広い権限が、より細かい権限を内包する」関係を表す。
/// 例: `fs.read.all` は `fs.read.user.documents` を含意する。
pub fn capability_implies(parent: Capability, child: Capability) -> bool {
    use Capability::*;

    if parent == child {
        return true;
    }

    match parent {
        // unsandboxed は明示的に「すべて」を許可する最後の手段。
        // これを持つプロセスは隔離を回避できるため、付与経路は信頼済みでなければならない。
        Unsandboxed => true,

        FsReadAll => matches!(
            child,
            FsReadUser
                | FsReadUserDocuments
                | FsReadUserDownloads
                | FsReadUserDesktop
                | FsReadUserPictures
                | FsReadUserMusic
                | FsReadUserVideos
                | FsReadTmp
                | FsReadRemovable
        ),

        FsWriteAll => matches!(
            child,
            FsWriteUser
                | FsWriteUserDocuments
                | FsWriteUserDownloads
                | FsWriteUserDesktop
                | FsWriteUserPictures
                | FsWriteUserMusic
                | FsWriteUserVideos
                | FsWriteTmp
                | FsWriteRemovable
        ),

        FsReadUser => matches!(
            child,
            FsReadUserDocuments
                | FsReadUserDownloads
                | FsReadUserDesktop
                | FsReadUserPictures
                | FsReadUserMusic
                | FsReadUserVideos
        ),

        FsWriteUser => matches!(
            child,
            FsWriteUserDocuments
                | FsWriteUserDownloads
                | FsWriteUserDesktop
                | FsWriteUserPictures
                | FsWriteUserMusic
                | FsWriteUserVideos
        ),

        _ => false,
    }
}

/// capability の集合
#[derive(Clone, Debug, Default)]
pub struct CapabilitySet {
    caps: BTreeSet<Capability>,
}

/// capability 文字列の解析エラー
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityParseError {
    /// 未知の capability 名
    UnknownCapability { name: String },
}

impl CapabilitySet {
    /// 空集合
    pub fn empty() -> Self {
        Self {
            caps: BTreeSet::new(),
        }
    }

    /// capability を追加
    pub fn insert(&mut self, cap: Capability) {
        self.caps.insert(cap);
    }

    /// 完全一致で含まれるか
    pub fn contains_exact(&self, cap: Capability) -> bool {
        self.caps.contains(&cap)
    }

    /// 含意（階層継承）を考慮して含まれるか
    pub fn contains(&self, cap: Capability) -> bool {
        self.implies(cap)
    }

    /// この集合が `cap` を満たすか（階層継承を含む）
    pub fn implies(&self, cap: Capability) -> bool {
        self.caps
            .iter()
            .copied()
            .any(|parent| capability_implies(parent, cap))
    }

    /// 文字列リストから生成
    pub fn from_strings(list: &[String]) -> Result<Self, CapabilityParseError> {
        let mut set = Self::empty();
        for s in list {
            let Some(cap) = Capability::from_str(s.as_str()) else {
                return Err(CapabilityParseError::UnknownCapability { name: s.to_string() });
            };
            set.insert(cap);
        }
        Ok(set)
    }
}

