# CI/CD Guide

## 目的

最小限の品質ゲートを常時実行し、セキュリティ修正の回帰を防ぐ。

## ワークフロー

### 1) `.github/workflows/ci.yml`

実行タイミング:
- push
- pull_request

主な検証:
- `cargo fmt --all -- --check`
- `cargo build --locked`
- `cargo test --locked`

補足:
- サブモジュールを再帰 checkout
- `src/lib` の `./configure` を CI で実行
- nightly toolchain + `rust-src` + `x86_64-unknown-uefi` target をセットアップ

### 2) `.github/workflows/security.yml`

実行タイミング:
- pull_request
- `dev` / `main` への push
- 定期実行 (毎週)
- 手動実行

主な検証:
- PR の依存関係レビュー (`dependency-review-action`)
- `cargo audit --deny warnings`

## ブランチ保護推奨

- 必須チェック: `CI / build-and-test`
- 推奨チェック: `Security / cargo-audit`
- force push 制限
- 承認付きレビューを必須化
