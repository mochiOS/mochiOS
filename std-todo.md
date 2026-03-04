# Rust `std` 対応 TODO（現行 `dev` 基準）

SwiftCore 上で Rust `std` を安定動作させるための、**現状ステータス**と**残タスク**を整理した一覧。

---

## ✅ ステータス更新（2026-03-04）

| # | 項目 | 状態 | 要点 |
|---|---|---|---|
| 1 | `libc` パス/差し替え | 完了 | `Cargo.toml` の不整合パッチは解消済み |
| 2 | `libc` クレート名整合 | 完了 | `libc` 置換が成立する構成に統一済み |
| 3 | `swiftlib` 依存不整合 | 完了 | 依存関係の破綻を解消 |
| 4 | `#![no_main]` 誤用 | 完了 | ライブラリ側の不適切属性を除去済み |
| 5 | `swiftlib::cfunc` 欠落 | 完了（不要化） | 依存箇所を整理し欠落参照を除去 |
| 6 | allocator 基盤 (`memalign/free/realloc`) | 完了 | `src/user/libc.rs` で newlib 実装へ委譲 |
| 7 | errno 返却規約 | 完了（基盤） | カーネルは負 errno 返却、ユーザー側判定も signed 化 |
| 8 | ターゲットOS設定 | 完了 | Linux 互換方針に整合 |
| 9 | Linux 互換 syscall 番号 | 完了（基盤） | 主要番号は Linux x86_64 準拠 |
| 10 | `mmap` / `munmap` | 完了 | 匿名マップ/アンマップ実装済み |
| 11 | `clone`（Linux意味論） | 未完（部分） | 現状は `fork` 寄り挙動 |
| 12 | `futex` | 完了 | `WAIT/WAKE` 実装あり |
| 13 | TLS (`PT_TLS`) | 未完（部分） | `arch_prctl + FS` はあり、`PT_TLS` ローダ未実装 |
| 14 | `errno` TLS 化 | 未完（部分） | まだ単一グローバル依存の経路がある |
| 15 | `clock_gettime` | 完了 | `MONOTONIC/REALTIME` 相当の実装あり |
| 16 | `nanosleep` 精度 | 未完（部分） | 現状は簡易待機で高精度ではない |
| 17 | signal 最低限互換 | 未完（部分） | `rt_sig*` は `ENOSYS` 返却 |
| 18 | `getcwd` | 完了（簡易） | ルート固定モデルで実装済み |
| 19 | FS syscall 群 | 完了（簡易） | `open/read/close/fstat/lseek` の基本経路あり |

---

## 🔧 今回反映した更新点（2026-03-04）

1. user libc の allocator 経路を修正  
   - `memalign/free/realloc` を newlib 実装へ委譲し、Rust 側 allocator と整合。
2. user 側 syscall ラッパの errno 判定を修正  
   - `u64::MAX` 固定判定ではなく `ret as i64 < 0` に統一。
3. 実行時の回帰を解消  
   - `fs.service` 起動時のクラッシュ経路を修正し、`InitFS mounted and initialized` 到達を確認。

---

## ⚠️ 残タスク（優先順）

### P0（std 実運用で重要）

- `clone(2)` の Linux 互換意味論（`CLONE_*`）を実装
- `PT_TLS` ロードとスレッド生成時 TLS テンプレート展開
- `errno` の完全 TLS 化（スレッド単位）

### P1（互換性/品質）

- `nanosleep` を tick 依存の簡易待機から高精度実装へ改善
- signal 最低限互換（`rt_sigaction`, `rt_sigprocmask`, `SIGABRT/SIGSEGV` の整理）

### P2（拡張）

- `std::thread` / 同期プリミティブの長時間ストレス検証
- FS 周辺 syscall の POSIX 互換拡張（簡易実装から段階移行）

---

## 🧪 推奨検証手順

```bash
cargo fmt --all -- --check
cargo build --locked --quiet
cargo test --locked --quiet
timeout 110s cargo run
```

（上記に加え、`fork/exec/wait/futex` の長時間回帰を継続実施）
