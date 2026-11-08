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

### Native共有ライブラリの依存宣言

別packageのnative libraryを承認する場合、利用側の署名済みmanifestに次を記録します。

```toml
[[library_dependency]]
package = "org.example.runtime"
developer = "org.example.developer"
subject_key_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
provenance = "verified-package"
abi = "example-runtime-1"
```

5項目すべて必須です。`subject_key_id`はinstall recordの署名鍵IDと同じ32 bytesを64桁の
hexで表し、`sha256:` prefixは付けません。`provenance`は`built-in`、`verified-package`、
`development`のいずれかです。built-inの場合もdeveloper / key / provenanceを照合します。
未知の項目、同じ項目の再指定、同一packageの依存宣言の重複、不完全な宣言は拒否します。

この宣言は依存先packageが公開する`kind = "library"`のbinary群と、そのpackageが明示した
library依存を承認します。通常のdata file、applicationやserviceのbinaryをlibraryとして
承認するものではありません。各libraryには、そのpackage自身の`[[file]]`への`file` IDと
一致するインストール先が必要です。利用側は依存先の`[[binary]]`や`[[file]]`を複写しません。
一つの共有packageを複数アプリから参照でき、同じpathを複数packageが所有する登録を避けられます。

依存先はinstall recordと同一のmanifest snapshotを認証してから使います。宣言したpackage ID、
developer、鍵ID、provenance、ABIの一致と、利用側がarchitectureを指定した場合はその一致を
確認します。providerのversion更新後もこれらが一致すれば、利用側のmanifestを変更せずに
更新後の認証済みfile digestを使えます。ABIや鍵の変更には利用側の承認の更新が必要です。
同一packageを一回の起動中に再読込せず、循環・共有依存も同じsnapshotを使います。
別の経路から同じpackageに異なる制約が指定された場合も、その制約を省略せず照合します。

実際のlibraryはcallerの読み取りscopeから取得し、捕捉済みbyte列を当該packageのsize / digestと
比較します。別packageのCapabilityや実行identityを利用側へ加算しません。依存先を自動で
download / installする機能はこの宣言には含めず、未installなら起動時に`ENOENT`を返します。
認証・ABI・内容の不一致は`EACCES`、export pathの所有の重複は`EEXIST`で拒否します。
認可graphはrootを含む256 package、8192 libraryを上限とし、超過は`E2BIG`を返します。
これは認可するmetadataの上限で、実際にlinkするELF scopeの制約とは別です。

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

### Linux application

Linux applicationも`kind = "application"`として格納し、ABIを
`mboot-linux-1`に固定します。実行可能ファイルをmochiOSから直接起動せず、署名済み
payloadに含まれるread-only SquashFSを`linux.service`がmBootへ転送します。

```toml
[package]
id = "org.example.editor"
name = "Editor"
version = "1.0.0"
vendor = "Example Developer"
kind = "application"
architecture = "x86_64"
abi = "mboot-linux-1"

[linux]
entrypoint = "/usr/bin/editor"
rootfs_file = "linux-rootfs"
writable_paths = ["/usr/share/editor", "/var/lib/editor"]
portal_read_paths = ["/applications", "/home/$USER/Develop"]
portal_write_paths = ["/home/$USER/Develop"]

[[file]]
id = "linux-rootfs"
path = "$/rootfs.squashfs"
digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
size = 12345678
mode = "0644"
```

`entrypoint`と`writable_paths`は正規化済み絶対pathだけを許可します。`/bin`、
`/sbin`、`/lib*`、`/usr/bin`、`/usr/lib`、`/dev`、`/proc`、`/sys`、`/run`、
`/tmp`、`/home`、`/mochios`は永続書き込み対象にできません。重複または親子関係に
ある`writable_paths`も拒否します。

mBootはrootfsを`/bin/mboot/<bundle-id>/rootfs.squashfs`へdigest付きで一度だけ
stageし、bundleごとのmount/PID/IPC/UTS/network namespaceと専用Xvfbで起動します。
processはUID/GID 65534へ降格し、rootfsとOverlayFSは`nosuid`でmountします。
`/home/user`と宣言済みpathだけがOverlayFSとなり、upper/work directoryは
`/libraries/users/<USER>/mboot/<bundle-id>/`に保存されます。その他のrootfsは
read-only、`/tmp`はprocess終了時に破棄されるtmpfsです。

package内に置かれた`/mochios`は信頼せず、空のread-only tmpfsで必ず隠してから、
許可済みdirectoryだけを同じ絶対pathの`/mochios`以下へ個別にmountします。
`portal_read_paths`と`portal_write_paths`は、`/mochios`以下へ公開するmochiOS側の
directoryを宣言します。`$USER`はログイン済みuser名にだけ置換されます。readは
`/applications`、`/libraries`、`/home/$USER`以下、writeは`/home/$USER`以下だけを
宣言できます。

service-managerによるsession/sender/path検証と、`secure-ui.service`の一回限りの
directory grant画面を通過したread-only directoryは、`linux.service`がsymlinkと
特殊fileを拒否しながらmBootのinstance専用stagingへ転送します。mBoot側でもgrant
rootから外れるentryを拒否し、転送完了後にそのdirectoryだけをread-only bind mount
します。未許可または同期未完了のhost filesystemをbind mountすることはありません。

