# mochiOSの証明書と署名検証

パッケージ形式の正本は[mochiOS Package Format](mpkg.md)です。この文書はRoot鍵、Developer Certificate、署名ツール、実行許可リストの運用境界を説明します。

## 信頼モデル

MPKG v1の現在の信頼チェーンはDeveloperCAのIssuerを含みます。

```text
signature.serviceへ組み込まれたOffline Root公開鍵
        | Trust SnapshotへのEd25519署名
        v
DeveloperCA Issuer公開鍵
        | Ed25519署名
        v
Developer Certificate v1 / Revocation Snapshot
        | Ed25519署名
        v
manifest.toml -> SHA-256で各payloadへ結合
```

MPKG内に任意の証明書chainを埋め込む方式は実装しません。`signatures/chain/`にエントリがあるMPKGは
拒否し、信頼するIssuerは同期済みTrust Snapshotだけから取得します。

Root秘密鍵はOSイメージ、公開リポジトリ、通常のイメージビルド環境へ置きません。`tools/devkit/fixtures/development`には再現可能な開発イメージ専用のDeveloper秘密鍵、事前発行済み証明書、Root公開鍵だけがあります。この鍵は公開済みであり、製品identityには使用できません。

## Developer Certificate v1

実装は共有`no_std` crateの`mochios-certificate`です。X.509、DER、PEMはMPKG内のwire formatに使用しません。整数はすべてlittle-endianです。

固定ヘッダーは144 bytesです。

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | magic `MCER` |
| 4 | 2 | format version `1` |
| 6 | 2 | header length `144` |
| 8 | 4 | certificate全長 |
| 12 | 8 | serial number |
| 20 | 32 | issuer key ID |
| 52 | 32 | subject key ID |
| 84 | 32 | Ed25519 subject public key |
| 116 | 8 | `not_before` Unix time |
| 124 | 8 | `not_after` Unix time |
| 132 | 4 | key usage |
| 136 | 2 | developer ID byte length |
| 138 | 2 | Package ID scope count |
| 140 | 2 | allowed Capability count |
| 142 | 2 | reserved、0固定 |

ヘッダー後にはdeveloper ID、Package ID scope、Capabilityを正規順で格納し、最後の64 bytesを
DeveloperCA IssuerによるEd25519署名とします。scopeは`kind: u8`、`reserved: u8 = 0`、
`length: u16`、UTF-8値です。`1`は完全一致、`2`は`.`境界だけを認めるprefixです。Capabilityは
`length: u16`とUTF-8値です。

署名対象は次です。

```text
"mochios-certificate-v1\0" || certificate_without_signature
```

Subject Key IDとIssuer Key IDは公開鍵32 bytesのSHA-256です。key usage v1はpackage signingの`1`だけです。未知field、未知flag、未知scope、非0 reserved、重複、未ソート項目、不正UTF-8、不正識別子、余分なbyte、短い入力を拒否します。

Package ID prefixは`org.mochios`に対して`org.mochios.app`を許可しますが、`org.mochiosx`は許可しません。globと正規表現はありません。Capability名は完全一致だけです。

## Root公開鍵、Trust Snapshotと時刻

`signature.service`と`update.service`の`build.rs`がOffline Root公開鍵をバイナリへ埋め込みます。
既定値はdevelopment Rootです。製品ビルドは次を指定します。

```text
MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX=<64桁hex>[,<64桁hex>...]
```

旧Rootと新Rootを同時に列挙すると、両Rootが署名したTrust Snapshotを移行期間中に検証できます。
移行後は旧Rootをリストから外します。

期限検証は`system.time.read`で取得する実行時UTCを使用します。同期DBの形式、A/B保存、失効、期限切れ
Policyは[Developer PKI同期](security/developer-pki-sync.md)を参照してください。

## 失効

失効serialはDeveloperCAのIssuer署名済み累積Revocation Snapshotから読みます。Snapshotは
`update.service`が定期同期するため、`signature.service`を含むイメージ更新は不要です。OCSPは
実装していません。

## MPKG検証順

`signature.service`は次の順でfail closedに検証します。

