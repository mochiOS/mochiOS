# 診断送信の実装状況

2026-11-12時点。ローカルのworkerd / D1での受信・表示と、本番の受信ルート・認証・
schemaを確認しています。mochiOS実機から本番への送信と、サーバー側の保持・削除の
運用確認は含みません。詳細な[ローカルクラッシュ診断](crash-diagnostics.md)は別の保存経路です。

## 現在接続されているもの

| 種類 | 実装 | 送信先・タイミング |
| --- | --- | --- |
| 利用診断 | `update.service`から送信 | HTTPS `https://api.mochios.org/diagnostics`、UTC日単位で1件をqueue |
| OS更新結果 | `update.service`から送信 | 同ホストの`/update-results`、更新結果が確定した後 |
| クラッシュ報告 | 同意区分のみ。個別確認と送信は未接続 | ローカルJSONや今回のUIからは送信しない |

送信元は`services/update/src/diagnostics.rs`、HTTP処理は`public_api.rs`です。
一般的なサービスログ、クラッシュJSON、memory dumpやstack内容を丸ごと送る実装ではありません。
外部送信queueは上の2 endpointだけを受け付けます。

## 受信APIとAdminDashboard

受信側は[mochiOS/diagnosis-api](https://github.com/mochiOS/diagnosis-api)です。
`/diagnostics`と`/update-results`を、16KiBの上限と明示的なJSON schemaで受け付けます。
event IDと正規化した内容のSHA-256で再送を重複排除し、同じIDで異なる内容は409で拒否します。
本文・ファイルパス・memory/stack/registerの内容を保存する欄や、公開の診断閲覧APIはありません。
クラッシュの受信endpointはまだ提供しません。

CloudWorkspaceでは`services/diagnosis-api`として管理します。AdminDashboardと同じ
REPORTS・PUBLIC_CATALOGのD1 identityとローカル保存先を使い、migrationと起動を組み込みます。
本番では診断の2パスだけを`mochios-diagnosis-api`へ接続し、その他の更新・配布APIは既存の
`mochios-public-api`を維持しています。無効だった公開ホストの割り当ても有効にしました。
旧診断DBは0件であることを確認し、件数を再検証するtransactional migrationで旧tableを
`legacy_*_20261111`へ退避してから現在のschemaへ移行しました。報告がある旧DBでは中止します。

AdminDashboardの**診断統計 → 受信レポート**から種類別の一覧と詳細を閲覧できます。
`/api/admin/reports`と`/api/admin/reports/:kind/:event_id`は既存のAccess署名・active memberを
検証し、Owner / Admin / 閲覧専用Operatorにだけ返します。1ページ50件のcursor paginationで、
端末IDや内部hashはブラウザへ返しません。起動結果・hardwareのnullは未計測として表示します。

受信から一覧・詳細・集計までを、実際のworkerdと共有D1、署名付きの検証用Access JWTで
確認しました。ブラウザでページ送り・詳細遷移・通信失敗後の復帰も確認しています。
本番では配信code、接続先DB、認証と同意なし送信の拒否を確認し、検証用の報告は保存していません。
これはmochiOS実機から本番へ送った報告の到着確認済みという意味ではありません。

## 送信する内容

利用診断:

- eventごとのランダムUUIDと、端末に保持するランダムな`device_id`。
- OS version / build number / architecture。
- `install_date`、`last_active_date`（UTCの日付）。`install_date`は時計が有効になった最初の日付で、
  厳密なインストール時刻を計測しているわけではありません。
- `basic_diagnostics_consent=true`、`additional_diagnostics_consent=false`。
- 現在は未計測: `boot_succeeded=null`、`is_mochi_pc=null`。
  従来の固定値を実測値として集計しないよう、新しい報告では不明を明示します。
- 現在は不明: `previous_shutdown="unknown"`、`ram_band="unknown"`。
- 現在は収集なし: `frequent_apps=null`、`country_or_region=null`。

OS更新結果:

- event UUID、同じ端末UUID、発生日のUTC日付。
- 更新元・更新先それぞれのversion / build number / architecture。
- `first_boot_succeeded` / `failed` / `rolled_back`。
- 失敗時だけ、英数字・`-`・`_`に限定した最大64文字のerror code。
- JSONの`basic_diagnostics_consent=true`は既存フィールド名です。実際の許可は独立した
  `UpdateResults`区分で判定し、利用診断への同意を代用しません。

ユーザー名、メールアドレス、開いたファイル、起動引数、環境変数、資格情報、raw memory、
stack内容、ローカルクラッシュのPC/SP/fault addressをこれらのbodyへ追加していません。
ランダムな端末UUIDは端末内で継続する識別子であり、毎回匿名化されるevent UUIDとは異なります。

## 同意の根拠

共通ライブラリは`tools/devkit/crates/device-consent`、保存先は
`/var/config/privacy/consent.json`です。Installerが永続化可能になったインストール先へ記録します。

schema versionは2で、利用診断・クラッシュ報告・更新結果の3区分を独立して保持します。
terms / privacy / OSS licenseの文書version・SHA-256・確認時刻、および全区分の明示決定を
持つ完全なrecordが前提です。各区分の`AGREE`、現行policy version、非ゼロrevisionを照合します。
未設定・破損・不完全・古いpolicy versionは許可しません。現在のpolicy versionは
**`placeholder-2`**で、正式リリースの文書revisionはまだ確定していません。

今回以前のSettingsは`/var/config/diagnostics/settings.conf`へ独自のON/OFFを書いていましたが、
送信側はこのファイルを参照していませんでした。今回、動作しない同意スイッチを送信状態として
見せる表示を取り除き、実際の共通同意ストアを読む区分別の状態表示に変更しました。
読めない場合は不明として表示します。Settingsからの同意変更はまだ提供していません。
従来の設定ファイルは送信許可を与えません。

## queue・再送・撤回

```text
/var/lib/diagnostics/device_id        端末UUID
/var/lib/diagnostics/install_date     最初の有効なUTC日付
/var/lib/diagnostics/queue/           送信待ち
/var/lib/diagnostics/quarantine/      自動再送しない記録
/var/lib/update/pending-result.json   更新結果が送信queueへ移る前の状態
```

queue追加時と送信の直前に、現行の区分・schema・policy version・決定時刻・revisionを
照合します。撤回や再決定で一致しなくなった古いqueueは、再同意しても復活させません。
許可されていない区分のqueue・quarantineを削除し、更新結果が不許可なら未送信の結果も削除します。
同意確認の予定間隔は約60秒で、ネットワーク処理時間等によって遅れる場合があります。
送信完了済みデータをサーバーから削除するAPIはこの実装にありません。

200/201で完了としてqueueから削除します。通信失敗・429・5xxは60 / 300 / 900 / 3600 /
21600秒の段階的待機に小さなjitterを加えて再試行し、`Retry-After`が長ければそちらを優先します。
その他のHTTP応答はquarantineへ移して自動再送しません。日単位の利用診断には完了・拒否markerが
あり、同じ日の再生成を防ぎます。queueの経過日数による自動失効処理はありません。
`PENDING_RETENTION_SECONDS`定数だけを根拠に「7日でqueueを消す」とは扱いません。

更新確認のGETは任意診断の同意と独立しています。`/health`、`/distribution/status`、
`/updates/check?version=…&build=…&architecture=…`を使い、通常は約6時間間隔にjitterを加えます。
この更新確認リクエストへ診断用の端末UUIDは付けません。

## リリース前に残っている点

- 正式policy versionと、その文書・送信項目の整合。
- 共通同意ストアを用いた、実効性のあるSettings側の変更・撤回UI。
- クラッシュごとの内容確認・明示送信と専用の送信契約。現状のクラッシュ同意だけでは送らない。
- 起動結果・ハードウェア区分の実測取得、および送信待ちqueueの容量・保持期限の運用方針。
- サーバー側の保持・削除・問い合わせの運用。この文書はクライアント実装から保証しない。
