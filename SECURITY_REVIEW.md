# SwiftCore セキュリティアーキテクチャ／セキュリティモデル仕様（コード導出版）

対象ブランチ: `dev`  
分析対象: `src/core/**`（`cpu`, `mem`, `syscall`, `task`, `interrupt`, `percpu`, `init`）  
文書目的: 「現時点の実装が、どの安全性不変条件を、どの機構で、どの範囲まで満たしているか」をコードから厳密に記述する。

---

## 0. 分析方法と前提

### 0.1 方法

本書は実装コードから導出した**記述的仕様**であり、理想設計ではなく「実装されている実際の振る舞い」を対象とする。  
主な参照先:

- syscall 境界: `src/core/syscall/mod.rs`, `src/core/syscall/syscall_entry.rs`
- メモリ分離/KPTI: `src/core/mem/paging.rs`, `src/core/interrupt/timer.rs`
- 権限モデル: `src/core/task/ids.rs`, `src/core/syscall/io_port.rs`, `src/core/syscall/exec.rs`
- 例外/割込み: `src/core/interrupt/idt.rs`
- プロセス/スレッド: `src/core/task/process.rs`, `src/core/task/thread.rs`, `src/core/task/context.rs`, `src/core/task/scheduler.rs`

### 0.2 本書の性質

- 形式検証・完全網羅証明ではない（静的コード監査ベース）。
- 「未記載の脆弱性が存在しない」ことを保証しない。
- ただし、カーネル境界条件（ポインタ検証、CR3切替、権限判定、ELF検証、FD所有制約）については、実装上の成立条件を明示する。

---

## 1. システム／信頼境界モデル

### 1.1 実行ドメイン

| ドメイン | CPU ring | 実装上の権限概念 | 代表 |
|---|---:|---|---|
| Kernel Core | Ring0 | `PrivilegeLevel::Core` | スケジューラ、メモリ管理、割込み処理 |
| Service | Ring3 | `PrivilegeLevel::Service` | `.service` 実行体 |
| User | Ring3 | `PrivilegeLevel::User` | 一般プロセス |

`Service` と `User` はともに Ring3 だが、syscall レイヤで論理権限が分岐される（例: I/O port）。

### 1.2 保護対象アセット

1. カーネル制御フロー（RIP/RSP/CR3 の整合）
2. カーネル専用メモリ（supervisor mapping）
3. プロセス間資源分離（FD、IPC宛先、ページテーブル）
4. 実行イメージの完全性（ELF妥当性、セグメント境界）

### 1.3 攻撃者モデル

- 攻撃者は Ring3 から任意 syscall を実行できる。
- syscall 引数（整数、ポインタ、長さ、フラグ）は任意に細工可能。
- 悪性 ELF / 異常 long string / race を含む入力を与える。
- ハードウェア攻撃（物理改ざん）は対象外。

---

## 2. セキュリティ不変条件（Security Invariants）

以下を「成立すべき不変条件」と定義し、現実装での担保箇所を対応付ける。

### INV-1: ユーザーポインタ安全性

**定義**  
ユーザー由来ポインタ `p,len` は、カーネル dereference 前に:

1. canonical user range (`<= 0x0000_7FFF_FFFF_FFFF`)
2. overflow なし
3. 現在プロセスのユーザーページテーブル上で全ページ mapped

を満たすこと。

**実装根拠**  
`syscall::validate_user_ptr` + `paging::is_user_range_mapped_in_table`。  
実際のアクセスは `syscall::with_user_memory_access` 内で実施。

### INV-2: SMAP下のユーザーメモリアクセス制御

**定義**  
SMAP 有効時、カーネルがユーザーメモリへアクセスする区間は明示的に許可されること。

**実装根拠**  
`with_user_memory_access` が `stac/clac` と CR3 切替を管理し、区間外アクセスを抑制。

### INV-3: KPTI境界でのCR3整合

**定義**  
syscall/割込み処理中のカーネル実行は kernel CR3 で行われ、復帰時に user CR3 へ戻すこと。

**実装根拠**  
- syscall: `syscall_handler_rust` で `switch_to_kernel_page_table` / `restore_page_table`
- timer IRQ: `timer_interrupt_handler` 入口で kernel CR3、出口で `switch_to_current_thread_user_page_table`

### INV-4: 特権 syscall の論理権限制約

**定義**  
機密性の高い機能（I/O port、service起動）は論理権限で拒否されること。

**実装根拠**  
- I/O port: `syscall/io_port.rs` で `Core|Service` のみ許可
- `.service` 実行: `syscall/exec.rs::caller_can_launch_service`