1. MPKGヘッダーとtar構文
2. 必須entry、重複、未知signature entry、`signatures/chain/`不在
3. Developer Certificateの構文と正規形
4. Issuer Key IDに対応する同期済みIssuerの選択とstatus確認
5. Issuer署名、Certificate期限、key usage、Package ID scope
6. 証明書シリアルの失効状態
7. Developer公開鍵による`manifest.sig`
8. manifestに列挙されたpayloadのサイズとSHA-256
9. 未列挙payloadがないこと

成功結果にはdeveloper ID、certificate serial、subject key ID、verified Package ID、allowed Capability、manifest digest、package digestを含めます。

## package.serviceとのIPC

共有`no_std` crateの`mochios-signature-protocol`がwire formatを定義します。共通headerはmagic `MSIG`、version、opcode、request ID、payload length、flagsを持ち、little-endianで明示的にencode/decodeします。余分なbyte、未知version/opcode、非0 flag/reservedを拒否します。

IPCの1メッセージ上限に合わせ、`package.service`が保持するMPKG bytesを`VERIFY_BEGIN`、連番offset付き`VERIFY_CHUNK`、`VERIFY_FINISH`で転送します。`signature.service`はパスを受け取らずファイルを再オープンしません。検証対象とインストール対象は同じ`Vec<u8>`です。各chunkは応答でflow controlされ、全体長とpackage digestも照合します。

`signature.service`はIPC senderの所有プロセスに`package.install`が付与されていることをカーネルへ照会し、権限のない直接検証要求を拒否します。通常経路ではこの権限を持つ`package.service`が、読み込んだ同一のMPKG bytesを検証へ送ります。

検証結果は`/system/packages/<package-id>/verification.bin`へ保存します。`capability.service`はmanifest digestとPackage IDを再確認し、要求Capabilityが証明書のallowed Capabilityに完全一致しなければ拒否します。現行manifestの`requires`はすべて必須要求なので、許可されない要求を黙って除去しません。

## msign

署名ツールはrepo管理対象の`tools/devkit/crates/msign`にあります。従来Komeが使用する`.pkg`向けコマンドを維持し、MPKG用に次を追加しています。

```text
msign key generate --private-key developer.key --public-key developer.pub
msign certificate issue --root-key root.key --developer-key developer.key \
  --output developer.cert --serial 1 --developer-id org.example.developer \
  --not-before 1700000000 --not-after 1800000000 \
  --scope prefix:org.example --capability fs.read.user.documents
msign certificate inspect developer.cert
msign package sign app.mpkg --certificate developer.cert --key developer.key
msign package verify app.mpkg --root-public-key root.pub --unix-time 1750000000
```

`package sign`はDeveloper秘密鍵と証明書のSubject公開鍵が一致することを確認し、証明書とmanifest署名を決定的なustarへ格納します。`package verify`はOS側と同じ証明書、manifest、payload検証を行い、identityとdigestを表示します。

## execution.allowlist

`/libraries/system/execution.allowlist`はDeveloper Certificate PKIとは別のbootstrap機構です。カーネルがrootfs上で実行を許すpathとSHA-256をビルド時に固定します。

```text
mnu-execution-allowlist v1
record /bin/ls <SHA-256 hex>
```

以前の`/signature.db`は公開鍵とレコード署名を生成していましたが、カーネルが検証していませんでした。誤解を生む未検証fieldを削除し、実態に合わせて改名しています。allowlistの欠落、構文不正、pathまたはdigest不一致はfail closedです。これは信頼済みrootfsを前提とする実行制御であり、Root CertificateやDeveloper Certificateとは呼びません。

## 主な実装

| Path | Responsibility |
| --- | --- |
| `user/crates/certificate` | Developer Certificate v1 encode/decode/verify |
| `user/crates/signature-protocol` | package/signature間wire format |
| `tools/devkit/crates/msign` | 鍵生成、証明書発行・表示、MPKG署名・検証 |
| `services/signature` | Root、期限、失効、manifest、payload検証 |
| `services/package` | 同一bytes転送、検証結果保存、payload配置 |
| `services/capability` | CertificateAllowed上限の強制 |
| `core/src/policy/signature.rs` | execution allowlist照合 |
