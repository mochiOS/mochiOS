# mDriverのライセンス

mDriverを配布するときのライセンス区分を記録します。法的な助言ではなく、配布方法や収録するfirmwareを変えた場合は確認し直す必要があります。

## Linux kernel

Linux kernelと、`mdriver/board/mdriver/patches/linux/`に置く変更はGPL-2.0-onlyです。

`vmlinux`を配布するときは、そのバイナリに対応するLinuxソース、mBoot用パッチ、最終的な`.config`も提供します。次のコマンドを実行すると、Buildrootがライセンス資料と対応するソースを`output/legal-info/`へまとめます。

```sh
make -C mdriver legal-info
```

書面によるソース提供の約束だけに頼らず、バイナリと同じ場所から対応するソースを取得できる配布方法を使います。

## PID 1

`mdriver/init/init.c`はApache-2.0です。Linuxのsyscallだけを使う独立したuserspaceプログラムとしてビルドします。Linux kernelのsyscall interfaceは明確な境界として扱われ、UAPIには`Linux-syscall-note`例外があります。

PID 1のソースとApache-2.0のライセンス文も`output/legal-info/`へ収録します。将来、GPL-onlyのkernel symbolを使うkernel moduleを追加する場合、そのmoduleにはApache-2.0を適用しません。kernel内で動くmBoot guest driverはGPL-2.0-onlyにします。

## mBootとmochiOS

mBoot、mochiOS、mDriver kernelは別々の実行ファイルです。mBootはLinux kernelをリンクせず、別のDomainへ読み込んで仮想CPUを開始します。通信もHypercall、共有ページ、Event Channelを介します。そのため、同じディスクイメージに収録しても別プログラムの集合として扱えると判断しています。

Linux由来のコードをmBoot本体へコピーした場合や、両者を同じアドレス空間でリンクする構成へ変えた場合は、この判断をそのまま使えません。

## firmware

現在のmDriver成果物には外部firmwareを収録しません。Wi-FiやGPU用のfirmwareを追加する場合は、個別に再配布条件を確認して記録します。Linux kernelがGPLでも、firmwareまで無条件に再配布できるわけではありません。

確認には[Linux kernel licensing rules](https://www.kernel.org/doc/html/next/process/license-rules.html)と[GNU GPL FAQのMere Aggregation](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation)を使いました。