### INV-5: 実行可能性ポリシー

**定義**  
不要な writable+executable を避け、実行権を最小化すること。

**実装根拠**  
- EFER.NXE 有効化: `cpu::enable_nxe`
- ユーザースタック NX: `mem/user.rs::alloc_user_stack`
- ELFセグメント NX: `paging::map_and_copy_segment_to`
- カーネル `.text` を read-only に戻す: `paging::protect_kernel_text_pages`

### INV-6: プロセス間 FD 分離

**定義**  
FD 操作は所有 PID のみ許可。

**実装根拠**  
`syscall/fs.rs` の `owner_pid` チェック（`read/close/seek/fstat`）。

### INV-7: fork 後のアドレス空間分離

**定義**  
子プロセスの user ページは親と物理分離されること。

**実装根拠**  
`paging::clone_user_page_table` は USER_ACCESSIBLE な 4KiB ページを新規フレームへコピー。

### INV-8: 例外封じ込め

**定義**  
ユーザーモード例外は当該プロセス終了、カーネルモード重大例外は停止へ遷移。

**実装根拠**  
`interrupt/idt.rs` の各 exception handler。

---

## 3. 制御フロー境界モデル

### 3.1 ブート時初期化の安全属性

`init::kinit` は概ね以下順で初期化:

1. CPU機能有効化（NXE/SMEP/SMAP/FSGSBASE）
2. フレームアロケータ
3. ページング初期化（新規 L4、`.text` 保護）
4. PIT・スケジューラ・割込み
5. SYSCALL MSR 設定

これにより「NX/SMEP/SMAPが有効化された状態で syscall/割込み処理へ進む」前提を作る。

### 3.2 syscall 入口

SwiftCore は **2系統**を保持:

1. `int 0x80` (`syscall_interrupt_handler`)
2. `SYSCALL/SYSRET` (`syscall_entry`)

両方とも最終的に `syscall_handler_rust -> dispatch` へ収束し、dispatch 前に kernel CR3 へ切替される。

### 3.3 CVE-2012-0217 緩和

`syscall_entry` では SYSRET 直前に user RSP の canonical 検査を行い、非正規アドレス検出時はプロセス終了へフォールバックする。

---

## 4. メモリ保護モデル

### 4.1 user range 検証アルゴリズム

`paging::is_user_range_mapped_in_table` は:

- `addr+len-1` の overflow を拒否
- 上限超過を拒否
- 範囲内全ページを走査して USER_ACCESSIBLE + PRESENT を要求

し、「上限比較だけ」の検証に比べて強い条件を課す。

### 4.2 KPTI の実装境界

`create_user_page_table` は kernel L4 の高位領域を広く共有せず、低位側の最小コピー方針を取る。  
一方で syscall 実行に必要な低位マッピングは残すため、完全非共有型 KPTI ではなく「最小共有型」に位置づく。

### 4.3 スタック保護

- ユーザースタック: 1ページ guard + NX (`mem/user.rs`)
- カーネルスタック: guard pattern (`0xA5`) による破壊検知 (`thread.rs`, `context.rs`)

ユーザースタックは未マップ guard、カーネルスタックは**検知型**であり、未マップ page fault 型ではない。

### 4.4 フレーム解放

- `munmap` は `unmap_range_in_table` を通じて unmap + frame 解放
- `destroy_user_page_table` は user hierarchy を辿り frame を解放

---

## 5. 実行イメージ（ELF/exec）セキュリティモデル

### 5.1 ELF 妥当性チェック

`task/elf.rs` と `elf/loader.rs` の両系統で、少なくとも以下を確認:

- ELF magic
- 64bit little-endian 条件
- `e_machine == EM_X86_64 (0x3E)`
- `p_offset + p_filesz` checked_add
- `p_vaddr` / `p_memsz` 境界の overflow 防止

### 5.2 セグメントマッピング方針

ロード時は書き込み可能でコピーし、最終フラグで `WRITABLE` / `NO_EXECUTE` を調整する。  
非 executable セグメントには NX を付与。

### 5.3 `.service` 起動認可

`exec` 経路では `.service` 実行時に:

- Core 呼び出しは許可
- それ以外は service manager PID 一致を要求
- manager PID の存在、状態（Zombie/Terminated除外）、権限（Core/Service）を検証

---

## 6. 資源分離モデル（FS / IPC / Process）

### 6.1 FD テーブル

`FD_TABLE` は `FileHandle { owner_pid, data, pos }` を保持し、操作ごとに `owner_pid == caller_pid` を要求。  
`read` は lock 保持中に handle を使用し、close との UAF 窓を抑制。

### 6.2 IPC

