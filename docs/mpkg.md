# mochiOS Package Format

この文書を `.mpkg` のコンテナ形式、manifest、署名、配置規則の正本とします。
証明書の wire format、信頼境界、失効および Root ローテーションは
[mochiOS の証明書と署名検証](certificates.md)を参照してください。

## 1. 現行バージョン

現行実装は MPKG v1 です。コンテナは 32 bytes の MPKG header と、無圧縮の
ustar stream で構成します。Zstandard を示す compression 値 `1` は header の
予約値として解析できますが、`signature.service` と `package.service` は
`ENOTSUP` で拒否します。

## 2. MPKG header

整数はすべて little-endian です。

```text
Offset  Size  内容
0x00    4     magic: "MPKG"
0x04    2     major version: 1
0x06    2     minor version: 0
0x08    2     header size: 32
0x0a    1     compression: 0
0x0b    1     flags: 0
0x0c    8     tar stream の byte 数
0x14    12    reserved: すべて 0
```

magic、version、header size、flags、reserved、実際の tar stream 長が一致しない
パッケージは拒否します。

## 3. コンテナ構造

```text
manifest.toml
signatures/
|-- manifest.sig
`-- developer.cert
payload/
|-- root/
`-- bundle/
```

必須ファイルは次の 3 個です。

- `manifest.toml`
- `signatures/manifest.sig`
- `signatures/developer.cert`

v1 は Root から Developer Certificate への 1 段だけを扱います。
`signatures/chain/`、および上記以外の `signatures/` 内エントリは拒否します。

## 4. ustar 制約

現行 parser が受理する typeflag は通常ファイル (`0` または NUL) と
ディレクトリ (`5`) だけです。symlink、hard link、device、FIFO、PAX、GNU 拡張は
拒否します。

各 path は UTF-8 の相対 path とし、次を拒否します。

- 空 path、絶対 path、末尾 `/`
- `.` または `..` segment
- `//`、backslash、NUL
- 同じ byte 列の path の重複
- `manifest.toml`、`signatures/`、`payload/` 以外の top-level entry

現行 parser は Unicode normalization や case folding を行いません。したがって
package producer は path に ASCII を使うべきです。

tar の uid、gid、uname、gname、mtime はインストール属性に使用しません。
mode は manifest の `[[file]].mode` を使います。

## 5. manifest.toml

現行 parser がインストールと Capability 解決に使用する基本形式は次です。

```toml
format = 1

[package]
id = "org.example.tool"
name = "example-tool"
version = "1.0.0"
vendor = "Example Developer"
kind = "binary"
architecture = "x86_64"
abi = "mochios-1"

[[file]]
id = "main"
path = "$/example-tool"
digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
size = 123456
mode = "0755"

[[binary]]
path = "/bin/example-tool"
file = "main"
kind = "application"
requires = ["process.basic"]
```

`package.id`、`package.name`、`package.version` は必須です。`package.id` は小文字の
逆 domain 形式を使用します。`package.kind` は `binary` または `application` です。
kind がない古い manifest は binary として扱います。

`[[file]]` は少なくとも 1 個必要です。各 entry の `path`、`size`、完全な
SHA-256 digest、mode と payload を照合します。manifest にない payload 通常
ファイル、および payload がない `[[file]]` は拒否します。mode は `0o000` から
`0o777` だけを許可し、setuid、setgid、sticky bit は拒否します。

`[[binary]].requires` はその binary の必須 Capability 一覧です。現行 v1 には
optional request の表現はありません。1 個でも Developer Certificate の
`allowed_capabilities` または Binary Policy を超える場合、起動時の Capability
解決全体を拒否します。

## 6. payload path と配置先

`[[file]].path` には、明示的な絶対配置先または `$/` 形式を使用します。

### binary

```text
path = "$/tool"
container: payload/root/bin/tool
install:   /bin/tool
```

```text
path = "/system/services/example.service"
container: payload/root/system/services/example.service
install:   /system/services/example.service
```

binary package の配置先は次の prefix に限定します。

- `/bin/`
- `/libraries/`
- `/binary/services/`
- `/binary/resources/`
- `/system/services/`

### application

```text
[package]
name = "Example"
kind = "application"

[[file]]
path = "$/entry.elf"
```

この場合の container path は `payload/bundle/entry.elf`、配置先は
`/applications/Example.app/entry.elf` です。application package は
`/applications/` の外へ配置できません。生成後の絶対 path についても空 segment、
`.`、`..`、backslash を拒否します。

## 7. 署名

ダイジェストは SHA-256、署名は Ed25519 に固定します。
`manifest.sig` は次の byte 列に対する 64 bytes の署名です。

```text
"mochios-mpkg-manifest-v1\0"
|| SHA-256(manifest.toml の正確な byte 列)
```

`developer.cert` は `MCER` v1 の Developer Certificate です。検証順は次です。

1. MPKG header、ustar、Developer Certificate の構文を検証
2. 埋め込み Offline Root で同期済み Trust Snapshot を検証
3. Trust 内の active/retired Issuer で Developer Certificate を検証
4. Snapshot と Certificate の validity、key usage、Package ID scope、失効 serial を検証
5. Developer 公開鍵で `manifest.sig` を検証
6. manifest と全 payload の size、SHA-256 を照合

`package.service` は読み込んだ同一の MPKG byte 列を chunk protocol で
`signature.service` へ渡します。署名検証後に path を開き直しません。

## 8. インストールと有効化

署名、manifest、全 payload、配置先、mode、既存 path との衝突を、書き込み開始前に
検証します。現行 ext2 CExt には rename transaction がないため、v1 installer は
既存 package の更新と既存 file の上書きを `EEXIST` で拒否します。

新規インストールは次の順です。

1. payload を配置
2. `/system/packages/<package-id>/verification.bin` を配置
3. 検証済みの `manifest.toml` を最後に配置

manifest を activation marker とし、Capability resolver は manifest digest と
`verification.bin` を再照合します。各ファイルは `O_CREAT | O_EXCL` で新規作成し、
途中で失敗した場合は、その要求で作成済みのpayloadとverification recordを逆順に
削除します。空の親directoryが残る場合はありますが、manifestが存在しないため
有効なインストールとして扱いません。

## 9. verification.bin

`verification.bin` は `mochios-signature-protocol` の `VERIFIED` message を
`request_id = 0` で encode したものです。次を保持します。

- developer ID
- certificate serial
- subject key ID
- verified package ID
- certificate が許可した Capability 一覧
- manifest digest
- package digest

これは package payload の代替署名ではなく、`signature.service` の検証結果を
`capability.service` へ引き渡す内部 record です。

## 10. 開発ツール

workspace に登録された `tools/devkit` の `msign` を使用します。

```text
msign key generate
msign certificate issue
msign certificate inspect
msign package sign
msign package verify
```

従来の Kome 用 `msign keygen`、`msign sign`、`msign verify` は互換性のため残して
あります。開発 fixture は `tools/devkit/fixtures/development/` にあり、製品用 Root
秘密鍵は repository や通常 build 環境へ置きません。

サンプルMPKG生成器は一時鍵を生成しません。署名前のコンテナだけを決定的に作り、
`msign package sign`が開発用Developer Certificateと対応する鍵で署名します。

## 11. 未実装事項

- Zstandard 展開
- 既存 package の atomic upgrade、uninstall
- MPKG 内に埋め込む任意の証明書 chain
- OCSP
- Unicode normalization と case-fold collision 検査
- optional Capability request

これらは現行 v1 の成功条件として扱いません。
