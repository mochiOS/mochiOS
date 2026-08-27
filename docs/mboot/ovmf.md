# QEMUで使うOVMF

mBootリポジトリの`firmware/`にあるOVMFは、QEMUでの起動確認にだけ使います。実機向けのイメージには収録しません。実機では、そのPCのUEFIファームウェアがEFI System Partitionにある`EFI/BOOT/BOOTX64.EFI`を読み込みます。

OVMFを更新するときは、QEMUの4 MiB pflash配置と互換性があるcodeとvariable templateを一緒に差し替えます。片方だけを更新すると、起動できない場合があります。

更新後はmBootリポジトリで次の確認を行います。

```sh
make image-test
make qemu-test
```
