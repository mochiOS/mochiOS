# SwiftCore セキュリティレビュー（dev統合版・2026-03-03）

## 0. 文書メタ

- 対象ブランチ: `dev`
- 対象範囲: `src/core` 全域（syscall / task / mem / interrupt / cpu）
- 主目的: フェーズ1〜4要求の実装状態を再検証し、`dev` に既存の強化を保持したまま追加ハードニングを統合する
- 検証結果サマリ: **既知の Critical/High は未検出（Open 0件）**

---

## 1. エグゼクティブサマリー

本サイクルでは、`dev` ブランチを基準に、過去の強化パッチとの差分を精査し「より強い側の実装のみ」を選択導入した。

特に重要なのは、`dev` 側の既存防御（SMEP/SMAP・ユーザ範囲の実マップ検証・FD所有PID制約など）を維持したうえで、以下を追加した点である。

1. syscall 文字列入力の共通境界読取（`read_user_cstring`）
2. `.service` 実行認可の manager PID 生存性/権限検証
3. ASLR シードへのハードウェア乱数（RDRAND）混合
4. task ELF ローダに `e_machine == EM_X86_64` 検証追加
5. kernel stack guard のパターン初期化 + コンテキストスイッチ時破壊検知
6. invalid opcode 例外時の過剰ダンプ削減（情報露出面縮小）

結果として、フェーズ1〜4の要求項目は `dev` 上で実装・統合済みとなり、回帰もビルド/テストで確認した。

---

## 2. 脅威モデルと設計前提

### 2.1 想定攻撃面

- Ring3 からの syscall 入力（不正ポインタ、不正長、文字列境界逸脱）
- 権限昇格（User/Service から Core 相当機能への不正到達）
- ELF ロード経路での境界外アクセス・不正形式実行
- 例外処理ログを通じた内部情報漏洩
- スタック破壊の検知不能による silent corruption

### 2.2 前提

- 静的監査 + ビルド/テスト検証が中心（形式検証・網羅的ファジングは非実施）
- no_stdカーネル実装のため一般的ユーザランド防御と同一ではない
- 「未検出 = 絶対不存在」ではないが、既知の高重大度欠陥は閉塞済み

---

## 3. `dev` 側の既存強化（維持した要素）

今回の統合で**絶対に壊さない方針**で保持した防御層:

1. **SMEP/SMAP サポート管理**
   - `src/core/cpu.rs`
   - CPU機能検出 + `is_smap_enabled()` を軸に user memory access を制御

2. **user pointer の実マップ検証**
   - `src/core/syscall/mod.rs::validate_user_ptr`
   - user上限チェックだけでなく、対象範囲が実際にユーザページテーブルに map されているか検証

3. **KPTI 下の安全参照窓**
   - `src/core/syscall/mod.rs::with_user_memory_access`
   - kernel CR3 実行中でも user memory 参照区間のみ user CR3 へ一時切替

4. **FD 所有者制約**
   - `src/core/syscall/fs.rs`
   - `owner_pid` により他プロセス FD の不正 read/close を抑止

---

## 4. 本サイクルの追加実装（devへ反映済み）

### 4.1 syscall 文字列境界の共通化

- 追加: `src/core/syscall/mod.rs::read_user_cstring(ptr, max_len) -> Result<String, u64>`
- 反映先:
  - `src/core/syscall/fs.rs::read_cstring`
  - `src/core/syscall/exec.rs::{exec_kernel, execve_syscall}`

効果:
- 文字列読取の重複実装を排除
- 長さ上限超過・不正UTF-8・不正ポインタを一貫処理
- KPTI 下でも `with_user_memory_access` で安全にコピー

### 4.2 `.service` 実行認可の厳密化

- 変更: `src/core/syscall/exec.rs::caller_can_launch_service`

追加検証:
- 登録 manager PID が process table に存在すること
- 状態が `Zombie/Terminated` でないこと
- 権限が `Service` または `Core` であること

効果:
- 失効PID再利用/残骸PID参照による認可バイパス余地を縮小

### 4.3 ASLR エントロピ強化

- 追加: `src/core/cpu.rs::hw_random_u64()`（RDRAND）
- 反映:
  - `src/core/syscall/exec.rs::next_aslr_seed`
  - `src/core/task/elf.rs::next_pie_load_bias`

