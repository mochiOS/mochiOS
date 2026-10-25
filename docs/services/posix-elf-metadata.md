# 読み取り可能なELFプログラムヘッダー（段階228）

従来のELFローダーは、最初のLOADのvaddr - offsetへe_phoffを足した値を返していました。
プログラムヘッダーがLOADに含まれないELFでは、このアドレスは未マップです。
このままでは、AT_PHDRを使うユーザー空間runtimeがTLS template等を調べられません。

汎用ELFローダーで、完全なプログラムヘッダー表がLOADのfile-backed範囲に含まれるか
調べます。含まれる場合は、そのLOADの実際のoffsetからアドレスを計算します。
最初のLOADと決めつけません。別のLOADと同じページを共有する場合は、その後の
上書きや権限変更に依存しないコピーを使います。

LOADに含まれない表は、LOADと重ならない位置へ読み取り専用・実行不可でコピーします。
配置は既存のuser-image window内に限定し、先頭LOADの前に収まらなければ最後のLOADの
後を使います。専用の配置領域が取れない場合はResourceUnavailableを返します。
ELFが持つ未知のheader entryやstride内の追加bytesも、そのまま保持します。
表全体の範囲、加算・乗算・page alignmentを、ページテーブル変更前に検査します。
探索は線形で、各候補ごとに全LOADを走査する処理はしません。

カーネルへ追加したのはELFメタデータの配置だけです。AT_*番号、TLS templateの解釈、
pthread、POSIX、サービスやディレクトリの名前は追加していません。
[ELF gABIのProgram Header](https://gabi.xinuos.com/elf/07-pheader.html)と同じく、
LOADのfile offsetとvirtual addressを区別します。

最初のKVM確認では、コピーを低いcanonical addressへ置いたことで既存forkが失敗しました。
新しいページテーブルにもsupervisor mappingが残る領域だったため、既存のLOADと同じ
user-image windowを守るよう修正しました。ホストfixtureにもこの配置境界を追加し、
修正後に実際のfork / child execが再び成功することを確認しています。
最初の失敗はout/checks/mochios-elf-metadata-228-native-attempt1.logへ保存しました。

ホストでは5 target / 22件が成功しました。実パーサーと配置処理を使い、LOAD外の表、
最初以外のLOAD内の表、非ゼロoffset、共有page、大きなstrideとその末尾切れ、
大きな表、メタデータ・各LOADの独立したmapping失敗を確認しました。
元の表と公開するbytesの一致、コピーのread-only / non-executable属性、配置境界も
検証しています。記録はmochios-elf-metadata-228-host-final.logです。

KVMでは、既存linker.ldのELFとFILEHDR / PHDRSをLOADに含めたELFを実行しました。
それぞれ通常の資格情報とseteuid(501)後のexecの両方で、AT_PHDR / PHENT / PHNUM /
ENTRYの内容、表全体と元ファイルの一致、faultableな出力で表へ書き込めないことを
確認しています。従来のthread-base / RAM / passwd / rusage / C++ / auxiliary /
memory adviceも成功しました。記録はmochios-elf-metadata-228-native.logです。
配布SDKとRust std overlayは再生成していません。

これはTLSの初期化完了を意味しません。PT_TLS segmentの出力、templateの検証とコピー、
メインスレッドとpthreadごとの領域、破棄処理は引き続き必要です。
clang / ld.lldの実行と完全なPOSIX環境も未完了です。


診断用の二つのELFを削除し、通常managerを再ビルドしてイメージを再生成しました。
新カーネルでinput READY、account endpoint handoff、Binder ready、デスクトップとDockを
確認しました。mochios-elf-metadata-228-normal.log / normal.pngへ保存し、VMは停止しました。
NICのないrunnerのnetwork Failed(-6)とUSB driver errno 22は従来どおり残ります。
