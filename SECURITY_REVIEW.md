# SwiftCore セキュリティアーキテクチャ／セキュリティモデル仕様（コード導出）

- 対象ブランチ: `dev`
- 分析対象: `src/core/**` と関連する `src/user/**`
- 最終更新: 2026-03-04
- 文書目的: 実装済みコードから、**成立している安全性境界・不変条件・残余ギャップ**を厳密に記述する

---

## 0. エグゼクティブサマリ

現行実装は、カーネル境界防御として以下を成立させている。

1. **syscall 境界の fail-closed 化**  
   `validate_user_ptr`（map-aware）＋`with_user_memory_access`（CR3/SMAP 制御）＋`copy_from_user` への収束。
2. **KPTI/per-CPU 基盤の常用化**  
   syscall/割込みで kernel CR3 に統一し、必要区間のみ user CR3 へ一時切替。
3. **制御フロー保護**  
   SYSRET 前に user RIP/RCX と user RSP の canonicality を検証し、異常時はプロセス終了へフォールバック。
4. **資源分離の強化**  
   FD owner 制約、IPC 世代番号検証、wait/reap と page table 破棄の連動を実装。

以前の残余リスク項目（R-01〜R-06）は実装上すべて閉塞済み。  
現時点の主要論点は、脆弱性というより **運用品質（長時間回帰・互換拡張）** に移っている。

---

## 1. 分析方法と前提

### 1.1 方法

- 本書は理想設計書ではなく、**実装コードから導出した記述的仕様**。
- 主要参照モジュール:
  - syscall 境界: `src/core/syscall/mod.rs`, `src/core/syscall/syscall_entry.rs`
  - KPTI/ページング: `src/core/mem/paging.rs`, `src/core/interrupt/timer.rs`
  - 権限モデル: `src/core/syscall/io_port.rs`, `src/core/syscall/exec.rs`, `src/core/task/ids.rs`
  - プロセス/スレッド: `src/core/task/process.rs`, `src/core/task/thread.rs`, `src/core/task/context.rs`
  - 例外封じ込め: `src/core/interrupt/idt.rs`

### 1.2 制約

- 静的監査＋実行検証結果に基づく。形式証明ではない。
- 未記載脆弱性の不存在を保証するものではない。
- ただし、境界条件（ユーザーポインタ・CR3・権限・資源所有）は実装根拠まで追跡済み。

---

## 2. 脅威モデル

### 2.1 攻撃者能力

- Ring3 から任意 syscall を発行可能。
- syscall 引数（ポインタ、長さ、flags、fd、tid）を細工可能。
- 悪性 ELF、長大文字列、race 条件を伴う入力を投入可能。

### 2.2 保護対象

1. カーネル制御フロー（RIP/RSP/CR3）
2. カーネル専用メモリ（supervisor mapping）
3. プロセス間資源（FD / IPC / page table）
4. 実行イメージ整合（ELF header/program header）

### 2.3 想定外

- 物理攻撃、ファームウェア改ざん、サイドチャネル全般の完全防御は対象外。

---

## 3. セキュリティ不変条件（Invariants）

| ID | 不変条件 | 実装担保 | 状態 |
|---|---|---|---|
| INV-1 | ユーザーポインタは canonical 範囲・overflow 無し・ページマップ済みのみ許可 | `validate_user_ptr`, `is_user_range_mapped_in_table` | Satisfied |
| INV-2 | ユーザーメモリアクセスは限定区間でのみ実施 | `with_user_memory_access`（CR3 切替 + STAC/CLAC） | Satisfied |
| INV-3 | syscall/割込みのカーネル処理は kernel CR3 で実行 | `switch_to_kernel_page_table`, timer IRQ 復帰処理 | Satisfied |
| INV-4 | SYSRET 復帰前に user RIP/RSP 正規性を検証 | `syscall_entry.rs` canonicality check + kill fallback | Satisfied |
| INV-5 | 特権 syscall は論理権限で拒否 | `io_port.rs`, `exec.rs::caller_can_launch_service` | Satisfied |
| INV-6 | 実行可能性を最小化（NX/W^X/guard） | `enable_nxe`, stack NX+guard, `.text` 保護 | Satisfied |
| INV-7 | プロセス間資源は所有/宛先整合を要求 | FD owner 制約, IPC generation 検証 | Satisfied |
| INV-8 | `fork` 後に user page は物理分離 | `clone_user_page_table` | Satisfied |
| INV-9 | 例外はユーザー封じ込め（kernel fatal は停止） | `interrupt/idt.rs` 各 handler | Satisfied |
| INV-10 | zombie 回収で page table/frame 破棄が連動 | `reap_zombie_child_process` + `destroy_user_page_table` | Satisfied |

---

## 4. 実装モデル（境界ごとの詳細）

### 4.1 syscall 境界（入力検証）

- `validate_user_ptr` は `ptr + len - 1` の包含終端で判定し、off-by-one を回避。
- `read_user_cstring` はページ境界単位で妥当性を再確認し、最大長を超える入力を拒否。
- バイト列取得は `copy_from_user` に統一し、syscall 実装ごとの unsafe 参照分散を削減。

### 4.2 KPTI と per-CPU 状態

- `percpu.rs` で CPU ローカル状態（`kernel_cr3`, `syscall_kernel_rsp`, `current_thread_id`）を保持。
- SYSCALL エントリ時のカーネルスタック取得は `gs:[offset]` 参照でグローバル依存を除去。
- `syscall_handler_rust` / timer IRQ で kernel CR3 実行を保証し、復帰時に user CR3 を復元。
- user pointer 参照の実アクセスのみ `with_user_memory_access` で user CR3 に一時切替。

