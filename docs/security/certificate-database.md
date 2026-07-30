# Certificate Database

Developer PKI databaseの固定保存先は`/libraries/certificate/`です。

```text
/libraries/certificate/
  trust-a.json
  trust-b.json
  revocations-a.json
  revocations-b.json
  state.bin
```

## A/B commit

更新はactiveでないslotへSnapshotを書き、fsync、同一bytesの再読込、署名・metadataの再検証を行います。
その後だけ`state.bin`のactive slotとgenerationを更新してfsyncします。Snapshot書込み中またはstate
commit前に停止しても、旧active slotを継続利用できます。ext2 renameのtransaction性には依存しません。

起動時はstateとactive slotを検証します。state破損、active slot破損、metadata不一致では両slotを
検査し、versionと生成時刻が最も新しい有効候補へfallbackします。`update.service`は復旧したstateを
書き戻します。読取り専用の`signature.service`はfallback結果を利用しますが、stateを修復しません。
両slotが無効なら空databaseとなり、新規MPKG installは`EAGAIN`で拒否されます。

## state.bin v1

`state.bin`はlittle-endianの固定392 bytesです。magicは`CPKI`（数値`0x494b5043`）、format versionは
1です。generation、Trust/Revocationのactive slot、snapshot version、`generated_at`、`expires_at`、
`last_checked_at`、最大128 bytesのETagを保持し、末尾32 bytesは直前360 bytesのSHA-256です。

decoderは長さ過不足、magic/version/encoded length不一致、A/B以外のslot、非0 reserved、ETag長・
UTF-8・構文不正、ETag後方の非0 padding、checksum不一致を拒否します。Snapshotは各4 MiBを上限とし、
既知の5 path以外を共有File backendで開きません。

Snapshot JSONの意味と署名は`mochios-developer-ca-trust`、stateとA/B操作は
`mochios-certificate-database`が共有実装です。後者は`no_std`、`alloc`、`std` featureを分離し、
サービスの実ファイルI/Oでは`std` featureを使います。

