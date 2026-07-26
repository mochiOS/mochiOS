# mochiOSの証明書と署名検証

この文書は、mochiOSにおけるRoot CertificateとDeveloper Certificateの**現在の実装**を説明します。
将来仕様を記した[mpkg形式](mpkg.md)および[パッケージ概要](packages.md)とは区別し、2026年7月時点のコードを正本とします。

## 結論

現在のmochiOSには、Root Certificateを信頼の起点としてDeveloper Certificateを検証するPKIはまだ実装されていません。

MPKGには`signatures/developer.cert`がありますが、内容は証明書ではなくEd25519の公開鍵です。
`signature.service`は、その公開鍵で同じMPKG内の`manifest.sig`を検証します。発行者、証明書チェーン、有効期限、失効、Package ID、Capabilityとの結び付けは検証しません。

また、カーネルが実行ファイルを許可するための`/signature.db`は、MPKGのDeveloper Certificateとは別の仕組みです。現在のカーネルはDB内のパスとSHA-256ダイジェストを照合しますが、DBに格納された公開鍵とレコード署名は検証していません。

| 項目 | 現在の状態 |
| --- | --- |
| mochiOS Root Certificate | 未実装 |
| Root公開鍵の組み込み、または信頼ストア | 未実装 |
| Developer Certificate | 名前は存在するが、実体は生のEd25519公開鍵 |
| Developer CertificateのRoot署名 | 未実装 |
| 証明書チェーン検証 | 未実装 |
| 証明書の期限検証 | 未実装 |
| 証明書失効確認 | 未実装 |
| MPKG manifest署名 | Ed25519で実装済み |
| MPKG payloadの完全性確認 | SHA-256とサイズの照合を実装済み |
| 実行ファイルの許可リスト | `/signature.db`のパスとSHA-256照合として実装済み |

## 2つの署名系統

現状は、次の2系統を独立して扱います。

```text
MPKGのインストール
  package.service
    -> signature.service
       -> MPKG内のdeveloper.certでmanifest.sigを検証
       -> manifestに記載されたpayloadのSHA-256とサイズを検証

バイナリの実行
  kernel execve
    -> /signature.dbを読み込む
    -> 実行パスと実行ファイルのSHA-256が登録レコードに一致するか確認
```

前者はパッケージ内容の整合性を確認し、後者は現在のrootfs上で実行可能なバイナリを限定します。両者の鍵、署名、信頼判断は接続されていません。

## MPKGのDeveloper Certificate

### コンテナ内の配置

現在の`signature.service`は、MPKG内に次の3項目があることを要求します。

```text
manifest.toml
signatures/manifest.sig
signatures/developer.cert
```

`signatures/chain/`以下のファイルはtarエントリとして受理され得ますが、チェーンとして解釈も検証もされません。

### `developer.cert`の実形式

`developer.cert`として受理される形式は次のどちらかです。

1. 32バイトのEd25519公開鍵
2. 同じ32バイトを表す64文字の16進数テキスト。前後の空白は除去される

X.509、DER、PEM、独自の証明書構造体ではありません。次のような証明書属性もありません。

- 発行者
- SubjectまたはDeveloper ID
- シリアル番号
- 有効期間
- 利用目的
- 許可Package ID
- 許可Capability
- Rootまたは中間証明書による署名

したがって、現在の`developer.cert`という名前は将来の役割を示す名前であり、実装上は`developer_public_key`に相当します。

### manifest署名

アルゴリズムはEd25519、ダイジェストはSHA-256です。`manifest.sig`は64バイト固定です。

署名対象は次のバイト列です。

```text
"mochios-mpkg-manifest-v1\0" || SHA-256(manifest.tomlの生バイト列)
```

`signature.service`は`ed25519-dalek`の`verify_strict`で署名を確認します。manifestの再シリアライズ結果ではなく、MPKGに格納されたUTF-8の生バイト列をハッシュするため、空白を含む変更も署名を無効にします。

署名検証後、manifestの各`[[file]]`について次を確認します。

