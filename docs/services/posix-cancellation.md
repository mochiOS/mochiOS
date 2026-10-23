# pthreadキャンセルの対象と寿命

`pthread_cancel`の対象はプロセス内の世代付きpthreadトークンです。
要求を設定するためにPIDや別プロセスのFDを使ったり、サービスを検索したり
しません。キャンセル要求はランタイム内で設定します。ローカルな待機通知に加え、
POSIXサービスの同期IPC待機は、明示的に委譲したThreadHandleで解除します。

対象の確認だけを行ってからロック外で要求を書き込むと、終了・回収・
スロット再利用の間に別のスレッドをキャンセルする競合が発生します。
`thread_reference::with_live_token`は、既存のネイティブ参照ロックを保持して
対象を再確認し、短いローカル操作を適用します。RPCやアプリ関数はこの
ロック内で実行しません。forkの複製とupcallの再入も操作中は保留します。

回収は、同じロックでネイティブ参照を閉じてからTCBをFREEにします。
したがって要求の書き込みが終わる前に新世代へ再利用されません。
FREEからSTARTINGへ移る際の旧世代が見える区間も考慮し、ネイティブIDが
公開済みであることを確認してから、世代を再確認します。
`pthread_create`が公開した直後でtrampolineが未実行の対象は受け付けます。
無効な世代、未公開のスロット、終了・回収済み対象はESRCHです。

## 非同期キャンセル

`pthread_setcanceltype(PTHREAD_CANCEL_ASYNCHRONOUS, ...)`も提供します。
POSIX環境の汎用upcallが未設定のスレッドでは、変更前にENOTSUPを返します。
deferredが初期値で、要求だけでは方式を変えません。

非同期要求は同じ世代保護の中で設定し、ランタイム所有のSIGNAL Thread Handleで
`ThreadInterrupt`を呼びます。これは既存の汎用通知であり、専用カーネルABIや
アプリケーション用のシグナル番号は追加しません。対象のキャンセルが無効、
またはdeferredなら、この非同期通知は送りません。有効なdeferred取消にも、
後述するサービスの待機解除は送ります。取消無効中はサービスの待機を解除せず、
要求を保留します。

汎用upcallは、保留された有効な非同期要求を確認してからシグナル処理へ進みます。
ネイティブ配送ガードや資源の準備が残る間にcleanupを開始しません。
`pthread_cancel`自身の参照ロックも解放してから自分への要求を処理します。
状態・方式の変更は以前の値を検査付きで出力してから確定し、有効な非同期方式へ
移った時には保留要求を確認します。cleanupの方式復元も同じ確認を行います。
終了処理はキャンセルを無効化し、既存のcleanup・TLS destructor経路を使います。

通知キューが満杯の場合も、そのキューには既に汎用通知があるため、次の通知入口で
要求を確認できます。対象消失や権限不足のエラーとは区別し、それらを成功へ
置き換えません。

