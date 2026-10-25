# メモリ配置への助言（段階218）

newlibのposix_madvise()とposix-clientの入力検証を追加しました。
NORMAL / RANDOM / SEQUENTIAL / WILLNEED / DONTNEEDを受け付け、
ページ境界、長さ、overflow、対象範囲の割り当てを検証します。
戻り値は0または正のerrnoで、呼び出し元のerrnoを変更しません。

現時点では助言による配置・先読み・解放の最適化を行いません。
POSIXの助言はメモリアクセスの意味を変えてはいけないため、
DONTNEEDでもデータを破棄しません。Linuxの破壊的なmadviseの移植ではありません。
仕様: https://pubs.opengroup.org/onlinepubs/009604399/functions/posix_madvise.html

カーネルにはMemoryRangeCheckを追加しました。引数はbyte addressとlengthだけで、
現在のプロセスの予約領域または保持されたuser page entryが範囲全体を覆うか調べます。
PROT_NONEの領域も割り当てとして認めます。ゼロ長はメモリへアクセスしません。
権限・内容・residency・backingを変更せず、I/Oも行いません。
POSIX advice、errno、サービス名、他プロセス指定はABIへ持ち込みません。
posix-clientがPOSIXの検証とnative errorからの変換を行い、サービスRPCは不要です。

ホスト試験は7 target / 19件成功しています。実装本体を使い、入力不正時にnative
queryへ進まないこと、穴、範囲overflow、現在の所有者だけを調べること、状態を
変更しないこと、native失敗の伝播を確認しました。
KVMでは通常identityとseteuid(501)後のexecの両方で、C ABIから全助言、errno保持、
PROT_NONE、unmapの穴、データ保持、FDを閉じたfile-backed mappingを確認しました。
既存C++ vector / regex / pthread / exception / filesystemと補助vector試験も成功しました。

記録はout/checks/mochios-memory-advice-218-{host-final,native}.log、artifacts.jsonです。
診断ELFを除去し通常managerへ戻してイメージを再生成後、KVMでinput READY、
Binder ready、デスクトップとDockを確認しました。normal.logとnormal.pngへ保存し、
今回起動したVMは停止しました。NICのないrunnerのnetwork Failed(-6)と
USB driver errno 22は別件として残ります。画面中央には起動ログの一部も残っています。
配布SDKとRust std overlayは再生成していません。
完全なPOSIX環境とclang / ld.lldの実行確認は引き続き未完了です。
