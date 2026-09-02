# バイナリを小さくする順番

2026年9月1日のrelease成果物を調べました。ここに書く数値は、`out/artifacts`と`mboot/output`に実在するファイルから取得したものです。`target/debug`に残っている過去のビルドは配布物ではないので、製品サイズには数えていません。

ファイルサイズだけを減らしても、起動後のRAMや処理時間が増えれば意味がありません。そのため、ELF全体、LOAD segment、常駐領域を分けて見ます。`strip`や圧縮による削減と、実行時の削減も混ぜません。

## 現在の大きさ

| 対象 | ファイル | 読み込む量 | 補足 |
|---|---:|---:|---|
| mnu kernel | 875,424 bytes | 1,274,894 bytes | 配布用ELFです。symbolは別に保存します |
| mBoot UEFI | 208,384 bytes | 208,384 bytes以下 | `.text`は182,394 bytesです |
| mDriver Linux | 52,058,088 bytes | 40,955,752 bytes | 非LOAD部分が11,102,336 bytesあります |
| mDriver initramfs | 924,160 bytes | 924,160 bytes | 未圧縮cpioです |

mBoot UEFIはすでに小さく、真っ先に削る対象ではありません。mDriver Linuxは大きいものの、GPUやNVMe、USB、Wi-Fiのドライバを組み込んだ単一の`vmlinux`です。ファイル末尾のsymbolを消すだけでは、約41 MBあるLOAD segmentは減りません。

mochiOSのViewKit利用バイナリも調査しましたが、今回の変更対象には含めません。次の値は別作業の優先順位を決めるための観測値です。

| バイナリ | bytes |
|---|---:|
| Binder | 10,556,968 |
| Settings | 10,217,472 |
| secure-ui | 10,151,848 |
| Installer | 9,251,736 |
| Files | 9,209,720 |
| Terminal | 9,094,864 |
| test.app | 9,078,880 |

合計は67,561,488 bytesです。各バイナリにはInter Variable 879,708 bytesとUDEV Gothic 3,874,340 bytesが埋め込まれています。同じ4,754,048 bytesを7本へ入れているため、フォントだけで33,278,336 bytesの重複です。その一方で、同じフォントはrootfsの`/libraries/fonts`にも置かれています。ここは明らかに無駄です。

`Binder`を例にすると、LOAD対象の`.text`は3,906,211 bytes、`.rodata`は5,520,384 bytesでした。symbolをすべて取り除いた試算では10,556,968 bytesから9,442,544 bytesになりました。ここでは測定結果だけを残し、ViewKit、フォント、画像codec、アプリのリンク方法は変更しません。

## mnu

カーネルのファイルサイズは目標の約1 MiBに入りました。最初の18,710,928 bytesから834,432 bytesまで減っています。現在は常駐量と待ち時間を詰めています。

- 4,460,544 bytesあった`KSTACK_POOL`は削除しました。threadが生きている間だけ物理pageを持ち、guard pageを挟んで配置します。
- AP bootstrap stackは、固定64本から起動するAPごとの確保へ変更しました。APが通常stackへ移ったことを確認してから回収します。
- kernel heapは32 MiBを一括mapせず、256 KiBから必要な分だけ増やします。allocation headerも64 bytesから48 bytesへ縮めました。
- 物理frame allocatorは、メモリマップの走査位置を保持します。要求ごとの先頭からの再走査はなくしました。
- process table、thread queue、audit buffer、firmware表示bufferは、それぞれ4万から13万bytesほどあります。上限を下げる前に実際のpeakを計測し、空のslotが持つデータを分離します。
- 配布用kernelからsymbol tableを外し、139,576 bytesの`kernel.debug`へ分けました。bootloaderは`kernel.meta`からAPのentryを取得します。
- `.text`は497,270 bytesです。ここから先は関数を闇雲に短くしません。VFS、exec、page管理、schedulerの順に時間とallocationを測り、遅い経路から直します。
- loaderのmetadataをkernel側へ退避し、使用中の範囲を除いた`BootloaderReclaimable`を通常RAMへ戻します。2 vCPUのQEMUでは90,112 bytesを回収しました。

`clone()`については、mnu内で目立つのはVFSとpage table周辺です。ただし、Copy相当の小さな値とpage内容の複製を同じ一回として数えると判断を誤ります。allocation回数とコピーしたbytesを計測へ追加し、後者を先に消します。

## アプリとサービス

ViewKit、フォント、text layout、画像codec、BinderやSettingsを含むアプリの最適化は、この一連のmnu作業から分離します。上の重複は解消候補ですが、UIの表示、移植性、ABIを同時に変えるため、カーネルの測定へ混ぜません。アプリ側を始める場合は別のbaselineと作業計画を用意します。

## mDriver

現在のLinux設定は91個の機能を組み込みにしており、moduleは無効です。Intel、AMD、NVIDIAのGPU driverや複数世代のNIC、Wi-Fi driverを一つのLOAD segmentへ入れているので、使わないdriverも常駐します。

次の段階では、起動に必須なmBoot guest driver、PCI、IOMMU、block backendだけを組み込みに残します。GPU、Wi-Fi、Ethernet、USB storageなどは署名済みmoduleへ分け、検出したデバイスに必要なものだけ読み込みます。広い実機対応を保ったまま常駐量を減らすには、この方法が妥当です。

配布サイズにはkernelとinitramfsの圧縮も効きますが、mBootは今のところELFの`vmlinux`を直接読みます。Linux boot protocolか、mBoot内の展開処理を追加してから切り替えます。圧縮はRAMを減らさないため、module分離より後です。

## ビルド成果物

`mboot/target/debug`には20 MBを超える古い`mbootd`が複数残っています。これらは製品へ入りませんが、作業領域とキャッシュを圧迫しています。削除対象をrepository単位で確認できるコマンドを用意し、ビルド中のtargetや共有cacheを巻き込まないようにします。

最適化中に毎回全体をclean buildする運用もやめます。カーネルだけなら`make update-kernel`で既存イメージを更新できます。次はmBoot、mDriver、サービス、アプリにも個別の入力fingerprintを持たせ、変更のあった成果物と、それを含むimageだけを更新します。全体のstamp一個で再ビルドを決める現在の方式は粗すぎます。

## 作業順

1. 現在の成果物、LOAD領域、常駐領域、待ち時間を保存します。
2. 小さいIPCの固定領域と一時allocationをなくします。
3. kernel stackとAP bootstrap stackを必要なときだけ確保し、安全に回収します。
4. 物理page allocatorとheapの使用量、断片化、走査、zeroing、lock待ちを測ります。現在はここです。
5. schedulerとtimerのcontext switch、run queue、wake-up latencyを測ります。
6. VFS、mmap、process、exec、forkのコピーとallocationを減らします。
7. 大きなIPCをGrantと共有ringへ移します。
8. 起動経路を測り、依存しない初期化と遅延可能な処理を分けます。
9. blockとnetworkは測定で支配的だった場合だけ変更します。
10. 最後にlinkとbuild設定を比較します。

mDriverとアプリの観測値は残しますが、この順序へ割り込ませません。

一回の変更で複数の要因を混ぜません。前後のバイナリ、LOAD量、起動ログ、実機の待ち時間を残し、改善しなかった変更は戻します。
