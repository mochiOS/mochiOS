# バイナリを小さくする順番

2026年9月1日のrelease成果物を調べました。ここに書く数値は、`out/artifacts`と`mboot/output`に実在するファイルから取得したものです。`target/debug`に残っている過去のビルドは配布物ではないので、製品サイズには数えていません。

ファイルサイズだけを減らしても、起動後のRAMや処理時間が増えれば意味がありません。そのため、ELF全体、LOAD segment、常駐領域を分けて見ます。`strip`や圧縮による削減と、実行時の削減も混ぜません。

## 現在の大きさ

| 対象 | ファイル | 読み込む量 | 補足 |
|---|---:|---:|---|
| mnu kernel | 866,728 bytes | 5,722,775 bytes | 配布用ELFです。symbolは別に保存します |
| mBoot UEFI | 208,384 bytes | 208,384 bytes以下 | `.text`は182,394 bytesです |
| mDriver Linux | 52,058,088 bytes | 40,955,752 bytes | 非LOAD部分が11,102,336 bytesあります |
| mDriver initramfs | 924,160 bytes | 924,160 bytes | 未圧縮cpioです |

mBoot UEFIはすでに小さく、真っ先に削る対象ではありません。mDriver Linuxは大きいものの、GPUやNVMe、USB、Wi-Fiのドライバを組み込んだ単一の`vmlinux`です。ファイル末尾のsymbolを消すだけでは、約41 MBあるLOAD segmentは減りません。

mochiOSのViewKit利用バイナリは次の大きさです。

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

`Binder`を例にすると、LOAD対象の`.text`は3,906,211 bytes、`.rodata`は5,520,384 bytesでした。symbolをすべて取り除いた試算でも10,556,968 bytesから9,442,544 bytesにしかなりません。先にフォントを外へ出したほうが効きます。

## mnu

カーネルのファイルサイズは目標の約1 MiBに入りました。最初の18,710,928 bytesから866,728 bytesまで減っています。次は常駐量を詰めます。

- `KSTACK_POOL`は4,460,544 bytesあります。thread上限64に合わせてありますが、未使用threadのstackまで起動時から持っています。page tableとguard pageの扱いを直してから、thread作成時の確保へ移します。
- AP bootstrap stackは、固定64本から起動するAPごとの確保へ変更しました。静的なLOAD領域は523,935 bytes減り、AP一つにつき8 KiBのframeを確保します。
- process table、thread queue、audit buffer、firmware表示bufferは、それぞれ4万から13万bytesほどあります。上限を下げる前に実際のpeakを計測し、空のslotが持つデータを分離します。
- 配布用kernelからsymbol tableを外し、139,576 bytesの`kernel.debug`へ分けました。bootloaderは`kernel.meta`からAPのentryを取得します。
- `.text`は530,950 bytesです。ここから先は関数を闇雲に短くしません。VFS、exec、page管理、schedulerの順に時間とallocationを測り、遅い経路から直します。

`clone()`については、mnu内で目立つのはVFSとpage table周辺です。ただし、Copy相当の小さな値とpage内容の複製を同じ一回として数えると判断を誤ります。allocation回数とコピーしたbytesを計測へ追加し、後者を先に消します。

## アプリとサービス

最初に、ViewKitがフォントを`include_bytes!`で各実行ファイルへ埋め込む処理をやめます。mochiOSではrootfsの`/libraries/fonts`から読み、LinuxとWindowsでは各OSのfont databaseを使います。起動のたびに同じフォントを別々のheapへ複製する処理も対象です。見た目や文字の選択は変えません。

次にrelease成果物からsymbolを外します。手元で`strip --strip-all`した場合、Binderは1,114,424 bytes、Settingsは1,024,816 bytes、secure-uiは1,016,504 bytes小さくなりました。開発用symbolは別ファイルとして保存し、panicやcrash reportのsymbolizeに使います。配布物から消したからといって、調査できない状態にはしません。

ViewKitは`image` crateのPNG、JPEG、WebP decoderを全アプリへ有効にしています。画像を開かないアプリにもdecoderが入るため、codecをfeatureへ分けます。SettingsのPNG iconのように必要な形式だけを、アプリ側で選べる形にします。SVG、text shaping、rasterizerも同じ方法で依存元を確認します。

release profileは各repositoryで`panic = "abort"`しか指定していません。`strip = "symbols"`は採用できます。ThinLTOと`codegen-units = 1`は、代表バイナリ一つでファイルサイズ、起動時間、操作のp95、ビルド時間を測ってから広げます。`opt-level = "z"`を全体へ指定する案は採りません。操作速度を落として数字だけ小さくなる可能性があるためです。

長期的には、ViewKitとtext rasterizerの同じ機械語も各アプリへ静的にリンクされています。共有libraryを読み取り専用でmapするか、描画処理をcompositor側へ寄せなければ、ディスクとRAMの重複は残ります。これはABI、更新、障害分離に関わるため、フォントとcodecを片付けた後に進めます。

## mDriver

現在のLinux設定は91個の機能を組み込みにしており、moduleは無効です。Intel、AMD、NVIDIAのGPU driverや複数世代のNIC、Wi-Fi driverを一つのLOAD segmentへ入れているので、使わないdriverも常駐します。

次の段階では、起動に必須なmBoot guest driver、PCI、IOMMU、block backendだけを組み込みに残します。GPU、Wi-Fi、Ethernet、USB storageなどは署名済みmoduleへ分け、検出したデバイスに必要なものだけ読み込みます。広い実機対応を保ったまま常駐量を減らすには、この方法が妥当です。

配布サイズにはkernelとinitramfsの圧縮も効きますが、mBootは今のところELFの`vmlinux`を直接読みます。Linux boot protocolか、mBoot内の展開処理を追加してから切り替えます。圧縮はRAMを減らさないため、module分離より後です。

## ビルド成果物

`mboot/target/debug`には20 MBを超える古い`mbootd`が複数残っています。これらは製品へ入りませんが、作業領域とキャッシュを圧迫しています。削除対象をrepository単位で確認できるコマンドを用意し、ビルド中のtargetや共有cacheを巻き込まないようにします。

最適化中に毎回全体をclean buildする運用もやめます。カーネルだけなら`make update-kernel`で既存イメージを更新できます。次はmBoot、mDriver、サービス、アプリにも個別の入力fingerprintを持たせ、変更のあった成果物と、それを含むimageだけを更新します。全体のstamp一個で再ビルドを決める現在の方式は粗すぎます。

## 作業順

1. ViewKitの埋め込みフォントをrootfs上の共有ファイルへ置き換えます。
2. 画像codecをfeatureに分け、各アプリが必要なものだけを選びます。
3. サービスとアプリのrelease profileをそろえ、代表バイナリでThinLTOを比較します。
4. mDriverの実機driverをmoduleへ分離し、検出したhardwareだけを読み込みます。
5. scheduler、VFS、exec、allocatorのp50、p95、p99を実機で取り、時間の大きい順に直します。

一回の変更で複数の要因を混ぜません。前後のバイナリ、LOAD量、起動ログ、実機の待ち時間を残し、改善しなかった変更は戻します。
