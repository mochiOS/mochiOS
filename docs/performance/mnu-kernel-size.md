# mnuカーネルのサイズ

2026年9月1日のreleaseビルドを、最適化前の基準値として保存しています。計測に使ったJSONは[baseline](baselines/mnu-kernel-2026-09-01.json)にあります。

再計測には次のコマンドを使います。

```shell
make measure-kernel-size
```

結果は`out/metrics/kernel-size.json`へ出力されます。ビルドは行わないため、既存の`out/artifacts/kernel.elf`を測ります。

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

## 次の変更

空きスロット情報を起動時に作れば、`MAILBOXES`を`.bss`へ移せます。しかし、それでは17.7 MBの常駐領域が残ります。この案は採りません。

次はメールボックスから本文を分離し、システム全体で上限を持つメッセージプールへ移します。メールボックスにはプール内の番号だけを置きます。これにより、未使用スレッドと空のキューがメッセージ本文を占有しなくなります。
