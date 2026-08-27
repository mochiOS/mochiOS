# mDriverから使う物理ストレージ

実機向けの標準設定では、mDriverへVMDやNVMeを割り当てません。物理ディスクへの書き込みを有効にするには、対象ディスクとmochiOS用パーティションの登録が必要です。

登録した範囲だけを公開することで、mochiOSやアプリから届いた範囲外I/OがWindowsなど別のパーティションへ達するのを防ぎます。ここではmBootとmDriverを信頼するため、この2つやデバイスのfirmwareそのものが侵害された場合は対象外です。

## 登録するもの

Launch Manifestには3種類のGUIDを署名対象として記録します。

| 値 | 用途 |
|---|---|
| Disk GUID | 対象の物理ディスクを特定します |
| Partition Type GUID | mochiOS用として用意したパーティションの種類を確認します |
| Unique Partition GUID | そのディスク上の対象パーティションを1つに絞ります |

GPTのGUIDはディスクとパーティションごとに変わるため、QEMUテスト用設定の値を実機へコピーしてはいけません。実機では対象ディスクのGPTを確認して登録します。

デバイス設定では`partitioned`、`ephemeral`、`read_only`のうち1つだけを選べます。

| 設定 | 使う場所 |
|---|---|
| `partitioned = true` | 登録した物理ディスクのmochiOS用パーティションへ読み書きします |
| `ephemeral = true` | QEMUなど、内容を破棄できるテストディスク全体へ読み書きします |
| `read_only = true` | ディスク全体を読み取り専用で調べます |

`ephemeral = true`をWindowsとのデュアルブートに使う物理ディスクへ設定してはいけません。

## 起動時に確かめること

mDriverはブロックデバイスを公開する前に、Protective MBR、Primary GPT、Backup GPTを読み、headerとpartition entry arrayのCRCを確認します。両方のGPTが同じパーティションを示し、各パーティションがディスク内に収まっていて互いに重ならないことも必要です。

そのうえでDisk GUIDが登録値と一致し、Partition Type GUIDとUnique Partition GUIDの両方に一致するパーティションが1つだけある場合に接続します。GPTが壊れている、GUIDが違う、同じ登録内容に複数のディスクが一致するといった場合は拒否します。候補を推測して選んだり、GPTを自動修復したりはしません。

## mochiOSへ見せる範囲

確認が終わると、mDriverは対象パーティションの先頭をLBA 0とする仮想ブロックデバイスをmochiOSへ公開します。このため、mochiOSからGPTやほかのパーティションは見えません。

同期I/Oと非同期I/Oの両方で、要求したLBAと長さを検査します。整数の桁あふれ、パーティション末尾を越える要求、変換後に物理ディスクの末尾を越える要求は、デバイスへ渡す前に拒否します。

## Installer.appからパーティションを変更するとき

Installer.appは、ディスク全体が書き込み可能な状態でmDriverへ割り当てられている場合に限り、GPTパーティションを作成または削除できます。既存パーティションの縮小とGPTの自動修復は行いません。

mDriverは操作の直前にPrimary GPTとBackup GPTを読み直します。両方が一致しない場合や、指定範囲が別のパーティションと重なる場合は書き込みません。作成時はBackup GPTを先に書いて同期し、その後Primary GPTを書きます。削除時は、Installer.appが選んだパーティションのUnique Partition GUIDが変わっていないことも確認します。

Windowsパーティションだけを特別扱いする判定は入れていません。削除を選べば、secure-uiの確認後に通常のGPTパーティションと同じ手順で削除します。必要なデータは作業前にバックアップしてください。

パーティションを作成した後は、新しいDisk GUID、Partition Type GUID、Unique Partition GUIDをLaunch Manifestへ登録します。

現在の`config/intel-hardware.toml`はストレージを割り当てない状態を保っています。VMD配下のNVMeを実機で使うには、GUIDの登録だけでなく、VMDとその配下のデバイスを安全に渡せることも確認する必要があります。そこまで確認できるまでは設定を有効にしません。

## 実機で読み取り専用の確認をするとき

Intel VMDを読み取り専用で調べる場合は、通常の実機イメージと分けて確認用イメージを作ります。

```sh
make storage-probe-image
```

`out/mochiOS-storage-probe.iso`が生成され、`config/intel-storage-probe.toml`が使われます。通常の`config/intel-hardware.toml`は変更しません。

mDriverはディスクを読み取り専用で開き、前述のGPT検査を行います。問題がなければ、Disk GUIDと各パーティションのType GUID、Unique Partition GUID、開始位置、長さを画面とシリアルログへ出します。画面に収まらない情報もシリアルログには残ります。

診断画面へ出力できるのはmochiOS System Domainだけです。また、確認用設定ではブロックデバイスをwrite modeで開かず、制御プロトコルのwriteとflushも拒否します。QEMUテストでは、処理前後のディスク全体のSHA-256が同じであることまで確認します。

GPTが壊れている、VMDが見つからない、VMD配下のディスクを1台に絞れない場合は処理を止めます。

## QEMUでの確認

`make mdriver-test`は一時的なGPTディスクを作ります。正しいGPTでは対象パーティションの内容だけが変わり、その前後が変わらないことを確認します。Primary GPTを壊したテストでは接続を拒否し、ディスク全体が一切変わらないことを確かめます。

このテストは実装の退行を見つけるためのものです。実機へ書き込む前のバックアップや、対象ディスクの確認を省略できるものではありません。
