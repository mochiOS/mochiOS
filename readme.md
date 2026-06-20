<div align="center">
<h1>mochiOS</h1>
<a href="https://deepwiki.com/tas0dev/mochiOS"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
<a href="https://deps.rs/repo/github/tas0dev/mochiOS" target="_blank"><img src="https://deps.rs/repo/github/tas0dev/mochiOS/status.svg" alt="dependency status" /></a>
<a href="https://discord.gg/2zYbEnMC5H" target="_blank"><img src="https://img.shields.io/badge/Discord-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord server" /></a>
</div>

## About
mochiOSはハイブリッドアーキテクチャを採用した、新しいOSです。中学生によって開発/維持されています。
「絶対クラッシュしないこと」を実現しようとしています。

餅という名前にしたのは餅は柔らかくて壊れにくいから（伸びても切れない）。超絶安直なネーミングだぜぇ。

## Build and Run

必要なツール:

- `cargo`
- `rustup`
- `qemu-system-x86_64`
- `mke2fs`
- `mkfs.fat`
- `mtools`
- `perl`
- `openssl`
- nightly toolchain

submoduleの初期化を行います。

```bash
git submodule update --init --recursive
```

newlibをconfigureする必要があります。`newlib`のルートディレクトリで以下のコマンドを実行してください。

```bash
./configure
```

## How to contribute?

ライセンスは[この](license)ファイルを参照してください

## Document
まともなドキュメントはまだないです。
[DeepWiki](https://deepwiki.com/tas0dev/mochiOS)を読んでください。
