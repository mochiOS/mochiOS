mochiOS の capability システムを実装してください。

前提:

* Rust 製の自作OSです。
* サービスは現在 `src/services/` にあります。
* `capability.service` を追加・整備します。
* 既存サービスもこの機会に `XXX.service/entry.rs` 形式のディレクトリ構造へ移行します。
* アプリだけでなく、サービス自体にも capability を持たせます。
* カーネル側の拡張は最低限にしてください。
* コメントはすべて日本語で書いてください。
* capability 一覧は TOML 形式で、fs/net/ipc/process/window/display/input/audio/clipboard/device/system/package/service/kernel/developer/unsandboxed などを含みます。
* 例: `fs.read.user.documents`, `fs.write.user.documents`, `net.connect`, `window.create`, `kernel.module.load`, `unsandboxed` など。
* 元の一覧では Documents/Downloads/Desktop/Pictures/Music/Videos/tmp/removable/all、ネットワーク、IPC、プロセス、ウィンドウ、入力、音声、カメラ、マイク、位置情報、USB、電源、パッケージ、サービス制御、カーネル、開発者機能まで定義されています。

目的:

* `capability.service` を「許可の管理・判定サービス」として実装する。
* 実際の強制は `fs.service` / `net.service` / `window.service` / `process.service` / `device.service` など各サービス側でも必ず行う。
* アプリ・サービスの manifest に capability 要求を書けるようにする。
* 起動時に init/core.service が manifest を読み、`capability.service` に問い合わせ、プロセスに `CapabilitySet` を付与する。
* User/Service プロセスが自分で capability を増やせないようにする。
* Service にも capability を要求・付与し、過剰権限を防ぐ。
* カーネルに policy を押し込まず、サービス側で管理できる構成にする。

1. サービスディレクトリ構造への移行

現在の `src/services/xxx.rs` または `src/services/xxx/` を、以下の形式へ整理してください。

```text
src/services/
  capability.service/
    entry.rs
    manifest.toml
    registry.rs
    policy.rs
    db.rs

  fs.service/
    entry.rs
    manifest.toml

  net.service/
    entry.rs
    manifest.toml

  window.service/
    entry.rs
    manifest.toml

  process.service/
    entry.rs
    manifest.toml
```

各 `*.service/manifest.toml` は最低限この形式にしてください。

```toml
[service]
id = "fs.service"
name = "Filesystem Service"
entry = "entry"
type = "system"
autostart = true
order = 10

[capabilities]
required = [
  "ipc.server",
  "fs.read.all",
  "fs.write.all"
]

optional = []
```

サービスごとの推奨 capability:

```toml
# capability.service
required = [
  "ipc.server",
  "system.info.read"
]

# fs.service
required = [
  "ipc.server",
  "device.storage",
  "fs.read.all",
  "fs.write.all"
]

# net.service
required = [
  "ipc.server",
  "device.net",
  "net.raw"
]

# window.service / compositor
required = [
  "ipc.server",
  "display.read",
  "display.capture",
  "input.pointer.global",
  "input.keyboard.global"
]

# process.service
required = [
  "ipc.server",
  "process.spawn",
  "process.inspect",
  "process.kill"
]
```

`device.net` が未定義なら追加してください。

2. Capability 型を追加

`src/capability/mod.rs` または既存設計に合う場所に、Capability 型を作成してください。

