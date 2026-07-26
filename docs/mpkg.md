# mochiOS Package Format

> **正本文書:** `.mpkg`のコンテナ形式、manifest、配置規則および検証手順はこの文書を正本とします。Root Certificate、Developer Certificate、証明書チェーンおよび失効確認の実装状況は[mochiOSの証明書と署名検証](certificates.md)を参照してください。

## 1. 概要

`.mpkg`は、mochiOSへアプリケーションまたはシステムコンポーネントを配布・インストールするためのパッケージ形式です。

* 署名済み`manifest.toml`
* Developer Certificate
* 証明書チェーン
* manifest署名
* インストール対象ファイル
* バイナリごとのCapability要求
* サービスやドライバーの定義

`.mpkg`は配布用コンテナです。インストール後に`.mpkg`ファイル自体を実行時の信頼根拠として使用しません。

## 2. パッケージ種別

v1では、次の2種類を定義します。

application
binary

`application`は、単一の`.app`バンドルを`applications`ディレクトリにインストールします。
`binary`は、CLI、ライブラリ、サービス、ドライバーなどをシステム内へ配置します。

サービスやドライバーは独立したパッケージ種別にしません。
サービスやドライバーであることは、manifest内の`[[service]]`または`[[driver]]`で表現します。

これにより、1つのbinaryパッケージへ複数のバイナリ、サービス、ライブラリを含められます。

## 3. コンテナ形式

`.mpkg`は次の構造を持ちます。

- MPKG header
- tar stream

tarストリームは圧縮できます。
v1では次の圧縮形式を定義します。

- 0 = 無圧縮
- 1 = Zstandard

通常のパッケージはZstandardを使用します。

## 4. MPKGヘッダー

ヘッダーは32バイト固定です。
すべての整数はリトルエンディアンです。

```text
Offset  Size  内容
0x00    4     Magic: "MPKG"
0x04    2     Major version
0x06    2     Minor version
0x08    2     Header size
0x0A    1     Compression
0x0B    1     Flags
0x0C    8     展開後tarストリームのサイズ
0x14    12    Reserved
```

v1の値は次のとおりです。

```text
Magic              = 4D 50 4B 47
Major version      = 1
Minor version      = 0
Header size        = 32
Compression        = 0 または 1
Flags              = 0
Reserved           = すべて0
```

未対応のmajor versionは拒否します。

未知のflagsが設定されている場合も拒否します。

展開後サイズは、展開爆弾への対策として使用します。実際の展開サイズが一致しない場合は拒否します。

## 5. tarストリームの制限

v1では、tarのすべての機能を許可しません。

使用できるエントリは次のとおりです。

- 通常ファイル
- ディレクトリ

次は禁止します。

- シンボリックリンク
- ハードリンク
- デバイスファイル
- FIFO
- ソケット
- sparse file
- GNU tar拡張
- PAX拡張

ディレクトリエントリは省略できます。package.serviceは、ファイルパスから必要なディレクトリを生成します。

tarヘッダー内の次の情報は信頼しません。

- uid
- gid
- uname
- gname
- mode
- mtime

実際のファイルモードは`manifest.toml`から取得します。

パスはUTF-8でなければなりません。

次のパスは拒否します。

- 絶対パス
- 空文字列
- "."または".."を含むパス
- 連続したスラッシュ
- 末尾がスラッシュの通常ファイル
- バックスラッシュ
- NUL文字
- Unicode正規化後に重複するパス
- 大文字小文字の正規化後に衝突するパス

同一パスのエントリが複数存在する場合は、後勝ちにせずパッケージ全体を拒否します。

## 6. コンテナ内部構造

すべてのmpkgは次の構成を持ちます。

```
manifest.toml
signatures/
├─ manifest.sig
├─ developer.cert
└─ chain/
   ├─ 000.cert
   ├─ 001.cert
   └─ ...
payload/
└─ ...
```

次のファイルは必須です。

- manifest.toml
- signatures/manifest.sig
- signatures/developer.cert

証明書チェーンが不要な場合、`signatures/chain/`は省略できます。

`manifest.toml`はコンテナ内に1つだけ存在しなければなりません。

## 7. 署名方式

mpkg v1では次のアルゴリズムを固定します。

- ダイジェスト: SHA-256
- 署名: Ed25519

アルゴリズムをmanifest内で指定する方式にはしません。

これにより、署名検証前の未検証manifestを見てアルゴリズムを選択する必要がなくなります。

`manifest.sig`は64バイトのEd25519署名です。

署名対象は次のバイト列です。

```text
"mochios-mpkg-manifest-v1\0"
+
SHA-256(manifest.tomlの正確なバイト列)
```