[POSIXの非同期キャンセル安全性](https://pubs.opengroup.org/onlinepubs/9799919799/functions/V2_chap02.html)
で必須なのは`pthread_cancel`・`pthread_setcancelstate`・`pthread_setcanceltype`です。
他の関数の実行中に非同期キャンセルされる場合は未定義とされており、mallocや
mutexなど全APIを非同期キャンセル安全にする保証はしません。計算処理で使い、
資源を扱う区間はキャンセルを無効化するかdeferredへ戻します。

ホストの`services/posix/tests/thread_reference.rs`は、処理を途中で止めて
回収を競合させ、要求が新世代へ残らないことを確認します。
join・sleepの既存キャンセル試験も同じ呼び出し経路へ接続します。
`native_cancel_lifetime.c`は実newlibで、再利用後の旧トークン32件、公開直後の
キャンセル32件、プロセスCPU待機・キャンセル・forkの既存試験を実行します。
`asynchronous_cancellation.rs`は通知条件・保留・エラー区別をホストで確認します。
`native_async_cancel.c`は全シグナルをブロックした計算ループ、無効状態からの
再有効化、方式変更、自己キャンセル、cleanup方式の復元を各4回実行し、
cleanup・TLS destructorが一度だけ呼ばれることと既存の遅延キャンセルを確認します。

## 同期IPCで待つ取消ポイント

`OP_THREAD_WAIT_INTERRUPT`は、参照保護ロックの解放後に送るユーザー空間の要求です。
WAIT|TRANSFERへ縮小したThreadHandleを一つ添付し、サービスがネイティブの
生存・所有者を確認して対象の待機世代を進めます。要求に自己申告のPID/TIDを載せず、
アプリ用signalも使いません。待機登録前に届いても、古い世代の要求を拒否できます。
POSIX endpointのない起動初期のランタイムには新たなサービス依存を要求しません。
元の対象が終了した、または待機世代が未登録なら、別の寿命の対象を探し直しません。
これは前節の非同期ThreadInterrupt通知の失敗処理とは区別します。

`waitpid`・`waitid`・`sigwait`・`sigwaitinfo`・`sigtimedwait`は、入口と再試行の前、
配送世代取得の直後で取消を確認します。解除されたIPCがEINTRを返した後に
無条件で再待機して、保留取消を見落とすことを防ぎます。エラー終了にも確認を置きます。
待機から成功した終了情報・シグナルを受け取った場合は、取消が後から届いても
その結果を隠しません。`sigwait`の直接エラー番号を返す規定、`sigwaitinfo`などの
errno経路、`sigtimedwait`の絶対化済み期限、SA_RESTARTの既存の扱いを維持します。
`waitpid(..., WNOHANG)`も入口で保留取消を確認します。

`service_wait_cancellation_runtime.rs`と`signal_receive_runtime.rs`は実際のループを
制御したバックエンドで動かします。入口前、世代取得直後、EINTR応答時の取消で
イベントを受け取らず終了し、成功応答後の取消では結果を公開すること、取消無効中は
処理を継続することを確認します。子待機13系列、シグナル待機15系列を含みます。

`native_service_wait_cancel.c`は実C APIで5種類×5モードを試します。
deferred取消・非同期取消・入口前の保留取消は、子が終了する／シグナルを送る前に
joinできることを確認します。その後、別の待機者が同じ終了状態やシグナルを取得し、
取消された要求がイベントを消費しないことを確認します。取消無効中と通常成功では、
PID・終了状態・siginfo・選択シグナルの出力とerrnoを確認します。
既存のwaitidのselector・WNOWAIT・停止／継続・終了状態の回帰試験も同じ実行体で行います。

待機APIの接続（162）のホスト検証は647ターゲット・3,104件成功、失敗0件です。
テスト用バックエンドが再試行を即座に失敗させるよう制限した後も、対象2ターゲットを
再実行して成功しました。KVMでは既存waitid試験と追加25ケースが成功しています。
ログは`out/checks/mochios-posix-host-audit-162.log`と
`out/checks/mochios-service-wait-cancel-162.log`に保存します。
これは待機の取消に関する確認であり、POSIX全体の適合やVim・WebKitの動作を
確認したものではありません。カーネルの変更とSDKの再生成は行っていません。

通常起動では、input.serviceの163 MBのIME辞書読み込み・解析と設定読み込みが
共通の5秒期限を超えました。最初の失敗ログは
`out/checks/mochios-normal-162-input-timeout.log`として保存しています。
サービス管理側でinputだけの期限を15秒に分け、他サービスの5秒期限は維持しました。
起動失敗を成功扱いにせず、READY受信とプロセス終了の監視も継続します。
修正後はBinderのREADY、Dock・ステータスバーの表示を確認し、通常起動のログと画面を
`out/checks/mochios-normal-162.log`・`out/checks/mochios-normal-162.png`に保存しています。
この通常起動確認は新しいPOSIXサービスと既存のデスクトップ実行体を組み合わせたもので、
全アプリを新しいランタイムで再リンクした確認ではありません。
