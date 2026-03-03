# Hardening Checklist

## カーネル防御

- [x] NXE 有効化
- [x] SMEP/SMAP 有効化
- [x] カーネル `.text` Read-Only 化
- [x] SYSRET 前 non-canonical RSP 防御
- [x] KPTI の CR3 切り替え導入

## Syscall 防御

- [x] 主要 syscall のユーザーポインタ検証
- [x] `read_cstring` の境界外読み出し防止
- [x] `arch_prctl(ARCH_SET_FS)` の上限検証
- [x] `wait` のタイムアウト保護
- [x] IPC 競合窓の明文化

## スレッド/プロセス整合性

- [x] core.service の二重登録防止
- [x] fork でのユーザーコンテキスト保存をスレッド単位化
- [x] カーネルスタック確保時の不要ロック除去

## ビルド/検証

- [x] `cargo fmt --all -- --check`
- [x] `cargo build`
- [x] `cargo test`
- [x] GitHub Actions CI 導入
- [x] 依存関係監査ワークフロー導入

## 残課題 (段階対応)

- [ ] futex を実待機キュー方式へ拡張
- [ ] int 0x80 / syscall 経路のコンテキスト保存完全統一
- [ ] SMP 前提の per-CPU データ設計の拡張
