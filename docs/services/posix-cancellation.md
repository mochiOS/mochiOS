# pthreadキャンセルの対象と寿命

`pthread_cancel`の対象はプロセス内の世代付きpthreadトークンです。
要求を設定するためにPIDや別プロセスのFDを使ったり、サービスを検索したり
しません。キャンセル要求と待機世代の通知はランタイム内で処理します。

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
またはdeferredなら、この非同期通知は送らず、既存の待機世代だけを通知します。

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