`manifest.toml`は解析後に再生成してはいけません。

署名検証には、コンテナ内に保存されている正確なバイト列を使用します。

## 8. manifest.tomlの形式

manifestはTOML形式とします。

基本構造は次のとおりです。

```toml
format = 1

[package]
id = "org.mochios.coreutils"
name = "coreutils"
version = "0.1.0"
revision = 1
vendor = "mochiOS Project"
kind = "binary"
architecture = "x86_64"
abi = "mochios-1"

[compatibility]
os = ">=26.0.0"
```

## 9. packageセクション

### package.id

パッケージの一意な識別子です。

逆ドメイン形式を使用します。

使用可能な文字は`a-z`、`0-9`、`.`、`-`です。

パッケージIDは大文字小文字を区別しません。manifestには小文字だけを使用します。

### package.name

表示および管理用のパッケージ名です。

一意性の判定には使用しません。

### package.version

ソフトウェアバージョンです。

```text
1.0.0
1.2.0-beta.1
```

### package.revision

同じversionに対するパッケージ再ビルド番号です。

```text
version = "1.0.0"
revision = 2
```

### package.kind

次のどちらかです。

```text
application
binary
```

### package.architecture

v1では次の値を使用します。

```text
x86_64
any
```

`any`は、アーキテクチャ非依存データだけを含むパッケージに使用します。

### package.abi

対象となるmochiOS ABIを示します。

```text
mochios-1
```

## 10. ファイル定義

パッケージに含まれるすべてのインストール対象ファイルを`[[file]]`へ記録します。

```toml
[[file]]
id = "ls"
path = "$/ls"
digest = "sha256:0123456789abcdef"
size = 123456
mode = "0755"
```

$は、インストールルートを示すプレースホルダです。

### file.id

manifest内で一意な識別子です。

`[[binary]]`、`[[service]]`などから参照します。

### file.path

インストールルートからの相対パスです。

絶対パスは使用しません。

`package.kind`によってインストールルートが変わります。

- binary: インストールルートは /bin/

- application: インストールルートは /applications/<bundle>.app/

### file.digest

次の形式を使用します。

```text
sha256:<16進数64文字>
```

### file.size

展開後のファイルサイズです。

圧縮後のサイズではありません。

### file.mode

インストール後のパーミッションです。

8進数文字列で指定します。

```text
0644
0755
```

setuid、setgid、sticky bitはv1では許可しません。

## 11. payload内のファイル配置

payload内のパスは、パッケージ種別によって固定します。

### binaryパッケージ

```text
payload/root/<file.path>
```

例:

```text
payload/root/bin/ls
payload/root/bin/cp
payload/root/bin/mv
```

### applicationパッケージ

```text
payload/bundle/<file.path>
```

例:

```text
payload/bundle/entry
payload/bundle/about.toml
payload/bundle/resources/icon.png
```

manifest内のすべての`[[file]]`に対応するpayloadエントリが必要です。

manifestに記載されていないpayloadファイルが存在する場合は拒否します。

payloadに存在しないファイルがmanifestへ記載されている場合も拒否します。

## 12. バイナリ定義

実行可能ファイルは`[[binary]]`で定義します。

```toml
[[binary]]
id = "ls"
file = "ls"
capability_profile = "coreutils.ls"
```

### binary.id

パッケージ内で一意なバイナリ識別子です。

### binary.file

`[[file]]`のidを参照します。

参照先ファイルは`0755`などの実行可能modeを持たなければなりません。

### binary.capability_profile

起動時に使用するCapability Profileです。

存在しないProfileを参照した場合は、パッケージ全体を拒否します。

## 13. applicationパッケージ

applicationパッケージには`[application]`が必要です。

```toml
[application]
bundle = "Example.app"
entry = "main"
about = "about.toml"
```

### application.bundle

インストールされるバンドルディレクトリ名です。

必ず`.app`で終わる必要があります。

```text
Example.app
Editor.app
Browser.app
```

スラッシュや`..`は使用できません。

### application.entry

起動時に使用する`[[binary]].id`です。

### application.about

表示用メタデータのパスです。

バンドルルートからの相対パスです。

通常は次の値を使用します。

```text
about.toml
```

`about.toml`も`[[file]]`へ登録し、ダイジェスト検証対象に含めます。

ただし、Capability付与や署名者判定の根拠には使用しません。

## 14. applicationパッケージの例

コンテナ内部:

```text
Example-1.0.0.mpkg
├─ manifest.toml
├─ signatures/
│  ├─ manifest.sig
│  ├─ developer.cert
│  └─ chain/
│     └─ 000.cert
└─ payload/
   └─ bundle/
      ├─ entry
      ├─ about.toml
      └─ resources/
         └─ icon.png
```

