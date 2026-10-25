# 最大常駐量のPOSIX会計（段階225）

native ProcessMemoryReadで計測したRAM参照量をgetrusage / wait4へ接続します。
カーネル変更はありません。RUSAGE_SELFはCPUとRAMをnative fast pathで取得し、
RUSAGE_CHILDRENは認証済み呼び出し元のwait履歴をposix.serviceへ問い合わせます。
CPUとRSSの子集計は一つのRPCで同じ履歴から取得します。

mochiOSのstruct rusageはtimeval二つとlong ru_maxrssの40 byte、maxrssのoffsetは32です。
Cの自己完結したsys/resource.hとRust libcのmochiOS定義を揃えました。
既存SDKのheaderとRust std overlayは再生成していないため、Cビルドでは
user/libc-port/includeをSDKより先に参照する必要があります。
古い32 byteのrusageを使うプログラムは再コンパイルが必要です。

サービス内ではbyteを保持し、libcがKiBへ切り上げます。CPUは従来どおり加算し、
RSSはwait済みの子・その子がwait済みの子孫の最大値とします。兄弟やprocess treeの
RSS合計は返しません。未waitの子や単なるWNOWAIT観測では親の履歴を変更しません。
この拡張fieldの単位と子の集計は[Linux getrusage](https://man7.org/linux/man-pages/man2/getrusage.2.html)に合わせています。

OP_WAIT_CHILD_USAGEはWaitInfo 24 byteとUsage 48 byteの72 byteを返します。
新しいOP_WAITED_CHILD_USAGEはpayloadなしでCPUと最大RSSの48 byteを返します。
CPUのみを使うtimes等の従来の集計opcodeは維持します。
要求の追加payload・flags・Handle添付は拒否し、任意PIDを指定して参照できません。

native会計の失敗や不整合を0値の成功に置き換えません。使用量の取得・変換と
replyが成功するまでwaitイベントと親の履歴を確定しません。
Cの出力にはMemoryCopyを使い、未整列pointerをRust参照に変換しません。
C出力faultはEFAULTで、サービスが回収済みのイベントは復元しません。

ホストでは9 target / 516件が成功しました。認証済み親子の集計、最大値と加算の区別、
WNOWAIT、会計失敗と返信拒否、native error伝播、C出力faultと未整列出力、
C / C++のインストール済みheaderとソースoverlayの重複、広いCPU累計を含みます。
記録はout/checks/mochios-rusage-225-host-final.logです。

KVMでは通常の資格情報とseteuid(501)後のexecの両方でCの試験が成功しました。
8 MiBのtouch、unmap後の自己peak保持、8 / 16 MiBをtouchした子のwait4のRSS、
RUSAGE_CHILDRENがそれぞれのwait後に最大値を返すことを確認しました。
従来の実RAM計測 / Handle権限、C++ thread / exception / filesystem、auxiliary、
memory advice、waitの試験も成功しています。記録はmochios-rusage-225-native.logです。

LLVM本体のProgram.cppも本番設定でコンパイルできました。
clang本体の実行と完全なPOSIX環境は未完了です。


診断ELFを削除して通常managerを再ビルドし、イメージを再生成しました。
通常KVM起動でinput READY、Binder ready、デスクトップとDockを確認し、VMを停止しました。
記録はmochios-rusage-225-normal.log / normal.pngです。
NICのないrunnerのnetwork Failed(-6)、USB driver errno 22、画面中央に起動ログの
一部が残る問題は従来どおり残ります。配布SDKとRust std overlayは再生成していません。
