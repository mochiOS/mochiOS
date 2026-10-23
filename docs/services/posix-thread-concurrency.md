# pthreadの並列度ヒント

`pthread_setconcurrency`・`pthread_getconcurrency`をnewlibランタイムへ実装しました。
初期値は0です。非負の指定値をプロセス内のAtomicI32へ保持し、getterはその値を
返します。負値はEINVALを直接返し、以前の値とerrnoを維持します。
0は実装側に並列度を任せる指定です。

[POSIXの規定](https://pubs.opengroup.org/onlinepubs/009696799/functions/pthread_getconcurrency.html)
では、この値は希望する並列度のヒントであり、実際に同時実行するスレッド数の
保証ではありません。ユーザースレッドを多重化しない実装でも、指定値を保持して
getterで返すことを要求しています。mochiOSのネイティブ1対1スレッドモデルでは
新しいスレッド、待機者、CPU、スケジューラ枠を予約しません。スレッド数や
rlimitの制限を回避する機能でもありません。取得値をオンラインCPU数や
現在走行中のスレッド数の測定値として使うことはできません。

処理は割り当て・ロック・TLS初期化・キャンセル確認・IPC・syscallを必要としません。
個別の指定値以外のメモリを同期する意味はないため、単一atomicのrelaxed操作を
使います。カーネルやPOSIXサービスへ新しい状態や専用APIを追加していません。

## 検証とSDK表面の監査（166）

`services/posix/tests/pthread_concurrency.rs`で実モジュールをコンパイルし、初期値、
0への復帰、INT_MAXを含む非負の値、負値での状態維持、errno、8スレッドからの
並行呼び出しを確認します。C呼び出し側は`pthread_concurrency_abi.c`です。
ホストのpthreadヘッダーを使って実モジュールのstaticlibとリンクし、別スレッドと
値を共有することも確認します。ホストlibcの同名実装を誤って試さないよう、
実行体内に両方の定義シンボルが存在することを確認してから実行します。

mochiOS用ランタイムも独立した出力先でビルドし、同じC呼び出し側をSDKヘッダーと
新しいランタイムに対してクロスリンクしました。そのELF内にも両シンボルが
定義されることを確認しています。これはビルド・リンクの確認であり、mochiOS上で
そのELFを実行したという意味ではありません。変更は純粋なプロセス内atomic処理なので、
今回はKVM起動や通常イメージの更新を行っていません。

SDKで現在有効な宣言を独立ビルドのランタイム・SDKのlibc/libmと照合すると、
pthread.hの91宣言のうち未定義だった2シンボルは解消しました。
`pthread_getname_np`・`pthread_setname_np`・`pthread_yield`はまだ未定義です。
sched.hでは`sched_getcpu`が未定義です。また、現在のヘッダーでは
`_POSIX_THREAD_PRIORITY_SCHEDULING`・`_POSIX_PRIORITY_SCHEDULING`が有効でなく、
その条件内の宣言はこの比較に含まれません。この監査は宣言とシンボルの照合に限り、
各APIの意味論・POSIX全体の適合率・VimやWebKitの動作を示すものではありません。

生成済みSDK内のランタイム、Rust std、通常起動の実行体は更新していません。
確認対象のランタイムは`out/continuous-native-check/`以下の独立ビルドです。
監査の入力アーカイブとSHA-256は
`out/checks/mochios-sdk-symbol-audit-166-final.json`へ保存しています。

ホスト全体は651ターゲット・3,115件成功、失敗0件です。
ログは`out/checks/mochios-posix-host-audit-166.log`、クロスリンクしたELFは
`out/continuous-native-check/native-probes/pthread-concurrency-166.elf`です。