- 対応するpayloadエントリが存在する
- エントリが通常ファイルである
- 実サイズがmanifestの`size`と一致する
- SHA-256がmanifestの`digest = "sha256:..."`と一致する
- manifestに記載されていないpayloadファイルが存在しない

このためmanifest署名が信頼できるという前提では、payloadの内容もmanifestへ推移的に結び付きます。

### `package.service`との連携

`package.service`はMPKG全体を読み込み、そのSHA-256を計算してから`signature.service`へ検証を依頼します。要求は現在、次の可変長形式です。

```text
u32 little-endian opcode (0x56455246)
32 bytes MPKG SHA-256
UTF-8の絶対パス（NUL終端なし）
```

`signature.service`は同じパスを読み直し、MPKG全体のSHA-256が要求値と一致することを確認してから内部を検証します。成功時は0、失敗時はerrnoを8バイトlittle-endianで返します。

`package.service`は成功応答を受け取るまでpayloadを配置しません。サービス間IPCのsender認証や専用共有Protocol crateは、この経路にはまだありません。

### サンプルMPKGの鍵生成

`scripts/build-sample-mpkg.pl`は、サンプルMPKGごとに一時的なEd25519秘密鍵をOpenSSLで生成します。その公開鍵の末尾32バイトを`developer.cert`へ格納し、manifest署名後に一時ディレクトリごと秘密鍵を破棄します。

この生成処理には次の性質があります。

- ビルドごと、かつサンプルパッケージごとに異なる鍵になる
- 秘密鍵はリポジトリや成果物へ保存されない
- 固定のDeveloper identityを表さない
- Root Certificateによる発行処理を行わない

これは形式とインストール経路を試すサンプル生成器であり、製品用のDeveloper Certificate発行ツールではありません。

## Root Certificate

### 現在の実装状態

リポジトリには、mochiOS Root Certificateのバイト列、Root公開鍵、Root秘密鍵、信頼ストア、証明書発行処理はありません。`signature.service`にも信頼アンカーをロードする処理はありません。

現在の検証関係は次のとおりです。

```text
MPKGがdeveloper.certを同梱
        |
        v
同じMPKGのmanifest.sigを検証
```

実装されていない、本来の証明書チェーンは次の部分です。

```text
mochiOS Root Certificate（信頼済み）
        |
        v  発行者署名の検証
Developer Certificate
        |
        v  パッケージ署名の検証
manifest.sig
```

このため、現在のMPKG検証が保証するのは「同梱された公開鍵に対応する秘密鍵でmanifestが署名され、payloadがそのmanifestと一致すること」です。「その公開鍵をmochiOSが信頼した開発者へ発行したこと」は保証しません。

攻撃者が独自の鍵でMPKGと署名を作成した場合も、形式と内容が正しければ現在の`signature.service`単体では拒否できません。Root Certificate導入前にMPKGを外部配布の信頼境界として扱うことはできません。

### 既存文書との関係

`docs/mpkg.md`にはDeveloper Certificate、証明書チェーン、失効状態、`CertificateAllowed`によるCapability制限が記載されています。これらは目標仕様であり、現在の`signature.service`には未実装です。

Root Certificateを実装する際は、少なくとも次を仕様として固定する必要があります。

- 証明書のwire formatと正規化規則
- Root公開鍵を保護して配布する信頼境界
- Developer identity、鍵ID、シリアル番号
- 有効期間と時計が未確立なboot時の扱い
- Package IDやCapability許可範囲との結び付け
- 中間証明書の可否とチェーン構築規則
- 失効リスト、その更新主体、オフライン時の扱い
- 鍵ローテーションと複数Rootの移行手順
- 不明なcritical fieldをfail closedにする規則

秘密鍵はOSイメージや公開リポジトリへ収録せず、Root秘密鍵とDeveloper秘密鍵の運用を分離する必要があります。

## カーネルの`/signature.db`

### 目的と生成

`scripts/build-signature-db.pl`は、ビルド対象のサービス、アプリケーション、コマンド、Driverなどから`/signature.db`を生成します。生成時には一時Ed25519鍵を作り、各レコードについて次を計算します。

