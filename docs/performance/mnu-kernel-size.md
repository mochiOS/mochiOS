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

allocation headerを64 bytesから48 bytesへ縮めた後の配布用kernelは875,424 bytesです。命令とrelocationが136 bytes増えましたが、allocationごとの予約量は16 bytes減っています。magic、cookie、checksum、tail canaryは維持しています。

allocatorの判断に必要な内訳は`performance-instrumentation`でだけ収集します。要求サイズ別、CPU別、schedulerやIPCなどの処理元別のallocation数に加え、commit済みheap量と内部断片化の現在値・peakを取得できます。通常buildは875,424 bytesのままで、計測buildは896,272 bytesでした。[allocator計測の確認結果](baselines/mnu-kernel-allocator-observability-2026-09-01.json)にABIと起動確認の範囲を残しています。

計測buildはQEMU TCGの2 vCPUでAP onlineとsystem service起動まで進みました。現在は`display.driver`が終了する既知の問題によりシェルへ到達しないため、実機負荷の`mperf`結果はまだ採れていません。コンパイルとABIだけを根拠にper-CPU cacheやsize class allocatorへ置き換えることはしません。

物理frame allocatorは、実装と合っていなかった`BitmapFrameAllocator`という名前を改めました。未使用領域のcursorと、解放済みframeを再利用するfree listで動きます。新規確保のたびにメモリマップ先頭へ戻る処理もやめ、最後に使ったregionから調べます。

通常buildは引き続き875,424 bytesです。LOAD segmentはファイル上866,006 bytes、メモリ上1,274,894 bytesでした。計測buildは896,256 bytesで、物理frameの要求、再利用、新規確保、region走査、連続確保、失敗理由、heap拡張時のzeroing量とcycleを`mperf`から取得できます。[物理frame計測の確認結果](baselines/mnu-kernel-frame-observability-2026-09-01.json)にrevisionとABI境界を保存しています。

frame allocatorのlock待ちと外部断片化を追加で観測できるようにした後、物理pageのzeroingを一つの関数へまとめました。通常buildの配布用kernelは875,480 bytesです。LOAD segmentはファイル上866,134 bytes、メモリ上1,269,962 bytesで、`.text`は537,622 bytesになりました。

共通化前と比べるとELF全体は56 bytes、LOAD segmentのファイル量は144 bytes増えています。その代わり`.text`は1,568 bytes、LOAD segmentのメモリ量は4,916 bytes減りました。ユーザー空間へ渡すpage、page table、共有pageのzeroingは残し、失敗時はmappingを中止して確保済みpageを戻します。

performance ABI v5には、frame確保数のCPU別・処理元別内訳と、zeroingの処理元別回数・cycleを追加しました。この計測処理はfeatureで分離されており、通常buildの上記サイズは追加前と同一です。計測buildのraw stripped kernelは892,008 bytes、snapshotは2,840 bytesです。[frame活動の計測結果](baselines/mnu-kernel-frame-activity-2026-09-02.json)に詳細を残しています。

ページテーブルのOOM経路を直し、frameの確保とzeroingを共通化した後は871,384 bytesになりました。LOAD segmentはファイル上862,038 bytes、メモリ上1,265,866 bytesです。直前からいずれも4,096 bytes減り、`.text`は536,086 bytesです。

実行時には、ユーザーページテーブルを作るたびに確保していた不要なL2を1 page減らしました。途中の確保に失敗した場合も、生成済みのtable frameを破棄します。通常の起動処理はQEMU TCGの2 vCPUでsystem service群まで進んでいます。[ページテーブルOOM監査の結果](baselines/mnu-kernel-page-table-oom-2026-09-02.json)に確認範囲を記録しています。

cext loaderのrollbackとW^X切替を直した後は875,488 bytesです。LOAD segmentはファイル上866,006 bytes、メモリ上1,269,798 bytesです。直前よりファイルが4,104 bytes増えました。失敗したmoduleのpageを残さず、書込み中のsegmentを実行可能にしないため、この増加は削りません。

計測buildのraw stripped kernelは896,072 bytesで、直前より24 bytes減りました。通常buildだけがpage境界を越えて増えているため、link配置を扱う段階までは両方を別々に追います。[cext rollback後の計測結果](baselines/mnu-kernel-cext-rollback-2026-09-02.json)にsection内訳を保存しています。