manifest:

```toml
format = 1

[package]
id = "com.example.application"
name = "Example"
version = "1.0.0"
revision = 1
vendor = "Example Developer"
kind = "application"
architecture = "x86_64"
abi = "mochios-1"

[compatibility]
os = ">=26.0.0"

[application]
bundle = "Example.app"
entry = "main"
about = "about.toml"

[[file]]
id = "entry"
path = "entry"
digest = "sha256:0123456789abcdef"
size = 123456
mode = "0755"

[[file]]
id = "about"
path = "about.toml"
digest = "sha256:0123456789abcdef"
size = 456
mode = "0644"

[[file]]
id = "icon"
path = "resources/icon.png"
digest = "sha256:0123456789abcdef"
size = 12345
mode = "0644"

[[binary]]
id = "main"
file = "entry"
capability_profile = "application.main"

[[capability_profiles."application.main".request]]
name = "process.basic"
source = "binary"
required = true

[[capability_profiles."application.main".request]]
name = "window.create"
source = "binary"
required = true

[[capability_profiles."application.main".request]]
name = "filebinary.read"
source = "caller"
scope = "selected-files"
required = false
```

インストール後:

```text
/applications/
└─ Example.app/
   ├─ manifest.toml
   ├─ about.toml
   ├─ entry
   ├─ resources/
   │  └─ icon.png
   └─ signatures/
      ├─ manifest.sig
      ├─ developer.cert
      └─ chain/
         └─ 000.cert
```

コンテナ内の`manifest.toml`は、バイト列を変更せずに次へ配置します。

```text
/applications/Example.app/manifest.toml
```

## 15. binaryパッケージ

binaryパッケージには、`[application]`を含めません。

coreutilsの例:

```text
coreutils-0.1.0.mpkg
├─ manifest.toml
├─ signatures/
│  ├─ manifest.sig
│  ├─ developer.cert
│  └─ chain/
└─ payload/
   └─ root/
      └─ bin/
         ├─ ls
         ├─ cp
         └─ mv
```

manifest:

```toml
format = 1

[package]
id = "org.mochios.coreutils"
name = "coreutils"
version = "0.1.0"
revision = 1
vendor = "mochiOS Project"
kind = "binary"
architecture = "x86_64"
abi = "mochios-1"

[compatibility]
os = ">=26.0.0"

[[file]]
id = "ls"
path = "bin/ls"
digest = "sha256:0123456789abcdef"
size = 123456
mode = "0755"

[[file]]
id = "cp"
path = "bin/cp"
digest = "sha256:0123456789abcdef"
size = 123456
mode = "0755"

[[file]]
id = "mv"
path = "bin/mv"
digest = "sha256:0123456789abcdef"
size = 123456
mode = "0755"

[[binary]]
id = "ls"
file = "ls"
capability_profile = "coreutils.ls"

[[binary]]
id = "cp"
file = "cp"
capability_profile = "coreutils.cp"

[[binary]]
id = "mv"
file = "mv"
capability_profile = "coreutils.mv"
```

インストール後:

```text
/bin/
├─ ls
├─ cp
└─ mv

/binary/packages/
└─ org.mochios.coreutils/
   ├─ manifest.toml
   └─ signatures/
      ├─ manifest.sig
      ├─ developer.cert
      └─ chain/
```

binaryパッケージのmanifestは次へ配置します。

```text
/binary/packages/<package.id>/manifest.toml
```

## 16. service定義

binaryパッケージはサービスを定義できます。

```toml
[[service]]
id = "org.mochios.input"
binary = "input"
startup = "binary"
restart = "on-failure"
after = ["org.mochios.drivers"]
requires = ["org.mochios.logger"]
provides = ["org.mochios.input"]
```

`binary`は`[[binary]].id`を参照します。

`startup`は次の値を使用します。

```text
binary
manual
on-demand
```

`restart`は次の値を使用します。

```text
never
on-failure
always
```

サービス定義はservice-manager.serviceが使用します。

manifestにサービス定義が存在することだけを理由に、そのサービスを信頼してはいけません。署名検証とCapability判定を通過する必要があります。

## 17. driver定義

ドライバーパッケージは`[[driver]]`を使用します。

```toml
[[driver]]
id = "org.mochios.driver.i8042"
binary = "i8042"
class = "input"
matches = [
    "platform:i8042"
]
```

ドライバーの起動はdrivers.serviceが要求しますが、実際の署名検証とCapability判定は通常のlaunch.service経路を使用します。

## 18. Capability Profile