```text
SHA-256(実行ファイル)

署名対象:
"mnu-signature-v1\0" || 実行パス || "\0" || SHA-256(実行ファイル)
```

テキストDBの概形は次のとおりです。

```text
mnu-signature-db v1
pubkey <32バイト公開鍵のhex>
record <実行パス> <SHA-256のhex> <64バイト署名のhex>
```

`scripts/build.pl`が登録対象を列挙し、生成したDBをrootfsの`/signature.db`へ配置します。鍵はビルドごとに一時生成され、MPKGサンプルの鍵とは共有されません。

### 実行時の実際の検証

カーネルの`core/src/policy/signature.rs`はDBを初回利用時に読み込みます。DBが欠落または構文不正ならfail closedとなり、rootfsからの実行を許可しません。

現在ロードして保持するレコードは次の2項目だけです。

- 実行パス
- SHA-256ダイジェスト

`pubkey`行は存在だけを確認し、値を公開鍵としてdecodeしません。各`record`の署名文字列も存在だけを確認し、decodeもEd25519検証も行いません。`execve`時の許可条件は次の一致です。

```text
要求された実行パス == record.path
かつ
SHA-256(ロードした実行ファイル) == record.digest
```

したがって`/signature.db`は現状、暗号学的に自己検証する署名DBではなく、信頼済みrootfsからロードするビルド時生成の実行許可リストです。DBの公開鍵をmochiOS Root Certificateと呼ぶことはできません。

boot-stageのcextはrootfsのマウント前にロードされるため、このDBでは検証されません。コード上はinitfs自体をboot時の信頼境界として扱い、既知の組み込みbundle登録とcextメタデータを確認します。

## Capabilityとの関係

現在のDeveloper公開鍵には、許可Capabilityの情報がありません。`signature.service`も証明書由来のCapability上限を返しません。

Capabilityの解決は`capability.service`とmanifest/policy側の別責務です。`docs/mpkg.md`にある次の式のうち、`CertificateAllowed`をDeveloper Certificateから導出する処理は未実装です。

```text
Requested
∩ CertificateAllowed
∩ binaryPolicy
∩ Delegatable
```

したがって現時点では、MPKG署名が成功したことを「Developer Certificateが要求Capabilityを許可したこと」と解釈してはいけません。

## 実装ファイル

| ファイル | 現在の責務 |
| --- | --- |
| `services/signature/src/main.rs` | MPKG解析、Developer公開鍵decode、manifest署名、payload照合、検証IPC |
| `services/package/src/main.rs` | MPKG全体digest付きの検証要求、検証後のインストール |
| `scripts/build-sample-mpkg.pl` | 一時鍵を使ったサンプルMPKG生成 |
| `scripts/build-signature-db.pl` | 実行許可DBと、現在未検証のレコード署名の生成 |
| `scripts/build.pl` | DB登録対象の列挙とrootfsへの配置 |
| `core/src/policy/signature.rs` | `/signature.db`のロード、パスとSHA-256の照合 |
| `core/src/syscall/exec.rs` | `execve`でカーネル署名Policyを適用 |

## 現在保証されること、されないこと

### 保証されること

- MPKGのmanifestが、同梱公開鍵に対応する秘密鍵で署名されている
- manifestの変更はEd25519検証で検出される
- manifestに列挙されたpayloadのサイズとSHA-256が一致する
- 未列挙のpayloadファイルは拒否される
- rootfs実行ファイルは、`/signature.db`に登録されたパスとSHA-256に一致する必要がある
- `/signature.db`の欠落または構文不正は実行許可へ倒れない

### 保証されないこと

- MPKG署名者がmochiOSに承認されたDeveloperであること
- Developer公開鍵がRoot Certificateから発行されたこと
- 証明書の有効期限、失効、利用目的
- DeveloperごとのPackage IDまたはCapability制限
- 証明書チェーンの真正性
- `/signature.db`内の公開鍵とレコード署名の暗号学的検証
- `/signature.db`自体の署名または改ざん検出

この区別が、現行実装を評価するときの信頼境界です。
