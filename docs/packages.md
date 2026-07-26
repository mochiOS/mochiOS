# Package and mpkg Format

> **実装状況:** 証明書と信頼チェーンに関する記述には目標仕様を含みます。現在の実装範囲と制約は[mochiOSの証明書と署名検証](certificates.md)を参照してください。

mochiOS では、アプリケーション、サービス、ドライバを「パッケージ単位」で扱います。
実行バイナリと個別の `.toml` を `/bin` 直下へ並べる方式は廃止し、信頼情報と配布情報は
`/system/packages/<package>/manifest.toml` に集約します。

この文書は、今後の配布形式として定義する `.mpkg` と、それを展開する `package.service` の責務をまとめたものです。

## 目的

`.mpkg` は、mochiOS のアプリインストーラー形式です。

狙いは次の通りです。

- 1つのパッケージに実行ファイル、manifest、署名情報をまとめる
- `/bin/*.toml` のような分散メタデータをなくす
- `signature.service` と `capability.service` に検証責務を集約する
- `package.service` が展開と配置を担当する
- 検証失敗時は fail closed にする

## ディレクトリ構成

インストール後の標準構成は次の形です。

```text
/bin/
├─ ls
├─ cp
└─ mv

/system/packages/
└─ coreutils/
   └─ manifest.toml
```

`/bin` には実行ファイルだけを置きます。
パッケージの正規なメタデータは `/system/packages/<package>/manifest.toml` に置きます。

## mpkg の論理構造

`.mpkg` は 1 つの配布単位です。物理的なコンテナ形式は将来の実装で固定しますが、論理的には次の内容を含みます。

```text
mpkg
├─ manifest.toml
├─ payload/
│  ├─ bin/ls
│  ├─ bin/cp
│  └─ bin/mv
├─ certs/
│  └─ developer-chain...
└─ signatures/
   ├─ manifest.sig
   └─ payload.sig
```

要点は次の通りです。

- `manifest.toml` はパッケージの正本
- `payload/` は実体ファイル
- `certs/` は Developer Certificate と証明書チェーン
- `signatures/` は manifest と payload の完全性を示す署名

インストール時は、payload の相対パスをそのまま `/` 配下へ展開します。
たとえば `payload/bin/ls` は `/bin/ls` として配置します。

## manifest.toml

`manifest.toml` は、配布と起動の両方で使う信頼できるメタデータです。

現在の基本形は次の通りです。

```toml
[package]
id = "org.mochios.coreutils"
name = "coreutils"
version = "0.1.0"
vendor = "mochiOS Project"

[[binary]]
path = "/bin/ls"
kind = "application"
capability_profile = "coreutils.ls"
requires = [
    "fs.read.all",
]

[[binary]]
path = "/bin/cp"
kind = "application"
capability_profile = "coreutils.cp"
requires = [
    "fs.read.all",
    "fs.write.all",
]
```

`[[binary]]` は、パッケージが提供する実行対象を列挙します。
`path` は実行時の絶対パスです。
`capability_profile` は、Capability 判定用の論理名です。

`kind` は少なくとも次を想定します。

- `application`
- `service`
- `driver`

## package.service の責務

`package.service` は、パッケージの展開担当です。
カーネルは package の概念を知りません。

`package.service` の責務は次の通りです。

- `.mpkg` の受け取り
- `signature.service` への署名検証要求
- `capability.service` への Capability 判定要求
- manifest の読み込み
- payload の展開
- 展開先の整合性確認
- `/system/packages/<package>/manifest.toml` の配置
- `/bin/...` への実行ファイル配置
- インストール済みパッケージ索引の更新
- 失敗時のロールバック

`package.service` は、検証済みの入力しか展開しません。
署名未検証、manifest 未検証、証明書失効、Capability 超過は、すべて拒否します。

## 実行フロー

インストール済みパッケージの実行フローは次の通りです。

```text
プロセス起動要求
  ↓
実行対象バイナリのパスを解決
  ↓
所属するパッケージと manifest を特定
  ↓
signature.service へ署名検証を依頼
  ↓
capability.service へ Capability 判定を依頼
  ↓
許可された Capability だけを初期 Capability として付与
  ↓
process_spawn
```

`package.service` は、この流れのうち「配置」と「索引管理」を担当します。
起動時の最終判断は `signature.service` と `capability.service` が担当します。

## 検証ルール

次のルールを必須にします。

- パス文字列だけでパッケージを特定しない
- パストラバーサルを拒否する
- シンボリックリンク差し替えを考慮する
- 署名検証後に別ファイルへ差し替えられないようにする
- 未署名 manifest から Capability を付与しない
- 不明な Capability 名は黙って許可しない
- 重複登録は拒否する
- 1 つのバイナリが複数パッケージに属する場合は拒否する
- 失敗時は fail closed にする

## インストール先

標準の配置先は次の通りです。

- 実行ファイル: `/bin/<name>`
- パッケージ manifest: `/system/packages/<package>/manifest.toml`
- パッケージ索引: `/system/packages/index.*` または同等のキャッシュ

索引は高速化のためのキャッシュです。
信頼の根拠にはしません。

## 旧方式からの移行

廃止する旧方式は次の通りです。

- `/bin/<binary>.toml`
- binary ごとの個別 metadata
- driver/service ごとの分散した署名判定

今後は、パッケージ単位の manifest と `.mpkg` を正規の入力にします。