write grantは`/home/$USER`の直下ではなく、その配下のdirectoryだけに限定します。
Linux process終了後、mBootはgrant directoryの完全なsnapshotを`linux.service`へ返し、
mochiOS側は同じ親directory内の一時treeへ全entryを復元します。一時treeが完成した後だけ
backup付きrenameで元directoryと置換するため、途中失敗時は元directoryを維持して
write-backを再試行します。fileの追加・更新だけでなく、削除とrenameもsnapshot差分として
反映されます。symlink、特殊file、grant外path、上限超過は両側で拒否します。

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

`package.service` は呼出元が添付した読み取り専用FileHandleから、root所有・0700の
`/var/lib/package-transactions`配下へMPKGを固定します。署名検証と展開はこの専用コピーを
使用し、呼出元のパスを開き直しません。保存先と親directoryのroot所有・mode・symlinkを
検査し、承認後にもMPKG全体のSHA-256を再照合します。既存packageの管理directoryと
manifest／verification recordにもroot所有・他ユーザーによる書込不可の検査を行います。

## 8. インストールと有効化

署名、manifest、全 payload、配置先、mode、既存 path との衝突を、書き込み開始前に
検証します。`/system/packages` は image に含まれる built-in package 専用で、Store
などから追加した package の管理情報は Data root の `/var/lib/packages` に置きます。
同じ Package ID の built-in package は追加・更新・削除できません。

AppStoreと`mpk install`／`mpk update`／`mpk remove`は、共通の
`mochios-package-client`から次の手順を使います。

1. 認証済みbrokerからpackage.serviceのSEND専用endpointを取得し、入力ファイルを明示的に添付します。
2. package.serviceがMPKGを固定・検証し、操作・Package ID・version・publisher・MPKGのhash・既存recordのhashを持つnative transaction objectを返します。
3. capability.serviceがそのobjectから内容を取得し、共通の管理者認証へ渡します。GUIはsecure-uiのウィンドウ、mshはTerminalの保護された入力欄を使います。AppStoreは選択したreleaseのID・version・SHA-256との一致も要求します。
4. 変更直前にpackage.serviceが同じobjectの承認をbrokerで一度だけ消費します。送信元Process/image・UID・ログインセッション・policy generation・期限を再検査します。
5. 既存recordとMPKGのhashを再照合してから、以下の配置・更新・削除を行います。

承認は別のpackageや別processへ使い回せません。旧式・versionedのパスだけを渡す変更RPCは
拒否し、UID 0や`package.install`等のCapabilityだけでは変更を許可しません。
取消・失敗後の自動再試行は行いません。transactionには件数・容量・期限の上限があり、
最終参照解放・送信元終了・期限切れで専用コピーを回収します。読み取り専用の一覧取得は認証不要です。

新規インストールは次の順です。

1. payload を配置
2. `/var/lib/packages/<package-id>/verification.bin` を配置
3. 検証済みの `manifest.toml` を最後に配置

manifest を activation marker とし、Capability resolver は manifest digest と
`verification.bin` を再照合します。各ファイルは `O_CREAT | O_EXCL` で新規作成し、
途中で失敗した場合は、その要求で作成済みのpayloadとverification recordを逆順に
削除します。空の親directoryが残る場合はありますが、manifestが存在しないため
有効なインストールとして扱いません。

更新は既存 `verification.bin` の Package ID、Developer ID、subject key ID が新しい
package と一致する場合だけ許可します。全新規 payload と管理 record を
`.mochi-new` へ検証済みで staging し、既存 file を `.mochi-old` へ退避してから
rename で切り替えます。途中または Capability index 更新に失敗した場合は逆順に
旧 file を復元します。現行 v1 では配置先一覧が変わる更新と Linux application の
更新を拒否します。

削除は `/var/lib/packages` に有効な record があり、built-in ではない package だけを
対象にします。全対象を `.mochi-remove` へ rename し、Capability index 更新後に
退避 file を削除します。通知失敗時は元へ戻します。現行 v1 では Linux application
の削除を拒否します。

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

Linux application用の未署名containerは`mpack linux`で生成できます。APT package、
単体Linux binary、既存rootfs directoryのいずれか一つをsourceにします。

```sh
mpack linux \
  --bundle-id org.example.editor \
  --name Editor \
  --version 1.0.0 \
  --vendor 'Example Developer' \
  --apt-package editor \
  --entrypoint /usr/bin/editor \
  --writable-path /usr/share/editor \
  --writable-path /var/lib/editor \
  --icon appicon.png \
  --output Editor.mpkg
```

APT形式ではhostの`apt-get`と`dpkg-deb`、全形式で`mksquashfs`が必要です。生成物は
通常のMPKGと同様にDeveloper Certificateで署名してからインストールします。

## 11. 未実装事項

- Zstandard 展開
- 配置先一覧を変更する upgrade と Linux application の upgrade、uninstall
- MPKG 内に埋め込む任意の証明書 chain
- OCSP
- Unicode normalization と case-fold collision 検査
- optional Capability request
- Linux applicationのnetwork、clipboard、device Capability portal

これらは現行 v1 の成功条件として扱いません。
