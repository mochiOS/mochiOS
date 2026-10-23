# POSIXプロセスCPUクロック

`clock_getcpuclockid(pid, &id)`、取得したIDでの`clock_gettime`と
`clock_getres`をユーザー空間で提供します。カーネルABIは追加しません。
`clock_getcpuclockid`はエラー番号を直接返し、errnoを保存します。

IDは組み込みクロックとpthreadクロックの間の値域でプロセスを識別します。
取得済みIDをforkでコピーした場合も、取得時に選択したプロセスを指します。
`CLOCK_PROCESS_CPUTIME_ID`のような呼び出し元を示す定数とは区別します。
現在の32ビットPOSIX PIDで表せないプロセスにはIDを発行しません。

自プロセスのサンプルは既存の`ProcessExecutionRead`を直接使います。
他プロセスのサンプルは`OP_PROCESS_CPU_CLOCK`でPOSIXサービスへ依頼します。
他プロセスでは登録・終了状態と資格情報の確認が必要なため、この経路はRPCです。
一般の時計読み出しと組み込みの自プロセスCPUクロックの経路は変えません。

サービスは起動登録時に委譲されたProcessHandleだけを使います。
要求中のPIDでカーネルからHandleを取得したり、呼び出し元にProcessHandleを
渡したりしません。資格情報も要求の自己申告値ではなく登録済みセッションを使います。
自プロセス以外は、実効UIDが0、または呼び出し元の実UID・実効UIDのいずれかが
対象の実UID・実効UIDと一致する場合に観測を許可します。

サービスは生のuser/kernel ticksと周波数を返します。libc境界で合計と
timespecへの変換を行い、オーバーフロー・不正な周波数・バックエンドエラーを
ゼロ時間に置き換えません。終了済み対象の取得はESRCH、終了済みクロックIDの
読み出しはEINVALです。分解能の取得でもIDの生存と権限を確認します。

取得したIDは`timer_create`でも利用できます。`OP_TIMER_CREATE_PROCESS_CPU`は
既存の作成フレームに計測対象のPIDを添え、通知先とタイマーIDの所有者には
認証済みの呼び出し元を使います。自プロセスのIDは既存のProcessCpu計測へ
まとめ、別プロセスは登録済みのHandleと資格情報を確認して計測します。
リアクターは所有者・時計・対象ごとにサンプルを共有します。

対象がexecしてもCPUクロックの寿命は続きます。対象の終了を観測した時は、
その対象を計測するタイマーと未配送通知を除去します。タイマー所有者自身の
exec・終了による破棄とは分離しています。資格情報の変更で対象を観測できなく
なった場合はエラーを返し、サンプルを捏造したり別対象のタイマーを止めたり
しません。所有者は観測権限を失ったタイマーでも削除できます。

ホスト試験は`services/posix/tests/process_clock*.rs`、
newlib・実IPC・forkの統合試験は`native_process_clock.c`です。
これらのIDを使うclock_nanosleep、SIGEV_THREAD、POSIX全体への適合は
未完了です。

## CPU時間での待機処理

`services/posix/src/cpu_sleep.rs`に、サービス側でプロセスCPU時間を
観測して完了する待機レジストリを追加しています。
`OP_PROCESS_CPU_SLEEP`で要求し、`posix-client::begin_process_cpu_sleep`は
サンプルから決めた絶対期限と専用の完了Handleを返します。
libcへの接続前であり、`clock_nanosleep`からはまだ利用できません。

登録時に認証済み所有者のイメージと、選択対象の資格情報・生存を確認し、
相対時間はその時のCPUサンプルから絶対期限へ変換します。ゼロ時間や
過去の絶対期限は即時完了として扱い、タイマーのdisarmとは区別します。
呼び出し側でCPU時間をポーリングしたり、アプリのシグナルを予約したり
する設計にはしていません。

完了チャネルは所有権付きのアダプターへ任せ、接続切断・登録失敗・
送信失敗・レジストリ破棄時に解放します。送信できない場合は結果を
保持して次の観測で一度だけ再試行します。対象の終了・権限喪失・計測失敗は
エラー完了とし、所有者のexecは完了通知前に再確認して古い待機を破棄します。
登録上限は所有者ごとに128件、サービス全体で4096件です。
実際の登録数はネイティブ資源の空きにも制限されます。現在の汎用Streamは
システム全体で64枠のため、POSIXの登録上限より先にENOMEMとなり得ます。

ネイティブアダプターは既存の`StreamPairCreate`を使います。
読み取り端にREAD・WAITだけを付けて明示的にIPC転送し、
WRITE・CONTROL・DUPLICATE・TRANSFERは委譲しません。
サービスは10ms間隔でCPU時間を観測し、8バイトの完了レコードを
`StreamWrite`の全量・非ブロッキング指定で送って書き込み端を閉じます。
カーネルABIの追加やPOSIXシグナルの予約はありません。

ホスト試験は`services/posix/tests/support/cpu_sleep.rs`と
`services/posix/tests/cpu_sleep_ipc.rs`です。
実IPCとネイティブHandleの診断は`native_cpu_sleep.rs`です。
libcのキャンセル・fork時の参照解放とnewlibからの呼び出しは、
ランタイム接続後に実OSで確認する必要があります。
