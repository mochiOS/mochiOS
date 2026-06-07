# Kagami
The window server for mochiOS.

!!! CURRENTLY, IN DEVELOPMENT !!!

## Backends

描画ターゲットは Cargo features で切り替えます（優先順位：`backend-linux-fb` > `backend-mochios-vga` > `backend-generic-memory` > `backend-custom`）。

- Linux framebuffer: `--features backend-linux-fb`（default）
- mochiOS VGA/VRAM: `--no-default-features --features backend-mochios-vga`
- mochiOS VGA/VRAM (hosted): `--no-default-features --features backend-mochios-vga-hosted`（Linuxホストで mochi_syscall の hosted-vga を使う）
- In-memory (debug/CI): `--no-default-features --features backend-generic-memory`
