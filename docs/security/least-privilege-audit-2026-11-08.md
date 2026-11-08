# 最小権限と受信境界の監査（2026-11-08）

## 対象と方法

各project直下のmanifestを読み、mnu ABIの登録Capabilityと照合しました。

| 対象 | manifest数 | binary宣言数 |
| :-- | --: | --: |
| applications | 17 | 17 |
| services | 20 | 20 |
| binaries | 2 | 23 |
| 合計 | 39 | 60 |

対象manifestの`requires`に未登録のCapability名はありませんでした。これだけで権限が強制されるとは判定していません。
capability.serviceのmanifest認証・証明書の許可範囲・実行用bytesの結合・起動brokerの役割検査、
package・user・keychain・workspaceの受信側認可を静的に追跡しました。

依頼に従い、試験コードの追加、試験実行、ビルド、KVM起動は行っていません。
本記録は静的監査の結果であり、実行時の成功やリリース承認を示すものではありません。

## 確認した問題と変更

| 問題 | 判断と対応 |
| :-- | :-- |
| Workspaceのセッション境界が未検査 | High。[MOC-SA-2026-1108-0008](../../security/MOC-SA-2026-1108-0008.md)へ登録。既存のProcessDomainを起動側から明示委譲し、全操作で送信元の所属とPOSIXの実UID・実効UIDを確認する実装へ変更しました。起動時の委譲不在と照会失敗も拒否します。動作未検証です。 |
| 通知送信のCapability検査漏れ | `ipc.client`だけで通知を受理していたため、既存の`notification.send`を要求します。既存のpackage identity照合とアプリ別通知設定も維持します。送信機能を持つTest.appのmanifestへ必要権限を明記しました。 |
| capability.serviceの不要な権限 | 現行処理に用途のない`net.connect`・`usb.access`をmanifestから削除し、旧about定義のUSB権限も除去しました。子への実行認可は`capabilities.manage`で行う別の機構であり、broker自身のネットワーク・USB権限を必要としません。 |

主な修正commitは、user `4046781`、services `2f3cf3d`・`4f6ded6`・`a2c9b3a`、Test.app `3d0ced2`です。
Workspaceの変更には共有定義と起動側の更新が必要です。古い起動側へのfallbackはありません。

## 残る確認・設計事項

- Settings・Installerと複数のサービスに`fs.write.all`が残っています。既存処理はアカウント・設定・package・private database・更新先等を操作します。個々の処理を追い、対象Handleの委譲やbroker操作へ狭める設計が必要です。権限名だけを削除して最小権限化が完了したとは扱いません。
- Workspaceでは通知一覧の取得と通知の削除・設定変更を`control-center.read`で許可しています。読み取りと管理操作を分ける方針が残っています。
- `account.other.modify`を持つSettings等の操作と、利用者に要求する管理者認証の方針を照合する必要があります。今回、UIDや管理者の役割を新たな推測で定義していません。
- パスに対するnamed Capabilityと、POSIXのUID/GID・root/cwd・明示的なFileHandle委譲の境界を通した監査は未完了です。
- カーネル全syscall、MMIO/DMA、全IPC protocol、配布ビルドの鍵・開発設定、更新・復旧経路の監査は本記録で完了としません。

既存のAdvisory 7件は内容を変えずに`core/security/`からルートの`security/`へ移しました。
それぞれの過去の修正状態を今回再評価したという意味ではありません。新しい問題の状態も、実装済みと動作確認済みを分けて記録します。
