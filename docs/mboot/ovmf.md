# QEMUで使うOVMF

mBootリポジトリの`firmware/`にあるOVMFはQEMUでの起動確認にだけ使い、実機向けのイメージには収録しません。実機では、そのPCのUEFIファームウェアがEFI System Partitionの`EFI/BOOT/BOOTX64.EFI`を読み込みます。

OVMFを更新するときは、QEMUの4 MiB pflash配置と互換性があるcodeとvariable templateを一緒に差し替えます。片方だけを更新すると、起動できない場合があります。

更新後はmBootリポジトリで両方のテストを実行します。

```sh
make image-test
make qemu-test
```
