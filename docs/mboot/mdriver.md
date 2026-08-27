# mDriver

mDriverは、Linuxのドライバを使ってmochiOSの物理デバイスを動かすHardware Domainです。Linuxデスクトップとしては使いません。ログイン画面、一般ユーザー向けのシェル、パッケージ管理機能、外部向けの管理サービスは入れず、mBootから割り当てられたデバイスだけを扱います。

## 作り直せるビルド環境

mDriverはBuildroot 2025.02.16とLinux 6.12.98で作ります。Buildrootの版、Linuxの版、kernel設定、mBoot用パッチをリポジトリで固定しています。取得するアーカイブはSHA-256を確認し、一致しないものは使いません。

mBootリポジトリを初めてビルドするときや、必要なキャッシュがなくなったときは`setup.sh`が必要なものを取得します。普段は取得済みのファイルを確認して再利用します。

```sh
./setup.sh
make -C mdriver build
```

必要なソースを取得したあとは、ネットワークを使わないビルドも確認できます。

```sh
make -C mdriver build-offline
```

成果物は`mdriver/output/artifacts/`に作られます。

| ファイル | 内容 |
|---|---|
| `vmlinux` | mBootがPVH形式で読み込むLinux kernelです |
| `initramfs.cpio` | PID 1だけを収録した最小initramfsです |
| `linux.config` | 実際のビルドで使ったkernel設定です |

mBootのイメージへ収録するときは、kernelとinitramfsを明示します。

```sh
make image \
  CONFIG=config/qemu-mdriver.toml \
  MDRIVER_KERNEL=mdriver/output/artifacts/vmlinux \
  MDRIVER_INITRAMFS=mdriver/output/artifacts/initramfs.cpio
```

ファイルが見つからない場合、mBootは別のLinuxをその場で作ったり、古い成果物を黙って使ったりせず、ビルドを止めます。

## mBootからデバイスを受け取るまで

mDriverはmBoot上でPVH起動します。Linuxの初期化中にDevice QueryでPCI機能を調べ、Launch Manifestで許可されたデバイスだけをDevice Claimで受け取ります。その後、仮想BARをLinuxのPCI coreへ登録し、LinuxがBARの位置と大きさを確認できた場合にDevice Activateを呼びます。Intel VMXでは`vmcall`、AMD SVMでは`vmmcall`を使います。途中で失敗した場合はReadyを送りません。

Linuxの通常のPCI列挙は無効にしています。mDriverはPCI設定空間を勝手に走査せず、mBootから受け取った設定値と仮想BARだけをPCI coreへ見せます。

物理ストレージの登録とI/O制限は[物理ストレージ](storage.md)に分けて説明しています。

## ライセンス

Linux kernelとmBoot対応パッチはGPL-2.0-onlyです。PID 1は独立したuserspaceプログラムで、Apache-2.0です。配布時に必要なソースやライセンス文書については[mDriverのライセンス](mdriver-licensing.md)を確認してください。
