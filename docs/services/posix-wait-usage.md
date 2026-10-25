# 待機と選択した子の使用量

現在のRSS対応は[段階225](posix-rusage-memory.md)を参照してください。
以下は段階222の実装・検証記録です。

wait4 / wait3と、従来不足していたwait / _waitのC entry pointを追加しました。
wait4はusageがNULLなら既存waitpidを利用します。それ以外は新しい
OP_WAIT_CHILD_USAGEで、段階221のサービス内の待機・会計処理へ接続します。
kernel syscallや名前によるサービス発見は追加していません。

requestは従来と同じguarded waitのselector / options / thread signal epochです。
senderの認証とprocess imageの検証は、従来のwaitと同じIPC経路で行います。
replyは24 byteのWaitInfoと40 byteのCPU使用量です。生ポインタ、FD、自己申告の
認証情報は送りません。使用量は子自身と、その子が実際にwaitした子孫の合計で、
native tickの精度をu128で保持し、libcでtimevalへ変換します。

nohangの空イベントに使用量やUIDが付いたreply、不正な長さ・予約byte・単位は
拒否します。サービスはreplyのencodeとIPC返信が成功するまで子を回収しません。
pending waitにも新形式を保持し、配送前にendpoint / image / signal epochを
再検証します。信号による中断・再試行とpthread cancellationは既存waitと同じです。

statusとrusageはnative MemoryCopyでC storageへ書き込みます。未整列pointerを
Rust参照へ変換しません。PID / CPU時間の変換失敗は出力前に検出します。
WNOHANGでイベントがない場合はどちらの出力にも触れません。
サービスで既に回収した後のC出力faultはEFAULTになり、イベントを復元しません。

現時点のmochiOS newlibのrusageは二つのtimevalだけを持つ32 byteのABIです。
RSSやfault countの測定値を捏造せず、その範囲だけを書き込みます。
LLVMのProgram.cppが要求するru_maxrssはまだ未対応です。

hostではprotocol 497件、wire / client 2件、C出力3件、pending wait境界1件、
C / C++ header 1件の計504件が成功しています。C出力のfault試験では
自プロセスのprocess_vm_readvを利用するため実行権限が必要です。
記録はout/checks/mochios-wait4-222-host-final.logとartifacts.jsonです。

KVMでは通常の資格情報とseteuid(501)後のexecの両方でCのwait4を実行しました。
pipeで終了を止めた子に対するWNOHANGがstatus / usage / errnoを変更しないこと、
CPUを使って終了した子の非ゼロ使用量と終了値、ECHILD、EINVAL、wait3 / wait、
usageがNULLのwait4を確認しました。従来のC++ / auxiliary / memory advice試験も
両方で成功しました。記録はmochios-wait4-222-native.logです。

診断ELFを除去し通常managerを再ビルドして、更新したPOSIXサービスとともに
イメージを再生成しました。KVMでinput READY、Binder ready、デスクトップとDockを
確認し、normal.log / normal.pngへ記録しました。今回のVMは停止しました。
kernelは段階218のままで、配布SDKとRust std overlayは再生成していません。
NICのないrunnerのnetwork Failed(-6)、USB driver errno 22、画面中央の起動ログの
一部が残る問題は別件として残ります。完全なPOSIX環境とclangの実行は未完了です。