Mailbox は thread slot 単位。宛先 thread id 検証を行い、受信時に `msg.to == receiver` を再確認して誤配送を防ぐ。

### 6.3 wait/reap

`wait` は parent-child 関係を前提に zombie を回収し、`WNOHANG` を実装。  
フェイルセーフとして 30秒 timeout が入る（POSIX完全互換ではない）。

---

## 7. 現状評価（2026-03-03, コード導出）

### 7.1 実装済み主要緩和

- user pointer map-aware 検証
- SMAP/STAC/CLAC 統制
- KPTI CR3 切替（syscall + timer IRQ）
- SMEP/SMAP/NXE 有効化
- I/O port 論理権限ゲート
- service 実行認可（PID生存性含む）
- ELF 境界チェック強化
- user stack NX + guard
- FD owner 分離

### 7.2 残余リスク / 既知ギャップ

| ID | 重大度 | 内容 | 根拠 |
|---|---|---|---|
| R-01 | High (SMP時) / Low (現単一CPU想定) | `SYSCALL_KERNEL_RSP` がグローバルで、per-CPU state が syscall ASM で未活用。SMP有効時に cross-core stack 汚染リスク。 | `syscall/syscall_entry.rs`, `percpu.rs` |
| R-02 | High (可用性) | zombie 回収時に page table / user frame 解放が自動連動せず、長時間運用でメモリリークにより OOM 化しうる。 | `task/process.rs::reap_zombie_child_process` と `paging::destroy_user_page_table` の非接続 |
| R-03 | Medium | `RtSigaction`/`RtSigprocmask` が SUCCESS スタブ。アプリは「設定できた」と誤認し得る。 | `syscall/mod.rs::dispatch` |
| R-04 | Medium | カーネルスタック guard は未マップ型ではなくパターン検知型。検知は可能だが事前遮断ではない。 | `task/thread.rs`, `task/context.rs` |
| R-05 | Low | IPC送信先存在確認と enqueue が原子的でなく、孤立メッセージが残る可能性。 | `syscall/ipc.rs` 内コメント |
| R-06 | Low | `wait` が 30秒 timeout を持つため POSIX の無期限待機と差異。 | `syscall/process.rs` |

---

## 8. 優先改善提案（実装順）

### P0（先行推奨）

1. `wait/reap` で child の page table 破棄 (`destroy_user_page_table`) を接続  
2. SYSCALL stack pointer を GS/per-CPU 経由へ完全移行（グローバル `SYSCALL_KERNEL_RSP` 脱却）  
3. signal syscall を `ENOSYS` 明示または最小実装へ変更（偽SUCCESSを避ける）

### P1

4. カーネルスタックを未マップ guard page 化（検知型 -> fault-before-corruption）  
5. IPC 宛先検証と enqueue の原子化（スロット世代番号など）

### P2

6. KPTI の共有範囲さらに最小化  
7. 追加動的検証（fuzz, stress, long-run leak test）

---

## 9. 検証プロトコル（再現用）

最低限の再検証コマンド:

```bash
cargo fmt --all -- --check
cargo build --locked --verbose
cargo test --locked --verbose
```

推奨追加（運用前）:

- 長時間 `fork/exit/wait` ループでフレーム使用量推移確認
- syscall fuzz（不正 pointer/length/flags）で EFAULT/EINVAL fail-closed 確認
- `.service` 認可の PID 生存性回帰テスト

---

## 10. 証拠マップ（主要不変条件 -> 実装）

| 不変条件 | 主実装 |
|---|---|
| INV-1 | `syscall/mod.rs::validate_user_ptr`, `mem/paging.rs::is_user_range_mapped_in_table` |
| INV-2 | `syscall/mod.rs::with_user_memory_access`, `cpu.rs::is_smap_enabled` |
| INV-3 | `syscall/mod.rs::syscall_handler_rust`, `syscall/syscall_entry.rs`, `interrupt/timer.rs` |
| INV-4 | `syscall/io_port.rs`, `syscall/exec.rs::caller_can_launch_service` |
| INV-5 | `cpu.rs::enable_nxe`, `mem/user.rs`, `mem/paging.rs::protect_kernel_text_pages` |
| INV-6 | `syscall/fs.rs` |
| INV-7 | `mem/paging.rs::clone_user_page_table`, `syscall/process.rs::fork` |
| INV-8 | `interrupt/idt.rs` |

---

この文書は「実装済み対策の誇張」ではなく「成立している境界と未成立の境界」を明示することを目的とする。  
従って、R-01/R-02 のような未閉塞項目は、現行リリース判断における主要監査ポイントとして扱うべきである。
