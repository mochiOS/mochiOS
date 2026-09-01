# mnuカーネルのサイズ

2026年9月1日のreleaseビルドを、最適化前の基準値として保存しています。計測に使ったJSONは[baseline](baselines/mnu-kernel-2026-09-01.json)にあります。

再計測には次のコマンドを使います。

```shell
make measure-kernel-size
```

結果は`out/metrics/kernel-size.json`へ出力されます。ビルドは行わないため、既存の`out/artifacts/kernel.elf`を測ります。

カーネルの変更だけを既存イメージへ反映するときは、次を使います。

```shell
make update-kernel
```

この処理はユーザーランド、initfs、rootfsを作り直しません。カーネルをreleaseビルドし、ESP単体と二つのディスクイメージ内にある`/system/kernel.elf`を置き換えます。既存イメージがない場合は、不完全なイメージを新しく作らずに終了します。

bootloaderだけを更新する場合は`make update-boot`を使います。こちらも既存のESPとディスク内にある`BOOTX64.EFI`だけを置き換えます。

`make update-kernel`は`secondary_cpu_entry`のアドレスを`kernel.meta`へ書き出します。bootloaderはこの小さなファイルを使ってAPのentryを決めるため、配布用kernelからsymbol tableを分離してもSMP起動を続けられます。metadataがない場合に使うELF parserのoffset上書きも修正しました。

## 配置

| 項目 | bytes | 配置 |
|---|---:|---|
| ELF全体 | 18,710,928 | ファイル |
| LOAD segmentのfilesz合計 | 18,560,566 | ファイル |
| LOAD segmentのmemsz合計 | 48,656,814 | メモリ |
| `.text` | 539,574 | ファイルとメモリ |
| `.rodata` | 41,594 | ファイルとメモリ |
| `.data` | 17,954,240 | ファイルとメモリ |
| `.bss` | 30,093,624 | メモリ |
| relocation | 19,440 | ファイルとメモリ |
| symbol table | 141,041 | ファイル |

LOAD segment間の空白は3,978 bytes、最大alignmentは4,096 bytesでした。debug sectionはありません。

## 固定領域

`MAILBOXES`は17,738,760 bytesあり、`.data`の98.8%を占めています。`Mailbox::new()`が空きスロット番号と`free_count = 64`を静的に設定するため、現在の表現はゼロ初期化できません。

| 領域 | bytes | 配置 |
|---|---:|---|
| `MAILBOXES` | 17,738,760 | `.data` |
| `KSTACK_POOL` | 16,781,312 | `.bss` |
| `RING0_STACKS` | 8,388,608 | `.bss` |
| `IST_STACKS` | 4,194,304 | `.bss` |
| `AP_BOOT_STACKS` | 524,288 | `.bss` |
| `KERNEL_THREAD_STACK` | 32,768 | `.bss` |

## 最初の変更後

空きスロット情報だけを起動時に作り、`MAILBOXES`を`.bss`へ移す案は採りませんでした。それでは17.7 MBの常駐領域が残るためです。

代わりに、メールボックスから本文を分離し、キューへ積まれたときだけ確保するようにしました。この時点ではELF全体が1,012,312 bytes、`MAILBOXES`が44,040 bytesです。詳しい値は[IPC動的化後の計測結果](baselines/mnu-kernel-dynamic-ipc-2026-09-01.json)にあります。

さらに、解放した本文を64件まで再利用し、送信側の一時bufferをなくしました。kernel stack poolとCPU用TSS stackも実際の上限に合わせています。現在はELF全体が1,008,296 bytes、LOAD segmentのメモリ量が6,246,845 bytesです。[IPCとstack整理後の計測結果](baselines/mnu-kernel-ipc-stacks-2026-09-01.json)で内訳を確認できます。

受信側にあった4,128 bytesの一時`Vec`をなくした後は、ELF全体が1,007,024 bytes、LOAD segmentのメモリ量が6,246,710 bytesになりました。[IPC受信変更後の計測結果](baselines/mnu-kernel-ipc-receive-2026-09-01.json)に、この時点のrevisionとsection内訳を保存しています。

64 CPU分あった`AP_BOOT_STACKS`を、起動するAPごとのframe確保へ移した後は、ELF全体が1,007,216 bytes、LOAD segmentのメモリ量が5,722,775 bytesになりました。コードは192 bytes増えましたが、静的なLOAD領域は523,935 bytes減っています。[AP stack変更後の計測結果](baselines/mnu-kernel-ap-stacks-2026-09-01.json)で確認できます。

最後に、配布用`kernel.elf`と開発用`kernel.debug`を分けました。配布用は866,728 bytes、開発用symbolは139,576 bytesです。LOAD segmentは変えていないため、起動後のメモリ量は5,722,775 bytesのままです。stripped kernelでも2 vCPUが起動し、APがonlineになってからサービス群まで進むことを確認しました。[配布用kernelの計測結果](baselines/mnu-kernel-stripped-2026-09-01.json)に内訳があります。

APが通常のidle stackへ切り替わった後にbootstrap stackを回収するようにした時点では、配布用kernelは867,240 bytes、LOAD segmentのメモリ量は5,726,199 bytesです。CPU別の状態と回収処理に3,424 bytes増えましたが、4 vCPU構成では起動中だけ使う24 KiBがonline後に残らなくなりました。また、物理offsetが0のidentity mapを未初期化と誤認してframe解放を拒む問題も直しています。[AP stack回収後の計測結果](baselines/mnu-kernel-ap-stack-reclaim-2026-09-01.json)で条件を確認できます。

通常threadの固定stack poolも削除しました。thread生成時に必要な物理pageだけを割り当て、終了後は別のstackへ切り替わってから回収します。guard pageは未mapのまま残し、ユーザー用page tableへはsupervisor-onlyかつ実行不可でmapしています。

この変更後の配布用kernelは863,296 bytes、LOAD segmentのメモリ量は1,261,983 bytesです。前回の5,726,199 bytesから4,464,216 bytes減りました。4 vCPUのTCG試験では3個のAPとsystem service起動まで進み、page fault、panic、stack quarantineは発生していません。[動的stack変更後の計測結果](baselines/mnu-kernel-dynamic-stacks-2026-09-01.json)に内訳を保存しています。

計測buildでは、解放時にstackの初期patternを調べます。今回観測した最大使用量は65,536 bytes中1,952 bytesでした。起動試験一回の値にすぎないため、設定値はまだ下げません。

kernel heapは32MiBの仮想上限を残しつつ、最初に256KiBだけ物理mapする形へ変えました。足りなくなると256KiB単位でpageを追加し、再利用pageもallocatorへ渡す前にzeroingします。1 vCPUと4 vCPUの試験では、同じservice起動地点で1,323,008 bytesまで増えました。以前の32MiB一括mapと比べると、7,869 page、32,231,424 bytesを起動時に確保せずに済んでいます。

追加した処理により配布用kernelは875,288 bytesへ増えました。LOAD segmentのメモリ量は1,270,886 bytesです。ファイルの11,992 bytes増加より、起動時に減る物理メモリのほうが大きいため、この変更は残します。[段階確保へ変更したheapの計測結果](baselines/mnu-kernel-growable-heap-2026-09-01.json)に条件をまとめています。
