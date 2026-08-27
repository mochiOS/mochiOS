# mDriver

mDriverは、Linuxのドライバを使ってmochiOSの物理デバイスを動かすHardware Domainです。Linuxデスクトップとして使うものではないので、ログイン画面や一般ユーザー向けのシェル、パッケージ管理機能、外部向けの管理サービスは入れません。

## 作り直せるビルド環境

mDriverのビルドにはBuildroot 2025.02.16とLinux 6.12.98を使います。kernel設定とmBoot用パッチもリポジトリで固定し、取得したアーカイブのSHA-256が一致しなければ処理を止めます。

最初のビルドやキャッシュを消したあとは、`setup.sh`が必要なものを取得します。普段は取得済みのファイルを確認して再利用します。

```sh
./setup.sh
make -C mdriver build
```

一度取得すれば、ネットワークを使わずに再ビルドできるかも確認できます。

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

mDriverはmBoot上でPVH起動します。Linuxの初期化中にDevice QueryでPCI機能を調べ、Launch Manifestで許可されたデバイスをDevice Claimで受け取ります。その後、仮想BARをLinuxのPCI coreへ登録し、BARの位置と大きさを確認できた場合にDevice Activateを呼びます。Intel VMXでは`vmcall`、AMD SVMでは`vmmcall`を使い、途中で失敗した場合はReadyを送りません。

Linuxの通常のPCI列挙は無効です。そのため、mDriverがPCI設定空間を走査することはなく、mBootから受け取った設定値と仮想BARだけがPCI coreから見えます。

物理ストレージの登録とI/O制限は[物理ストレージ](storage.md)に分けて説明しています。

## ライセンス

Linux kernelとmBoot対応パッチはGPL-2.0-onlyです。PID 1は独立したuserspaceプログラムで、Apache-2.0です。配布時に必要なソースやライセンス文書については[mDriverのライセンス](mdriver-licensing.md)を確認してください。