Capability要求はmanifestへ含めます。

```toml
[[capability_profiles."coreutils.ls".request]]
name = "stdio"
source = "caller"
required = true

[[capability_profiles."coreutils.ls".request]]
name = "filebinary.enumerate"
source = "caller"
scope = "launch-paths"
required = true
```

`source`は次のいずれかです。

```text
caller
binary
service
```

manifestに記載されているCapabilityを無条件に付与してはいけません。

実際の付与結果は次で決定します。

```text
Requested
∩ CertificateAllowed
∩ binaryPolicy
∩ Delegatable
```

不明なCapability名は拒否します。

## 19. 依存関係

パッケージ依存関係は`[[dependency]]`で定義します。

```toml
[[dependency]]
package = "org.mochios.libc"
version = ">=0.1.0"
required = true
```

v1では、依存関係はパッケージIDとバージョンだけで判定します。

自動的な外部リポジトリ検索やダウンロードは、mpkg形式自体には含めません。

## 20. インストール可能なパス

applicationパッケージは、`.app`バンドル外へファイルを配置できません。

binaryパッケージは、v1では次の領域だけへファイルを配置できます。

```text
/bin/
/libraries/
/binary/services/
/binary/resources/
```

次の領域へpayloadから直接配置することは禁止します。

```text
/binary/packages/
/applications/
/var/
/run/
/mnt/
/boot/
```

`/binary/packages/<package.id>/`はpackage-installer.serviceが作成し、manifestと署名情報だけを配置します。

`/var`や`/run`に必要なディレクトリは、将来的に宣言的なruntime directory定義で作成します。

パッケージpayloadへ可変データを含めてはいけません。

## 21. インストール処理

インストールは次の順序で行います。

```text
MPKGヘッダーを検証
    ↓
サイズ上限を検証
    ↓
tarストリームを展開
    ↓
エントリ名と重複を検証
    ↓
manifest署名を検証
    ↓
Developer Certificateを検証
    ↓
証明書チェーンと失効状態を検証
    ↓
manifestスキーマを検証
    ↓
payloadとfile定義を照合
    ↓
全ファイルのサイズとダイジェストを検証
    ↓
既存パッケージとの所有権衝突を検証
    ↓
ステージング領域へ展開
    ↓
最終配置へアトミックに切り替え
    ↓
manifestと署名情報を配置
    ↓
パッケージインデックスを更新
```

検証前に、payloadを最終配置先へ書き込んではいけません。

## 22. ファイル所有権

1つの通常ファイルを複数のパッケージが所有してはいけません。

既に別のパッケージが所有するパスへインストールしようとした場合は拒否します。

ディレクトリは複数パッケージで共有できます。

パッケージの所有ファイル一覧は、署名済みmanifestを基準とします。

インストール状態や高速検索用データは、次のような派生データとして管理します。

```text
/var/lib/mpkg/
```

このデータベースは信頼の根拠にはしません。

## 23. 更新

更新パッケージは、既存パッケージと同じ`package.id`を持たなければなりません。

次を確認します。

```text
新しいversionまたはrevisionである
証明書が有効である
証明書がパッケージIDを署名可能である
すべての新しいファイルが検証済みである
他パッケージのファイルと衝突しない
```

更新はステージング領域を使用し、途中失敗時に旧バージョンを保持します。

## 24. 削除

削除時は、インストール済みの署名済みmanifestへ記載されているファイルだけを削除します。

アプリケーションでは、対象`.app`バンドル全体を削除できます。

ただし、アプリケーションの可変データは別領域に保存します。

```text
/var/appdata/<package.id>/
```

通常のアンインストールでは、このデータを自動削除しません。

## 25. v1で禁止する機能

v1では次を実装しません。

```text
post-installスクリプト
pre-installスクリプト
uninstallスクリプト
任意コードによるインストール処理
シンボリックリンク
ハードリンク
setuid
setgid
複数アプリケーションを含む単一mpkg
applicationとbinaryの混在
manifest未登録ファイル
署名されていない開発者パッケージ
```

必要なインストール処理は、将来的に宣言的なmanifestフィールドとして追加します。

## 26. MIMEタイプと命名

MIMEタイプは次とします。

```text
application/vnd.mochios.mpkg
```

推奨ファイル名は次です。

```text
<name>-<version>-<architecture>.mpkg
```

例:

```text
coreutils-0.1.0-x86_64.mpkg
Example-1.0.0-x86_64.mpkg
fonts-1.0.0-any.mpkg
```

ファイル名は識別や信頼の根拠には使用しません。

パッケージの正式な識別には、manifest内の`package.id`を使用します。
