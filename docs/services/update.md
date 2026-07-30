# update.service

`update.service`はDeveloper PKIのTrust SnapshotとRevocation Snapshotを同期する常駐サービスです。
OSイメージ、アプリケーション、Driver、MPKG自体の更新は担当しません。

## 起動と権限

実行ファイルは`/system/services/update.service`、Package IDは`org.mochios.update`です。
`service-manager.service`が`network.service`のReady確認後に起動します。起動失敗または同期失敗は
bootを停止しません。付与するCapabilityはファイル読み書き、HTTP client、IPC client/server、
署名DB更新通知、UTC参照に限定し、Process Spawn、raw network、device、DMA権限は持ちません。

## 同期

通常同期先は次の固定HTTPS URLです。redirectはHTTP client層の制限に従い、HTTPS downgradeや
別hostnameへの移動を許可しません。

```text
GET https://ca.mochios.org/v1/trust-store
GET https://ca.mochios.org/v1/revocations
```

Trustを先に取得・適用し、そのTrustに含まれるIssuerでRevocation署名を検証します。保存済みETagは
`If-None-Match`へ設定します。`304 Not Modified`ではSnapshotとslotを変更せず、`last_checked_at`
だけを更新します。304のETag不一致、複数ETag、128 bytes超のETagは拒否します。

初回、Trustは24時間ごと、Revocationは6時間ごとに同期します。有効期間の75%を経過した場合は
固定周期より早く同期します。時刻間隔はmonotonic clock、Snapshot期限はUTCで判断します。
DNS/TCP/TLS timeoutとHTTP 429/500/502/503/504は1分、5分、15分、1時間、最大6時間のbackoffで
再試行します。`Retry-After`は6時間を上限に尊重します。署名不正、rollback、不正形式は短周期で
再試行しないセキュリティエラーです。

## 検証と保存

JSONの型、canonical encoding、署名domain、successor規則は共有
`mochios-developer-ca-trust` crateを使用します。Trustは組み込みOffline Root公開鍵、Revocationは
Trust内の`active`または`retired` Issuerで検証します。レスポンス上限は4 MiBです。

保存先は`/libraries/certificate/`です。inactive slotへ書込み、fsync、再読込、再検証を行った後、
`state.bin`を更新してslotを切り替えます。詳細は
[Certificate Database](../security/certificate-database.md)を参照してください。

更新後は`TRUST_UPDATED`、続いて`REVOCATIONS_UPDATED`を`signature.service`へ通知します。通知は
versionとgenerationだけを伝え、Snapshot bytesや「検証済み」フラグは渡しません。
`signature.service`はディスクから独立に再検証します。

## 診断

`/system/logs/services/update.log`に、Trust/Revocationのversion、生成・期限・最終確認時刻、ETag、
active slot、失効件数、最終同期結果、最終同期エラー、次回deadline、試行・更新・304・失敗・署名拒否・rollback・
期限・storage・復旧の各counterを記録します。鍵、token、署名値は記録しません。

## 検証

外部サービスに依存しない決定的検証は`make developer-pki-sync-smoke-test`です。ホスト限定のtest Rootと
Issuerを使うTLS 1.3 serverを起動し、v1取得、ETag/304、v2更新、rollback、不正Root/Issuer署名、
失効serial、A/B保存、プロセス再起動相当のDB再読込を検査します。test秘密鍵はOSイメージへ収録しません。

本番確認はproduction Offline Root公開鍵を指定して実行します。

```text
MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX=<production-root-hex> \
  make developer-pki-production-e2e
```

この検査はDeveloperCAへTLS 1.3とWeb PKIで接続し、両APIの200、署名検証、永続保存、ETag付き304、
再読込を要求します。TrustまたはRevocation Snapshotが未発行の場合は成功扱いしません。

## 未実装事項

システムイメージ、AppStore catalog、アプリケーション、firmware、Driverの更新とMPKGの取得・installは
`update.service`の責務ではありません。version指定APIは診断・復旧用URLを生成できますが、通常同期では
latest endpointだけを使用します。また、Revocation Snapshotは6時間周期と期限接近時に同期しますが、
最終確認から24時間以上経過したMPKG installの直前に同期を強制するIPCはまだありません。install側は
期限内のTrustとRevocationが揃わない場合にfail closedします。
