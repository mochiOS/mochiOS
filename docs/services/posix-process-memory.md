# Process HandleによるRAM参照量の取得（段階224）

native ProcessMemoryReadを追加しました。引数はProcess Handle（0は呼び出し元）、
出力pointer、厳密な構造体サイズ16 byteです。current_bytes / peak_bytesを返します。
他プロセスの情報には既存のWAIT権限を持つ明示的なProcess Handleが必要です。
新しいPID / endpoint / 名前検索やサービス固有の識別子は受け付けません。
無効Handle、権限不足、object種類、出力faultはnative Errorを保持します。
情報が未計測または不整合ならNotReadyであり、0値の成功に置き換えません。

mochi-user-syscall::execution::memory_usageはNone / Some(Handle)を受け、
Some(0)をsyscall前に拒否します。異常な成功値やcurrent > peakも拒否します。
POSIXのrusage、親子関係、KiBへの変換、子の最大値集計はABIへ含めていません。
ru_maxrssへの接続はまだ未完了です。

ProcessObjectが独立した観測状態を持ち、VMルートはそれをWeak参照で通知します。
ルートの最大値をそのままprocessの最大値に流用しません。共有ルートに後から
参加したprocessは、その時点のcurrentから計測し、参加前のpeakを継承しません。
execによるroot交換でもprocessのpeakは維持し、古いrootの後続変更は加算しません。
通常のleaf更新はallocationせず、既存のcounter更新に続いて観測状態だけ更新します。
計測用のallocation / 登録が失敗した場合も未計測とし、数値を捏造しません。

processが実行用アドレス空間の所有を手放した時点で監視を外し、currentを0にします。
独立したVM pinによる物理破棄の延期とprocessの参照寿命は別です。
保持したProcess Handleからpeakを読めても、それを理由に古いページテーブルや
RAM frameを生存させません。破棄処理が失敗した場合に成功したことにはしません。
同じphysical root addressの再利用も別の登録であり、古いprocessの観測へ混ざりません。

hostでは8 target / 22件が成功しています。共有rootへの参加時点、root交換、
終了後の統計保持、不整合、Handleの権限と種類、サイズ、未整列のC出力、fault、
従来のCPU会計とprocess lifecycle / image revisionを確認しました。
CPU / lifecycleのfixtureではRAMの未計測を明示的にNoneとしています。
RAMの振る舞いは実際のresidency実装を利用する別試験で確認しています。
記録はout/checks/mochios-memory-read-224-host-final.logです。

実機試験で、実行記録の回収後も物理rootの寿命を追ってcurrentが残る問題を検出し、
上記の所有終了処理を追加しました。さらに既存の回収処理では、終了threadがqueueを
離れてからkernel stackをrootから外すまでの間も、paging側が破棄を拒否します。
その間にProcessをtableから外して破棄責任を失わないよう、stackが残る場合は
回収をdeferして所有者を保持する確認を追加しました。

KVMで4ページのtouch後のcurrent増加が厳密に16 KiB、PROT_NONEで量が変わらないこと、
unmap後のcurrent復帰とpeak保持を確認しました。self / WAIT Handleからの取得、
WAITを外したHandleの拒否、厳密な出力サイズとpointer faultも確認しています。
終了した子の実行記録の回収後にはcurrentが0となり、保持したHandleでpeakを読めました。
従来のC/C++ fork / wait / memory advice / pthread / exception / filesystem /
auxiliary試験も成功しました。記録はmochios-memory-read-224-native.logです。
hostのCPU競合中のBinder timeoutはnative-attempt1.log、監視の終了処理の修正前の
失敗と診断はnative-attempt2.log / native-attempt3.logに残しています。

診断ELFを外して通常managerを再ビルドし、新カーネルでinput READY、Binder ready、
デスクトップとDockを確認しました。normal.log / normal.pngへ保存し、VMは停止しました。
配布SDKとRust std overlayは再生成していません。NICのないrunnerのnetwork Failed(-6)、
USB driver errno 22、画面中央に起動ログの一部が残る問題は別件として残ります。
完全なPOSIX環境、ru_maxrss、clangの実行はまだ未完了です。
