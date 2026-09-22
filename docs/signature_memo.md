# mochiOSの署名方式の整理

## 今の問題点

### 1. 署名の仕組みがMPKG中心になっている

現在のmochiOSでは、主にMPKGを単位として署名・検証しています。

```text
manifest.toml
+ payload hash
+ Developer Certificate
+ manifest signature
```

という構成なので、配布パッケージとしては問題ありません。

しかし、mochiOS上で普通に

```bash
gcc main.c -o app
```

のようにコンパイルした場合、生成されたELFをそのまま自然に実行するための署名経路がありません。

つまり、配布用の署名方式は存在する一方で、ローカル開発用の署名方式が不足しています。

### 2. `ld` と署名処理が完全に分離している

現在はリンカがELFを生成して終了します。

そのため、署名が必要ならリンク後に別途 `msign` などを実行する必要があります。

これでは、

```bash
gcc main.c -o app
./app
```

という普通の開発体験になりません。

Rust、C、C++、Zigなどの言語ごとに署名処理を組み込むことになると、toolchain側の実装も重複します。

### 3. ELF単体では署名状態が分からない

現在の署名情報はmanifestやInstall Recordなど、実行ファイルの外側に強く依存しています。

そのためELFだけを見ても、

* 署名済みなのか
* 開発用ビルドなのか
* どの鍵で署名されたのか

といった情報を判断できません。

### 4. `Development` provenanceとビルド工程がつながっていない

mochiOSにはすでに、

```text
BuiltIn
VerifiedPackage
Development
```

というprovenanceがあります。

しかし、ローカルでコンパイルしたバイナリを自動的に `Development` として扱う仕組みがtoolchainに統合されていません。

そのため、OS側の設計と開発ツール側の設計がまだ接続されていない状態です。

## だからこうする

署名を、

```text
開発時署名
配布時署名
```

の2種類に分けます。

## 開発時署名

mochiOS上でリンクしたELFには、自動的にDevelopment署名を付けます。

```text
gcc / clang / rustc / zig
        ↓
       ld
        ↓
      ELF
        ↓
Development署名
        ↓
そのまま実行可能
```

実装としては、`lld` 本体を大きく改造するのではなく、mochiOS用の `ld` wrapperを用意します。

```text
/usr/bin/ld
    ↓
real lld
    ↓
リンク成功
    ↓
Development署名
```

これによって、どの言語を使っていても最終的に同じ署名処理を通せます。

### ELF内に署名情報を持たせる

Development署名はELFの独自sectionとして格納します。

例えば、

```text
.mochios.signature
.mochios.identity
```

のようなsectionを用意します。

最低限、次の情報を保持します。

```text
signature format version
binary hash
signing key ID
signature
```

これによりELF単体でも、Development署名されたバイナリであることを確認できます。

### 開発用鍵を使う

Development署名には正式なDeveloper Certificateとは別の、ローカル開発用鍵を使用します。

この鍵はOSまたは開発環境側で管理します。

```text
Local Development Key
        ↓
ELFを署名
        ↓
Development provenance
```

正式な配布用identityとは分離します。

### CapabilityはDevelopment署名に含めない

Development署名は、

```text
このバイナリがローカル開発環境で生成されたこと
リンク後に改変されていないこと
```

を証明する用途に限定します。

Capabilityは現在のmanifestやPolicy、ユーザー許可によって決定します。

つまり、

```text
Development署名
≠
Capability付与
```

とします。

## 配布時署名

正式に配布するアプリケーションについては、現在のMPKG方式を維持します。

```text
ELF
 ↓
manifest.toml
 ↓
MPKG
 ↓
Developer Certificate
 ↓
msign package sign
 ↓
VerifiedPackage
```

こちらでは、

* Developer Identity
* Package ID
* Capability許可範囲
* payload hash
* Developer Certificate

などを含めて正式に検証します。

## 最終的な構成

```text
ローカル開発

source
 ↓
compiler
 ↓
ld wrapper
 ↓
ELF
 ↓
Development署名
 ↓
Development provenance
 ↓
実行


正式配布

source
 ↓
compiler
 ↓
ld
 ↓
ELF
 ↓
MPKG生成
 ↓
Developer Certificateで署名
 ↓
VerifiedPackage provenance
 ↓
インストール・実行
```

これにより、

```bash
gcc hello.c -o hello
./hello
```

のような普通の開発体験を維持しながら、正式配布時には現在のDeveloper CertificateとCapabilityモデルをそのまま利用できます。

つまり、

```text
ELF
+ Development署名
+ MPKG正式署名
```

という二段構成にします。