cextのDMA確保から`Vec<PhysFrame>`を外し、frame allocatorの連続確保を使うようにした後は830,216 bytesです。LOAD segmentはファイル上824,734 bytes、メモリ上1,232,830 bytesです。直前からファイルは45,272 bytes、`.text`は41,952 bytes減りました。

この変更はサイズだけでなく、DMA範囲の確保に必要なlockを最大64回から1回へ減らします。QEMUの`disk.cext`初期化で成功経路を確認しました。[連続DMA確保後の計測結果](baselines/mnu-kernel-contiguous-dma-2026-09-02.json)に値を保存しています。

DMA syscallがユーザーへ渡す連続範囲をzeroingするようにした後は859,072 bytesです。LOAD segmentはファイル上849,598 bytes、メモリ上1,257,294 bytesです。zeroing前から28,856 bytes増えましたが、DMA整理前との比較では16,416 bytes減っています。

`zero_frame`は呼び出し元へ本体を複製しないよう非inline化しました。DMA範囲は1回の連続clearとして計測し、解放処理も一つのloopへまとめています。[DMA zeroing後の計測結果](baselines/mnu-kernel-dma-zero-2026-09-02.json)に通常buildと計測buildの差を記録しています。

共有page確保の一時`Vec<u64>`を外した後は834,448 bytesです。LOAD segmentはファイル上824,950 bytes、メモリ上1,228,774 bytesです。DMA zeroing直後からファイルは24,624 bytes、`.text`は25,440 bytes減りました。

計測buildは900,144 bytesで4,072 bytes増えています。通常buildの大きな減少にはsection配置の境界も含まれるため、実際に減った命令量とは分けて扱います。実行時には、共有page数の8倍だった一時heap allocationが呼び出し方にかかわらず不要になりました。[共有page確保後の計測結果](baselines/mnu-kernel-shared-page-allocation-2026-09-02.json)に値を保存しています。

per-CPU syscall領域のframe確保とzeroingを共通経路へ揃えた後は830,272 bytesです。`.text`は224 bytes、計測buildは8,264 bytes減りました。通常buildのLOAD segmentはファイル上824,734 bytesです。

LOAD segmentのメモリ量はsection境界の移動により1,232,782 bytesとなり、4,008 bytes増えました。ファイルサイズだけで改善と判断せず、残っている配置の差として追います。[per-CPU frame確保後の計測結果](baselines/mnu-kernel-percpu-frame-allocation-2026-09-02.json)に比較を保存しています。

per-CPUの2-page確保を独立した処理へ分け、失敗注入でrollbackを確認した後は826,184 bytesです。LOAD segmentはファイル上820,646 bytes、メモリ上1,228,638 bytes、`.text`は495,366 bytesです。直前からファイルは4,088 bytes、LOAD時メモリは4,144 bytes減りました。

起動後もloaderが所有していたメモリを見直しました。`BootInfo`、メモリマップ、SMP handoffをkernel内へコピーしてから、参照が残らない`BootloaderReclaimable`だけを`Usable`へ変えます。kernel image、initfs、rootfs、AP trampoline、使用中のBSP stackは回収しません。

変更後は834,432 bytes、LOAD segmentはファイル上824,934 bytes、メモリ上1,237,046 bytesです。2 vCPUのQEMUでは90,112 bytesを回収したため、静的領域の増加を差し引いた起動時の差は81,704 bytes減です。[bootloader memory回収後の計測結果](baselines/mnu-kernel-boot-memory-reclamation-2026-09-02.json)に拒否条件と起動確認も記録しています。

schedulerのrun queue選択時間と起床待ち時間は、`performance-instrumentation`でだけ記録します。通常版のファイルサイズは834,432 bytesから変わりません。計測版のraw stripped kernelは897,032 bytesで、2 vCPUのQEMUでもsystem service起動まで進みました。[scheduler計測経路の確認結果](baselines/mnu-kernel-scheduler-observability-2026-09-02.json)には、通常版へ時刻フィールドが入らないことも記録しています。

失敗注入を含む試験buildは895,984 bytesです。通常buildでは注入用の状態と分岐をコンパイルしません。[frame確保失敗注入の結果](baselines/mnu-kernel-frame-failure-injection-2026-09-02.json)に、通常版との差とQEMUログを記録しています。
