<h1 align="center">SwiftCore</h1>
<div align="center">
    <a href="https://deepwiki.com/tas0dev/SwiftCore"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
    <a href="https://deps.rs/repo/github/tas0dev/SwiftCore" target="_blank"><img src="https://deps.rs/repo/github/tas0dev/SwiftCore/status.svg" alt="dependency status" /></a>
    <a href="https://discord.gg/2zYbEnMC5H" target="_blank"><img src="https://img.shields.io/badge/Discord-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord server" /></a>
</div>

## About
SwiftCoreはハイブリッドアーキテクチャを採用した、新しいOSです。中学生によって開発/維持されています。
「絶対クラッシュしないこと」を実現しようとしています。

## Build
必要なツール:
- git
- qemu-system-x86_64
- x86_64-elf-gcc
- cargo
- rustup
- make
- e2fsprogs
- texinfo
- build-essentialで入るすべてのツール
- libgcc-s1
- `x86_64-unknown-none`ターゲット
- `x86_64-unknown-uefi`ターゲット

> [!TIP]
> x86_64-elf-gccは[homebrew](https://brew.sh/)でインストールすることを推奨します。（aptにありません）また、brewをインストール時、`Run there commands in your terminal to add Homebrew to your PATH`と表示されたら、必ず指示に従ってください。

1. このレポをクローンします。
2. サブモジュールをインストールします。
    ```bash
    git submodule update --init --recursive
    ```
3. libcのconfigureをします。
    ```bash
    cd src/lib
    ./configure
    ```
4. ビルドします。
    ```bash
    cd ../..
    cargo build
    ```

## How to contribute?
ライセンスは[この](./LICENSE)ファイルを参照してください

## Documentation
セキュリティ/運用ドキュメントは [`docs/`](./docs/README.md) を参照してください。
