# 起動時のELF補助情報（段階217）

user/libc-port/include/sys/auxv.hとnewlib runtimeにgetauxval()を追加しました。
argvや環境変数から実行名を推測せず、起動時に渡されたELF補助情報を読みます。
AT_EXECFNは起動名のbytesであり、実行imageの認証やファイルアクセスの権限ではありません。
kernelへのパス問い合わせ、サービス名検索、新たなsyscallは追加していません。

runtimeは初期vectorの位置と要素数をconstructorより前に保持します。
取得はその範囲内だけをnative MemoryCopyで行い、application memoryへのRust参照を作りません。
値0の項目が存在する場合はerrnoを保ち、未知の項目は0 / ENOENTです。
overflowや既存tableの読み取りfaultも成功扱いにしません。

従来のstack encoderはUID / GID / AT_SECUREへplaceholderを入れていました。
POSIX image-startの反映後、ユーザー空間の認証済みsessionから資格情報を取得して、
実際の初期vectorを更新します。constructorとmainが動く前に完了し、
後のseteuid等で起動時のsnapshotを変更することはありません。
AT_SECUREは起動時のreal / effective UIDまたはGIDが異なる場合に設定します。
set-id imageや動的loader全体の対応を完了したという意味ではありません。
AT_CLKTCKの固定値も、既存のsysconf(_SC_CLK_TCK)と同じPOSIX単位へ修正しました。

ホストでは実装本体を使った20件の関連試験が成功しています。MemoryCopyのhost境界は
自プロセスへのprocess_vm_readvで代替し、読み取りfaultと実際のvector書き換えを検証しました。
そのため、hostのsandboxで同syscallが拒否される場合は試験の実行権限が必要です。

KVMでは通常identityと、seteuid(501)後にexecするidentityの2ケースを実行しました。
意図的に異なるargv[0]に対するAT_EXECFN、page size、clock単位、各UID/GID、
AT_SECURE、0値とerrno、起動後の資格情報変更に対するsnapshot維持を確認しています。
既存のC++ vector / regex / pthread / exception / filesystem試験も両ケースで成功しました。

記録はout/checks/mochios-auxiliary-217-{host-final,native,normal}.log、normal.png、
artifacts.jsonです。診断ELFを除去し、通常managerで再起動後のinput READY、Binder ready、
デスクトップとDockを確認しました。VMは停止済みです。配布SDKとRust std overlayは再生成していません。
NICのないrunnerのnetwork Failed(-6)とUSB driver errno 22は別件として残ります。

LLVMのgetMainExecutableへの接続と、残るfilesystem locality / memory adviceの対応、
clang本体の動作確認は引き続き必要です。完全なPOSIX環境のgoalは継続しています。