### 4.3 SYSRET 復帰経路のハードニング

- SYSRET 直前で user RSP と user RIP（`rcx`）の canonicality を検証。
- 異常値検出時は通常復帰せず、カーネル側 kill 経路へ強制遷移（CVE-2012-0217 系の緩和）。

### 4.4 メモリ実行ポリシー

- NXE を有効化し、実行不可領域を明示。
- ユーザースタックは guard page + NX を採用。
- カーネル `.text` は writable を落とし read-only 化。
- カーネルスタックは未マップ guard を使用し、コンテキスト切替時に guard 破壊を検知。  
  （内部プール外スタックには誤検知回避ロジックを適用）

### 4.5 ELF / exec / 権限モデル

- ELF ロード時に magic/class/endianness/`EM_X86_64`/サイズ計算の安全性を検証。
- `.service` 実行は manager PID と権限状態（Core/Service）を検証して許可。
- `exec` の heap 状態初期化漏れは修正済み（runtime 回帰対策）。

### 4.6 資源分離（FD / IPC / wait-reap）

- FD テーブルは `owner_pid` 一致を必須化。
- IPC は宛先 thread id に加え slot generation 一致を要求し、再利用スロット誤配送を遮断。
- `wait` は `WNOHANG` 互換を維持しつつ、ブロッキング待機は無期限化。
- zombie 回収時に child page table を破棄し、frame 解放を自動連動。

### 4.7 signal syscall の扱い

- `RtSigaction` / `RtSigprocmask` は `SUCCESS` スタブを廃止し、`ENOSYS` を明示。  
  （「成功したように見える失敗」を禁止）

---

## 5. 残余リスク項目（R-01〜R-06）閉塞状況

| ID | 現状態 | 実装内容 | 根拠 |
|---|---|---|---|
| R-01 | Closed | SYSCALL stack pointer を `gs:[offset]` の per-CPU 参照へ移行 | `syscall_entry.rs`, `percpu.rs` |
| R-02 | Closed | zombie 回収で child page table を自動破棄 | `task/process.rs`, `mem/paging.rs` |
| R-03 | Closed | `rt_sig*` を `ENOSYS` 明示に変更 | `syscall/mod.rs` |
| R-04 | Closed | カーネルスタック guard を未マップ化し切替時検証 | `task/thread.rs`, `task/context.rs` |
| R-05 | Closed | IPC generation 検証を導入し誤配送を抑止 | `syscall/ipc.rs`, `task/thread.rs` |
| R-06 | Closed | `wait` の固定 timeout を撤廃し無期限待機化 | `syscall/process.rs` |

---

## 6. 検証結果（直近サイクル）

### 6.1 静的/ビルド検証

```bash
cargo fmt --all -- --check
cargo build --locked --quiet
cargo test --locked --quiet
```

上記は直近検証サイクルで成功。

### 6.2 実行時スモーク

```bash
timeout 110s cargo run
```

- kernel/user page fault の再発は非観測。
- `fs.service` は `InitFS mounted and initialized` 到達を確認。

---

## 7. 既知ギャップ（脆弱性というより互換・運用品質）

1. **KPTI 共有範囲のさらなる縮小余地**  
   `create_user_page_table` は既に最小化方針だが、低位共有マップはなお残る。
2. **`clone(2)` の Linux 完全互換は未達**  
   現状は `fork` 寄り挙動が中心。
3. **TLS の `PT_TLS` ローダは未完**  
   `arch_prctl` はあるが、TLS テンプレート配置は未完。
4. **signal 互換の段階的導入が必要**  
   現状 `rt_sig*` は fail-closed (`ENOSYS`)。

---

## 8. 継続改善提案

### P0

- syscall / IPC / wait-reap の長時間ストレス試験を CI 以外でも定期運用
- `fork/exit/wait` ループでフレーム使用量・リークの継続監視

### P1

- KPTI 共有マッピングの実測ベース縮小
- SMP 構成で GS/per-CPU 経路の soak test を追加

### P2

- signal/TLS/clone 互換を段階拡張し、`std` 実運用の失敗面を縮小

---

## 9. 証跡トレーサビリティ（不変条件 → 実装）

| 不変条件 | 主実装 |
|---|---|
| INV-1 | `syscall/mod.rs::validate_user_ptr`, `mem/paging.rs::is_user_range_mapped_in_table` |
| INV-2 | `syscall/mod.rs::with_user_memory_access` |
| INV-3 | `syscall/syscall_entry.rs`, `syscall/mod.rs::syscall_handler_rust`, `interrupt/timer.rs` |
| INV-4 | `syscall/syscall_entry.rs`（canonicality check + kill fallback） |
| INV-5 | `syscall/io_port.rs`, `syscall/exec.rs` |
| INV-6 | `cpu.rs`, `mem/user.rs`, `mem/paging.rs`, `task/thread.rs`, `task/context.rs` |
| INV-7 | `syscall/fs.rs`, `syscall/ipc.rs`, `task/thread.rs` |
| INV-8 | `mem/paging.rs::clone_user_page_table`, `syscall/process.rs::fork` |
| INV-9 | `interrupt/idt.rs` |
| INV-10 | `task/process.rs::reap_zombie_child_process`, `mem/paging.rs::destroy_user_page_table` |

---

本書は「実装済み対策の宣言」ではなく、**成立している境界の証跡化**を目的とする。  
今後の重点は、重大リスクの新規導入防止を前提に、互換拡張と運用検証の深度を高めることである。
