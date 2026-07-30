# signature.service

`signature.service`はDeveloper PKI databaseを読み、MPKGを検証するサービスです。HTTP通信は行わず、
`net.http.request`を持ちません。

起動時に`/libraries/certificate/state.bin`とactive Trust/Revocation slotを読み、組み込みOffline Root、
Trust内Issuer、Snapshot署名、version、構造を独立に検証します。更新通知を受けた場合も同じ読込みを行い、
通知されたversion/generation以上であることを確認してからmemory上のdatabaseを置換します。senderには
`signature.db.write`を要求します。不正通知、disk破損、署名不正では既存memory状態を維持します。

Developer CertificateはIssuer Key IDでTrust Snapshotを検索します。`active`と`retired`は検証可能、
`future`と`revoked`は拒否します。Issuer署名、Certificate期限、Package ID scope、Capability上限を確認し、
続いてRevocation Snapshotのserialを照合してからMPKG manifestとpayload digestを検証します。

新規installでは現在UTCに対してTrustとRevocationの両Snapshotが有効であることが必須です。database欠落、
片方のSnapshot欠落、期限切れ、UTC取得不能は`EAGAIN`です。失効済みまたは署名不正は許可しません。

`package.service`とのchunked IPC、MPKG wire format、保存されるverification recordは
[MPKG](../mpkg.md)を参照してください。同期と保存は
[Developer PKI同期](../security/developer-pki-sync.md)を参照してください。