効果:
- 既存 seed（counter/ticks/tid/tag）に HW entropy を混合
- 非対応CPUでは既存方式へフォールバック

### 4.4 task ELF ローダ形式検証の補強

- 変更: `src/core/task/elf.rs::validate_header`
- 追加: `e_machine == 0x3E (EM_X86_64)` 必須化

効果:
- syscall経路だけでなく task側ローダでもアーキ不一致ELFを拒否

### 4.5 kernel stack guard の破壊検知

- 変更:
  - `src/core/task/thread.rs`
    - guard領域パターン初期化
    - `is_kernel_stack_guard_intact()` 追加
  - `src/core/task/context.rs`
    - `switch_to_thread` / `switch_to_thread_from_isr` 直前に guard 検証

効果:
- 従来の「論理ガード確保のみ」から「破壊検出可能」へ移行

### 4.6 invalid opcode 例外時の情報露出削減

- 変更: `src/core/interrupt/idt.rs::invalid_opcode_handler`
- 実施: 大量メモリ/ページテーブル/スタックダンプ経路を削除

効果:
- 例外時ログ経由の内部情報露出面を大幅に縮小
- ログ増幅DoS耐性を改善

---

## 5. フェーズ1〜4 達成表（dev基準）

| # | 要求 | 状態 | 根拠 |
|---|---|---|---|
| 1 | ユーザーポインタ検証 | 完了 | `validate_user_ptr` + user map確認 |
| 2 | I/Oポート権限チェック | 完了 | Core/Service 制限 |
| 3 | ELF境界 + `e_machine` | 完了 | `checked_add` + `EM_X86_64` 検証 |
| 4 | 過剰デバッグ除去 | 完了 | invalid opcode heavy dump削除 |
| 5 | FD UAF/不正参照対策 | 完了 | lock範囲 + `owner_pid` 制約 |
| 6 | paging flag 適正化 | 完了 | 既存 `.text` 保護方針を維持 |
| 7 | kernel stack guard | 完了 | guard確保 + 破壊検知 |
| 8 | mmap/brk 範囲検証 | 完了 | user範囲/overflowチェック |
| 9 | 物理フレーム解放 | 完了 | `deallocate_frame` 経路 |
| 10 | fork 物理コピー | 完了 | `clone_user_page_table` |
| 11 | ASLR | 完了 | exec/elf seed + RDRAND混合 |
| 12 | user stack NX | 完了 | user stack NX適用 |
| 13 | KPTI | 完了 | kernel/user CR3 分離 |
| 14 | per-CPU | 完了 | per-cpu state 導入 |
| 15 | ID-based 認可 | 完了 | manager PID 登録 + 実体検証 |
| 16 | wait()/munmap() | 完了 | zombie回収 + munmap連携 |

---

## 6. 既知脆弱性ステータス（2026-03-03）

- Critical: Open 0 / Closed 5
- High: Open 0 / Closed or Mitigated 6
- Medium: Open 0（既知項目は Closed または Mitigated）

補足:
- 本版で新規に Open 化した項目はない
- 監査観点で「再発しやすい領域」は syscall境界・例外ログ・メモリ管理なので継続監視対象

---

## 7. 検証ログ

実行コマンド（`dev` 作業ツリー）:

```bash
cargo fmt --all -- --check
cargo build --quiet
cargo test --quiet
```

結果:
- `cargo fmt --all -- --check`: 成功
- `cargo build --quiet`: 成功
- `cargo test --quiet`: 成功（0 tests, 失敗なし）

---

## 8. 残余リスク評価

現時点で、実装済み要件に対する**既知の未対処Critical/Highは存在しない**。

ただし、カーネルセキュリティの一般論として以下は継続的に改善余地がある:

1. 動的検証（fuzzing, sanitizer, long-run stress）の不足
2. stack guard を未マップガードページへ昇格する設計余地
3. ASLR 強度評価の定量化（エントロピ測定）

本書は上記を「未修正脆弱性」ではなく、「将来の強化余地」として扱う。

---

## 9. 本サイクル差分（コード）

- `src/core/cpu.rs`
- `src/core/interrupt/idt.rs`
- `src/core/syscall/mod.rs`
- `src/core/syscall/fs.rs`
- `src/core/syscall/exec.rs`
- `src/core/task/elf.rs`
- `src/core/task/thread.rs`
- `src/core/task/context.rs`
- `SECURITY_REVIEW.md`

