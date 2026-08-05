<div align="center">
    <h1>mochiOS</h1>
    <h5>This is an OS. Do not eat.</h5>
</div>

---

mochiOSは、「自由で、安心して使えるコンピューター」を目指して開発しているオペレーティングシステムです。
普段使っているWindowsやmacOS、Linuxなどと同じように、コンピューターを動かすための土台となるソフトウェアを完全に一から作っています。

このプロジェクトは主に中学生によって開発されています。

### About

現在のコンピューターには、たくさんのアプリが存在します。
しかし、インストールしたアプリが本当に安全なのか、裏側で何をしているのか、利用者からは分かりにくいことがあります。

mochiOSでは、アプリが利用できる機能を必要な範囲だけに制限し、問題が起きた場合でも影響を広げにくくすることを目指しています。
安全性だけでなく、使いやすさや見た目の美しさ、開発者が自由にものを作れる環境も大切にしています。

### What we aim for

mochiOSでは、次のようなOSを目指しています。

- 安心してアプリを利用できる
- アプリが必要以上の権限を持たない
- 問題が発生しても、ほかの部分へ影響しにくい
- シンプルで分かりやすい
- 美しく、一貫性のあるUI
- 知らない人でも簡単に使える
- 誰でも自由にアプリや機能を開発できる
- OSの仕組みを学び、試せる

### why "mochiOS"?

mochiOSという名前は、日本のおもちから付けられています。
やわらかく、親しみやすく、いろいろな形になれるOSを目指しています。
ただし、食べることはできません。（というかどうやっても食べられません）

### Building and Running

mochiOSは複数のリポジトリに分かれて開発されており、それらの管理に[repo](https://gerrit.googlesource.com/git-repo)
を使用しています。
まずはrepoとビルドに必要なツールをインストールしてください。

主な依存ツールは次のとおりです。

```text
git
repo
make
perl
rustup
cargo
e2fsprogs
fakeroot
libncurses-dev
```

1. このリポジトリをクローンします。

```bash
git clone https://github.com/mochiOS/mochiOS.git
```

2. repoを初期化します。

```bash
cd mochiOS
repo init \
    -u https://github.com/mochiOS/mochiOS.git \
    -b master
    
repo sync
```

3. mochiOSが固定しているRust toolchainをインストールします。

```bash
rustup toolchain install nightly-2026-05-14 \
    --component rust-src \
    --component llvm-tools-preview
rustup toolchain install "$(cat build/rust-std-toolchain)" \
    --component rust-src
```

ユーザーランドの標準ライブラリは`libraries/rust`にあるmochiOSのRust forkからビルドします。
このソースはrustcの内部APIを使用するため、任意のnightlyとは組み合わせられません。
対応するtoolchainは`build/rust-std-toolchain`で固定されており、`make build`も必ずこの値を使用します。

4. mwsをインストールします。

```bash
make install
```

5. newlibをconfigureします。

```bash
cd libraries/newlib
./configure
cd ../..
```

6. mochiOSをビルドします。

```bash
make build
```

`make build`はビルド前に`make olddefconfig`を実行します。以前のワークスペースに
`USER_RUST_STD_TOOLCHAIN`が残っている場合、その設定は自動的に削除され、
`build/rust-std-toolchain`の固定値へ移行します。

物理PC向けのmBootイメージをビルドする場合は、次を実行します。

```bash
make mboot
```

このターゲットはmochiOSをビルド（キャッシュ済みのものがあればそれを使用します）し、その`out/artifacts/disk.img`を内包したmBootを
Buildrootの既存outputを再利用してビルドします。初回のみBuildrootの設定と依存物の
構築が必要です。

mBoot自体をQEMUで起動し、その中でmochiOSを全画面実行する場合は次を使用します。

```bash
make run-boot
```

利用可能な環境では外側のQEMUにKVMを使用し、keyboardとmouseはmBootを経由して
内側のmochiOSへ配送されます。

配布用イメージは次で生成します。（キャッシュを参照しないので時間がかかります）

```bash
make release
```

`out/releases/mochiOS.img`はmBootとmochiOSを一つにまとめたraw GPTイメージです。
USBメモリまたは専用ディスク全体へ書き込んで使用します。

次のような`rustc_comptime`、`offload_kernel`、または予約済み`rustc`属性のエラーが出た場合は、
Rust forkとcompilerのversionが一致していません。

```bash
repo sync
rustup toolchain install "$(cat build/rust-std-toolchain)" \
    --component rust-src
make olddefconfig
make build
```

`nightly`や別の日付のtoolchainを指定して回避しないでください。標準ライブラリsourceと
compilerを別versionにすると、coreのビルド時にcompiler組み込みmacroや内部属性を認識できません。

### Status

mochiOSは現在開発中です。
まだ一般的なパソコンのOSとして日常的に利用できる段階ではありません。
予期しない不具合が発生したり、データが失われたりする可能性があります。

現在は、OSの基本部分や、アプリを安全に動かすための仕組み、画面を表示するための機能などを開発しています。

### Projects

mochiOSは、複数のプロジェクトから構成されています。

- mnu  
  mochiOSの中心となるカーネルです。@tas0dev によって開発されています

- ViewKit  
  mochiOSのアプリ画面を作るための仕組みです。@tas0dev と @098orin によって開発されています。

- Kome
  mochiOS向けのアプリを開発するための言語です。初心者でも簡単にアプリを作れることを目指しています。
  @098orin と @tas0dev によって開発されています。

これらのプロジェクトはすべてオープンソースで開発されており、GitHub上で公開されています。

### Contributing

mochiOSはオープンソースで開発されています。
つまり、誰でも自由にソースコードを読んだり、仕組みを調べたり、改善案を提案したりできます。
OS開発に詳しくなくても、コードをかけなくてもデザイン、文章、アイデア、不具合報告など、さまざまな形で開発に参加できます。

ぜひ開発に参加してみたい！という方は[contributing.md](./contributing.md)を読んでみてください！
私達はいつでも新しい仲間を歓迎しています。

### Notice

mochiOSは現状実験的なソフトウェアです。
重要なデータが保存されているパソコンへ直接インストールすることは推奨していません。試す場合は、仮想マシンや専用のテスト環境を使用してください。

### License

mochiOSのライセンスについては、各リポジトリのライセンスファイル（`license`）を確認してください。

<small>Copyright (c) 2026 mochiOS team.</small>
