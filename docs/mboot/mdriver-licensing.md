# mDriverのライセンス

このページは、mDriverを配布するときに守る区分を記録したものです。法的な助言ではありません。配布方法や収録するfirmwareを変えた場合は、その内容に合わせて確認し直します。

## Linux kernel

Linux kernelはGPL-2.0-onlyです。mBootリポジトリの`mdriver/board/mdriver/patches/linux/`にある変更もkernelへ組み込むため、GPL-2.0-onlyにしています。

`vmlinux`を配布するときは、そのバイナリを作るために使ったLinuxソース、mBoot対応パッチ、最終的な`.config`を提供します。mDriverでは次のコマンドで、Buildrootのライセンス資料と対応するソースを`output/legal-info/`へまとめます。

```sh
make -C mdriver legal-info
```

書面によるソース提供の約束だけに頼らず、バイナリと同じ場所から対応するソースを取得できる配布方法を使います。

## PID 1

`mdriver/init/init.c`はApache-2.0です。Linuxのsyscallだけを使う独立したuserspaceプログラムとしてビルドします。Linux kernelはsyscall interfaceを明確な境界として扱い、UAPIには`Linux-syscall-note`例外があります。

PID 1のソースとApache-2.0のライセンス文も`output/legal-info/`へ収録します。将来、GPL-onlyのkernel symbolを使うkernel moduleを追加する場合、そのmoduleにはApache-2.0を適用しません。kernel内で動くmBoot guest driverはGPL-2.0-onlyにします。

## mBootとmochiOS

mBoot、mochiOS、mDriver kernelは別々の実行ファイルです。mBootはLinux kernelをリンクせず、別のDomainへ読み込んで仮想CPUを開始します。通信にはHypercall、共有ページ、Event Channelを使います。この構成では、同じディスクイメージに収録しても別プログラムの集合として扱えると判断しています。

Linux由来のコードをmBoot本体へコピーした場合や、両者を同じアドレス空間でリンクする構成へ変えた場合は、この判断をそのまま使えません。

## firmware

現在のmDriver成果物には外部firmwareを収録しません。Wi-FiやGPU用のfirmwareを追加するときは、それぞれの再配布条件を確認して記録します。Linux kernelがGPLであることは、firmwareを無条件に再配布できるという意味ではありません。

確認に使った一次資料は次の2つです。

- [Linux kernel licensing rules](https://www.kernel.org/doc/html/next/process/license-rules.html)
- [GNU GPL FAQ: Mere Aggregation](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation)
