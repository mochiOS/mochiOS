# Operations Runbook

## 1. リリース前チェック

1. `cargo fmt --all -- --check`
2. `cargo build`
3. `cargo test`
4. CI 成功確認
5. 変更が security-sensitive な場合は `docs/hardening-checklist.md` を更新

## 2. セキュリティインシデント対応

1. 影響範囲の特定
   - 入口: syscall/interrupt/path parser
   - 影響: 情報漏えい / DoS / 権限昇格
2. 一時緩和
   - 機能フラグ/ガード条件で攻撃面を狭める
3. 恒久修正
   - 根本原因へ最小変更で対応
4. 回帰確認
   - 単体・統合・CI の全通過
5. ドキュメント更新
   - 仕様変更、制約、残課題を反映

## 3. 典型トラブルシュート

### newlib ビルド失敗
- `src/lib` で `./configure` 済みか確認
- cross tool が無い環境では fallback 設定を確認

### service build で JSON target エラー
- `-Z json-target-spec` が有効か確認
- service 側 `.cargo/config.toml` の build-std を確認

### wait/fork 周辺の停止
- `wait` の timeout 到達をログで確認
- 対象 PID の状態遷移を確認

## 4. 監査ログに残す項目

- 変更コミット
- 検証コマンドと結果
- 残課題/制約
- ロールバック方針
