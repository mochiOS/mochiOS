# 起動手順

このリポジトリは、ルートで `cargo run` を実行するとビルドと起動をまとめて行います。

## 必要条件

- `qemu-system-x86_64`
- `cargo`
- `rustup`
- `mke2fs`
- `mkfs.fat`
- `mtools`
- `perl`
- `openssl`

## 起動

```bash
cargo run
```

デフォルトでは QEMU の画面ウィンドウを開きます。`stdout` にはシリアルログも出ます。

## 表示モード

表示モードは `QEMU_DISPLAY` で上書きできます。

- `QEMU_DISPLAY=gtk cargo run` でウィンドウ表示を明示
- `QEMU_DISPLAY=sdl cargo run` で SDL 表示に切替
- `QEMU_DISPLAY=none cargo run` で従来の headless 実行

環境変数が未設定の場合は `gtk` を使います。

## 期待する出力

正常起動すると、serial に `core.service: resident core process online` が出ます。
