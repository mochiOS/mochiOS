# mDriverから使う物理ストレージ

実機向けの標準設定では、mDriverへVMDやNVMeを割り当てません。物理ディスクへの書き込みを有効にする前に、対象ディスクとmochiOS用パーティションを登録する必要があります。

この仕組みは、mochiOSやアプリから届いた範囲外I/OによってWindowsなど別のパーティションを壊さないためのものです。mBootとmDriverは信頼するシステム部品として扱います。mDriver、mBoot、デバイスのfirmwareそのものが侵害された場合まで防ぐ仕組みではありません。

## 登録するもの

Launch Manifestには、次の3つを署名対象として記録します。

| 値 | 用途 |
|---|---|
| Disk GUID | 対象の物理ディスクを特定します |
| Partition Type GUID | mochiOS用として用意したパーティションの種類を確認します |
| Unique Partition GUID | そのディスク上の対象パーティションを1つに絞ります |

GPTのGUIDはディスクごと、パーティションごとに変わります。QEMUテスト用設定のGUIDを実機へコピーしてはいけません。実機で使う値は、対象ディスクのGPTを確認して登録します。

デバイス設定では`partitioned`、`ephemeral`、`read_only`のうち1つだけを選べます。

| 設定 | 使う場所 |
|---|---|
| `partitioned = true` | 登録した物理ディスクのmochiOS用パーティションへ読み書きします |
| `ephemeral = true` | QEMUなど、内容を破棄できるテストディスク全体へ読み書きします |
| `read_only = true` | ディスク全体を読み取り専用で調べます |

`ephemeral = true`をWindowsとのデュアルブートに使う物理ディスクへ設定してはいけません。

## 起動時に確かめること

mDriverはブロックデバイスを公開する前に、次の内容を確認します。

- Protective MBRが正しく、hybrid MBRになっていないこと
- Primary GPTとBackup GPTの両方が読めること
- GPT headerとpartition entry arrayのCRCが正しいこと
- Primary GPTとBackup GPTが同じパーティションを示していること
- パーティションがディスクの範囲内にあり、互いに重なっていないこと
- 登録したDisk GUIDが一致すること
- Partition Type GUIDとUnique Partition GUIDの両方が一致するパーティションが1つだけあること

GPTが壊れている場合や、登録したGUIDと一致しない場合は接続を拒否します。mDriverはGPTを自動修復しません。壊れたメタデータへ書き込んで状況を悪化させないためです。

複数のディスクが同じ登録内容に一致した場合も拒否します。正しそうな1台を推測して選ぶことはありません。

## mochiOSへ見せる範囲

確認が終わると、mDriverは対象パーティションの先頭をLBA 0とする仮想ブロックデバイスをmochiOSへ公開します。mochiOSからはGPTやほかのパーティションが見えません。

同期I/Oと非同期I/Oの両方で、要求したLBAと長さを検査します。整数の桁あふれ、パーティション末尾を越える要求、変換後に物理ディスクの末尾を越える要求は、デバイスへ渡す前に拒否します。

## パーティションを用意するとき

mBootとmDriverは、既存のパーティションを縮小したり、新しいパーティションを作ったり、ファイルシステムを初期化したりしません。これらは信頼できるインストーラーや管理ツールで行います。

Windowsとのデュアルブート環境では、作業前に大切なデータをバックアップします。対象ディスク、空き領域、新しく作るmochiOS用パーティションを画面上で確認し、作成後の3つのGUIDをLaunch Manifestへ登録します。

現在の`config/intel-hardware.toml`はストレージを割り当てない状態を保っています。VMD配下のNVMeを実機で使うには、GUIDの登録だけでなく、VMDとその配下のデバイスを安全に渡せることも確認する必要があります。そこまで確認できるまでは設定を有効にしません。

## 実機で読み取り専用の確認をするとき

通常の実機イメージとは別に、Intel VMDを読み取り専用で確認するイメージを作れます。

```sh
make storage-probe-image
```

出力は`out/mochiOS-storage-probe.iso`です。このイメージは`config/intel-storage-probe.toml`を使います。通常の`config/intel-hardware.toml`は変更しません。

mDriverはディスクを読み取り専用で開き、Primary GPTとBackup GPT、CRC、パーティションの範囲と重複を確認します。確認できた場合は、Disk GUIDと各パーティションのType GUID、Unique Partition GUID、開始位置、長さを画面とシリアルログへ出します。画面に出せる量を超えた情報もシリアルログには残ります。

診断画面を出せるのはmochiOS System Domainだけです。一般アプリのDomainやLinux Application Domainから、起動時の診断画面を書き換えることはできません。

この確認用設定では、ブロックデバイスをwrite modeで開きません。制御プロトコルのwriteとflushも拒否します。QEMUテストでは、確認前後のディスク全体のSHA-256が同じであることを検査しています。

GPTが壊れている場合、VMDが見つからない場合、VMD配下のディスクを1台に絞れない場合は処理を止めます。候補から推測して続行しません。

## QEMUでの確認

`make mdriver-test`では、一時的なGPTディスクを作って次の2つを確認します。

- 正しいGPTでは、対象パーティションの内容だけが変わり、その前後は変わらないこと
- Primary GPTを壊した場合は接続を拒否し、ディスク全体が一切変わらないこと

このテストは実装の退行を見つけるためのものです。実機へ書き込む前のバックアップや、対象ディスクの確認を省略できるものではありません。
