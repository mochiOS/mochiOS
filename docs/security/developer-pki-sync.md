# Developer PKI同期

Developer PKIは次の信頼連鎖を使用します。

```text
OSへ組み込んだOffline Root公開鍵
  -> Root署名済みTrust Snapshot
  -> active/retired Issuer公開鍵
  -> Developer CertificateとRevocation Snapshot
  -> Developer公開鍵によるMPKG manifest署名
```

Offline Root秘密鍵とIssuer秘密鍵はOSイメージ、リポジトリ、CI、QEMU fixtureへ格納しません。
製品ビルドでは`MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX`へ32-byte Ed25519公開鍵をhexで指定します。
comma区切りで複数Rootを指定できるため、Root移行期間だけ旧鍵と新鍵を併用できます。未指定の
開発ビルドは公開済みdevelopment Rootと署名済みfixtureを使用し、production identityには使えません。

## Snapshot検証

Trust署名domainは`mochios-issuer-trust-snapshot-v1\0`、Revocation署名domainは
`mochios-revocation-snapshot-v1\0`です。独自JSON再構築は行わず、CloudとOSで共有するcanonical
encodingを署名対象にします。未知field、未知format/algorithm、非正規順、重複、上限超過を拒否します。

Trust successorはversionと生成時刻の前進、既存Issuerの保持、同一key IDの公開鍵不変、許可された
status遷移を要求します。Revocation successorは累積形式であり、既存serialの欠落、`revoked_at`変更、
reason矛盾を拒否します。これにより古いSnapshotへのrollbackとIssuer差替えを防ぎます。

## 期限切れPolicy

保存済みSnapshotは期限後も署名と構造を検査したうえで復旧候補として保持できます。ただし、新規
MPKG installはTrustとRevocationの両方が現在UTCで有効な場合だけ許可し、欠落または期限切れなら
`EAGAIN`でfail closedします。既に展開済みでexecution allowlistに含まれるbinaryの起動は、MPKGの
新規install検証とは別のboot policyです。

同期不能時は最後に検証済みのdatabaseを維持し、OS bootは継続します。不正な更新通知や再読込失敗で
memory上のactive databaseを置換しません。

## 境界

`update.service`は取得・schedule・ETag・検証・永続化を担当します。`signature.service`はnetwork権限を
持たず、active databaseを再検証してDeveloper Certificate、失効、MPKGを判定します。詳細は
[update.service](../services/update.md)、[signature.service](../services/signature.md)、
[Certificate Database](certificate-database.md)を参照してください。