文字列のまま全処理しないでください。外部表現は文字列、内部表現は enum または compact ID にしてください。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
```

次の変換関数を実装してください。

```rust
impl Capability {
    pub fn as_str(&self) -> &'static str;
    pub fn from_str(s: &str) -> Option<Self>;
}
```

3. CapabilitySet を実装

```rust
#[derive(Clone, Debug)]
pub struct CapabilitySet {
    caps: BTreeSet<Capability>,
}
```

必要なAPI:

```rust
impl CapabilitySet {
    pub fn empty() -> Self;
    pub fn insert(&mut self, cap: Capability);
    pub fn contains_exact(&self, cap: Capability) -> bool;
    pub fn contains(&self, cap: Capability) -> bool;
    pub fn implies(&self, cap: Capability) -> bool;
    pub fn from_strings(list: &[String]) -> Result<Self, CapabilityParseError>;
}
```

`contains()` は階層継承を考慮してください。

```text
fs.read.all -> fs.read.user -> fs.read.user.documents
fs.write.all -> fs.write.user -> fs.write.user.documents
```

推奨:

* write は read を含めない。
* read/write は明示的に分ける。
* `open(ReadWrite)` は read と write の両方を要求する。

```rust
pub fn capability_implies(parent: Capability, child: Capability) -> bool {
    use Capability::*;

    if parent == child {
        return true;
    }

    match parent {
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
```

4. カーネル拡張は最低限にする

カーネル側の拡張は最低限にしてください。

capability の主要な管理・ポリシー判定・manifest 解析・許可DB管理は `capability.service` 側に寄せてください。

カーネルは原則として以下だけを担当してください。

```text
- プロセスに紐づいた CapabilitySet を保持する
- syscall / IPC / service 呼び出し時に caller pid を正しく渡す
- 必要なら最低限の fast-path check を行う
- User/Service プロセスが自分の capability を変更できないようにする
```

カーネルに以下を大量に入れないでください。

```text
- TOML manifest parser
- ユーザー許可UI
- capability の危険度分類
- アプリごとの許可DB
- サービス別の細かい policy
- capability.service 相当のロジック
```

カーネル内に追加してよいものは、原則として以下に限定してください。

```rust
// プロセスに紐づく capability の集合
pub struct CapabilitySet { ... }

// プロセスが capability を持つか確認する最小API
pub fn process_has_capability(pid: Pid, cap: Capability) -> bool;

// プロセス生成時に capability を設定する内部API
pub fn set_process_capabilities(pid: Pid, caps: CapabilitySet) -> Result<(), KernelError>;
```

`set_process_capabilities` はカーネル内部用にしてください。一般 User プロセスから呼べる syscall にしてはいけません。

capability の階層関係や危険度分類は、基本的には `capability.service` 側に置いてください。

ただし、各サービスが高速に確認できるように、カーネル側には確定済みの `CapabilitySet` だけを保持して構いません。

5. コメント規約

コメントはすべて日本語で書いてください。

特に security-critical な箇所には、なぜその検査が必要なのかを日本語コメントで明記してください。

例:

```rust
// ユーザープロセスが自分で capability を増やせると sandbox を回避できるため、
// capability の変更は信頼済みの起動経路からのみ許可する。
if !is_trusted_capability_grant_caller(caller_pid) {
    return Err(CapabilityError::CallerNotTrusted(caller_pid));
}
```

既存コードに英語コメントがある場合、新規追加・変更するコメントは日本語に統一してください。

ただし、以下は英語のままで構いません。

```text
- capability 名
- manifest のキー名
- ログの固定タグ
- ABI/API上の識別子
- 既存の外部仕様と一致させる必要がある文字列
```

6. capability.service の役割

`capability.service` は以下を担当してください。

```text
- capability registry の保持
- capability 名と ID の変換
- manifest の required/optional の解釈
- アプリ/サービスごとの許可DB
- 起動時の capability grant
- IPC 経由の check 要求
- dangerous capability の分類
- サービス・アプリの権限昇格拒否
```

API は最低限これを用意してください。

```rust
pub enum CapabilityRequest {
    Resolve {
        name: String,
    },
    Check {
        pid: Pid,
        capability: String,
    },
    GrantForExec {
        subject_id: String,
        subject_type: SubjectType,
        requested: Vec<String>,
        caller_pid: Pid,
    },
    ListGranted {
        pid: Pid,
    },
}
```

```rust
pub enum SubjectType {
    App,
    Service,
}
```

`GrantForExec` は誰でも呼べないようにしてください。

呼べるのは以下のような信頼済みプロセスだけに限定してください。

```text
- init
- core.service
- process.service
```

7. 起動時の流れ

サービス起動:

```text
init/core.service
  ↓
src/services/*.service/manifest.toml を読む
  ↓
capability.service に required capability を問い合わせる
  ↓
許可された CapabilitySet を process 作成時に渡す
  ↓
service entry を起動
```

アプリ起動:

```text
process.service
  ↓
/Applications/xxx.app/manifest.toml を読む
  ↓
capability.service に requested capability を問い合わせる
  ↓
許可済み CapabilitySet を process に付与
  ↓
exec
```

8. Process 構造体に CapabilitySet を追加

既存の process/task 構造体に以下を追加してください。

```rust
pub struct Process {
    pub pid: Pid,
    pub app_id: Option<String>,
    pub service_id: Option<String>,
    pub privilege: PrivilegeLevel,
    pub capabilities: CapabilitySet,
}
```

User/Service プロセスから `capabilities` を変更するAPIは作らないでください。

カーネル内部専用で以下を用意してください。

```rust
pub fn set_process_capabilities(pid: Pid, caps: CapabilitySet) -> Result<(), KernelError>;
pub fn process_has_capability(pid: Pid, cap: Capability) -> bool;
```

9. 各サービス側で強制する

`capability.service` の check だけに依存せず、各サービスは操作ごとに確認してください。

`fs.service`:

```rust
open(path, mode):
  required_caps = capability_for_path(path, mode)
  for cap in required_caps:
      if !kernel::process_has_capability(caller_pid, cap):
          return PermissionDenied
  open実行
```

```rust
fn capability_for_path(path: &str, mode: OpenMode) -> Vec<Capability> {
    let mut caps = Vec::new();

    if mode.can_read() {
        caps.push(read_capability_for_path(path));
    }

    if mode.can_write() {
        caps.push(write_capability_for_path(path));
    }

    caps
}
```

パス対応:

```text
/home/<user>/Documents -> fs.read/write.user.documents
/home/<user>/Downloads -> fs.read/write.user.downloads
/home/<user>/Desktop   -> fs.read/write.user.desktop
/home/<user>/Pictures  -> fs.read/write.user.pictures
/home/<user>/Music     -> fs.read/write.user.music
/home/<user>/Videos    -> fs.read/write.user.videos
/home/<user>           -> fs.read/write.user
/tmp                   -> fs.read/write.tmp
/mount/removable       -> fs.read/write.removable
その他                 -> fs.read/write.all
```

`net.service`:

```text
connect()       -> net.connect
listen()        -> net.listen
raw_socket()    -> net.raw
```

`ipc.service`:

```text
connect service -> ipc.client
publish service -> ipc.server
```

`process.service`:

```text
spawn -> process.spawn
inspect other -> process.inspect
kill other -> process.kill
```

`window.service` / compositor:

```text
create window    -> window.create
overlay          -> window.overlay
capture          -> window.capture
screen capture   -> display.capture
global key       -> input.keyboard.global
global pointer   -> input.pointer.global
```

`clipboard.service`:

```text
read  -> clipboard.read
write -> clipboard.write
```

`device.service`:

```text
gpu direct     -> device.gpu
audio direct   -> device.audio
input direct   -> device.input
storage direct -> device.storage
network direct -> device.net
usb            -> usb.access
serial         -> serial.access
bluetooth      -> bluetooth.access
```

`system.service`:

```text
shutdown  -> power.shutdown
reboot    -> power.reboot
suspend   -> power.suspend
set time  -> system.time.set
read logs -> system.logs.read
```

10. dangerous capability 分類

`capability.service` に危険度分類を入れてください。

```rust
pub enum CapabilityLevel {
    Normal,
    Sensitive,
    Privileged,
    Dangerous,
}
```

Normal:

```text
fs.read.user.documents
fs.write.user.documents
window.create
audio.playback
notification.send
system.time.read
system.info.read
account.self.read
```

Sensitive:

```text
clipboard.read
audio.record
camera.access
microphone.access
location.access
display.capture
window.capture
input.keyboard.global
input.pointer.global
fs.read.user
fs.write.user
net.listen
```

Privileged:

```text
fs.read.all
fs.write.all
net.raw
process.kill
service.register
service.control
package.install
package.remove
package.update
device.storage
device.gpu
device.input
device.audio
system.time.set
system.logs.read
```

Dangerous:

```text
kernel.module.load
kernel.debug
unsandboxed
developer.debug
developer.tracing
```

Policy:

```text
- Normal は manifest にあれば許可してよい。
- Sensitive はユーザー許可が必要。
- UI が未実装なら Sensitive は deny-by-default にする。
- 開発ビルドのみ、明示的な設定がある場合に限って Sensitive を仮許可してよい。
- Privileged は署名済み service / system app のみ許可。
- Dangerous は debug build + developer mode + system signature のみ許可。
```

11. サービスへの capability

サービスも manifest で capability を要求します。

```toml
[service]
id = "net.service"
name = "Network Service"
entry = "entry"
type = "system"
autostart = true
order = 20

[capabilities]
required = [
  "ipc.server",
  "device.net",
  "net.raw"
]
```

サービスだから無条件に全権限、にはしないでください。

ただし以下は bootstrap 例外として扱ってよいです。

```text
core.service
capability.service
initfs/rootfs mount service
```

bootstrap 例外もコード上で明示してください。

```rust
fn is_bootstrap_trusted_service(id: &str) -> bool {
    matches!(
        id,
        "core.service"
            | "capability.service"
            | "fs.service"
    )
}
```

12. manifest parser

アプリ:

```toml
[app]
id = "dev.taso.editor"
name = "Text Editor"
entry = "/bin/editor"

[capabilities]
required = [
  "fs.read.user.documents",
  "fs.write.user.documents",
  "window.create"
]

optional = [
  "clipboard.read",
  "clipboard.write"
]
```

サービス:

```toml
[service]
id = "fs.service"
name = "Filesystem Service"
entry = "entry"
autostart = true
order = 10

[capabilities]
required = [
  "ipc.server",
  "device.storage",
  "fs.read.all",
  "fs.write.all"
]
```

共通構造体:

```rust
pub struct ManifestCapabilities {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}
```

TOML parser がカーネル内で重い場合、manifest の解析は `core.service` または `capability.service` 側で行い、カーネルには確定済みの `CapabilitySet` だけ渡してください。

13. エラー型

以下を用意してください。

```rust
pub enum CapabilityError {
    UnknownCapability(String),
    PermissionDenied {
        subject: String,
        capability: Capability,
    },
    PrivilegedCapabilityDenied {
        subject: String,
        capability: Capability,
    },
    DangerousCapabilityDenied {
        subject: String,
        capability: Capability,
    },
    InvalidManifest(String),
    CallerNotTrusted(Pid),
}
```

14. ログ・監査

権限拒否は audit log に残してください。

```text
[AUDIT] pid=42 app=dev.taso.editor denied capability=fs.write.all op=open path=/system/config
```

必要な情報:

```text
pid
app_id or service_id
operation
required capability
result
target path/socket/service/device
```

15. セキュリティ上の必須条件

必ず満たしてください。

```text
- User プロセスは自分の CapabilitySet を変更できない。
- capability.service の許可DBを書き換えるには service.control または専用 privileged 権限が必要。
- unsandboxed はすべてを許可するが、通常アプリには絶対に出さない。
- kernel.module.load は User アプリに出さない。
- fs.write.all は通常アプリに出さない。
- display.capture, window.capture, input.keyboard.global, clipboard.read, microphone.access, camera.access は sensitive として扱う。
- 各サービス側の enforcement を省略しない。
- manifest に書いてある capability をそのまま信用しない。
- unknown capability は deny。
- path traversal や symlink による fs capability 回避を防ぐため、fs.service 側では正規化後の絶対パスで判定する。
- capability grant は init/core.service/process.service などの信頼済み経路だけに限定する。
```

16. 最低限のテスト

以下のテストを追加してください。

```text
- fs.read.user.documents を持つアプリが /home/user/Documents/a.txt を読める。
- 同じアプリが /home/user/Downloads/a.txt を読めない。
- fs.read.user を持つアプリが Documents/Downloads/Desktop を読める。
- fs.read.user.documents だけでは /home/user/Documents/a.txt に書けない。
- fs.write.user.documents があっても read を持っていなければ read open は失敗する。
- fs.read.all は /system/config を読める。
- 通常アプリが kernel.module.load を要求しても拒否される。
- service manifest に required capability が不足している場合、その操作が拒否される。
- unknown capability を含む manifest は起動拒否。
- unsandboxed は trusted service 以外拒否。
```

17. 実装順序

次の順番で実装してください。

```text
1. Capability enum と from_str/as_str
2. CapabilitySet
3. capability_implies
4. manifest の capabilities parser
5. Process への CapabilitySet 追加
6. capability.service ディレクトリ追加
7. 既存 service を XXX.service/entry.rs 形式へ移行
8. service manifest 追加
9. init/core.service の service manifest 読み込み処理
10. fs.service の enforcement
11. net/window/process/clipboard/device の enforcement
12. audit log
13. テスト追加
```

18. 既存コードへの配慮

```text
- 既存API名を壊さないように、可能なら wrapper を残してください。
- 一気に全サービスを完全実装できない場合でも、少なくとも fs.service と process/service 起動経路は実動作する状態にしてください。
- todo!() や unwrap() は避け、起動不能になる場所では明確なエラーを返してください。
- no_std 環境なら alloc の Vec, String, BTreeSet 可否に合わせて調整してください。
- TOML parser がカーネル内で重い場合、manifest の解析は core.service 側で行って、kernel には CapabilitySet だけ渡してください。
- コメントは日本語で書いてください。
- security-critical な処理には、なぜ必要なのかを日本語コメントで説明してください。
```

最終的な期待結果:

```text
- アプリとサービスが manifest で capability を宣言できる。
- capability.service が許可・拒否を判定できる。
- プロセスに CapabilitySet が紐づく。
- 各サービスが caller pid の CapabilitySet を見てアクセスを拒否できる。
- src/services/XXX.service/entry.rs 形式でサービスが整理される。
- サービス自体にも capability が適用される。
- カーネル側の拡張は最小限に抑えられている。
- 危険 capability は通常アプリから拒否される。
- 新規追加・変更コメントは日本語で書かれている。
```
