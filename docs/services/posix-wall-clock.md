# gettimeofdayとnewlib time

newlibの`_gettimeofday_r`が参照する`gettimeofday`本体がruntimeに存在しなかった。
既存の`clock_gettime(CLOCK_REALTIME)`を使うC ABI adapterを追加した。
カーネル、IPC protocol、SDKそのものは変更しない。

既存のclock readerがnative ClockRead、Capability確認、未初期化時計のエラー、
秒数の範囲、nanosecondsとreserved fieldの妥当性を扱う。
adapterはnanosecondsを1000で除算し、秒とmicrosecondsを16byteのtimevalとして
MemoryCopy経由で公開する。POSIX endpointへのRPCは行わない。
成功時はerrnoを保持し、clock readerや出力copyのエラーはそのまま伝播する。
出力がNULLの場合はEFAULTとする。

timezone引数は読み書きしない。これは現代のPOSIX timezone設定機構ではなく、
[gettimeofdayの仕様](https://pubs.opengroup.org/onlinepubs/009604599/functions/gettimeofday.html)
でも非NULL時の動作は未規定となっている。架空のUTC情報は返さない。

ホストの`wall_clock_runtime`と既存`time_memory_runtime`の計7テストで、
microsecondsへの切り捨て、64bit秒、unaligned出力とcanary、不正pointer、
clock reader失敗時の出力保持、errno、timezone pointerを確認した。
実際のMemoryCopy境界をLinuxの自プロセスmemory copyで試験するため、
このホスト試験はprocess_vm_readvが利用可能な実行環境を必要とする。

ネイティブ試験`native_wall_clock.c`は、既存ClockReadとの時刻比較、readonly/null/
overflowする出力先の拒否、実newlib `time()`から`_gettimeofday_r`を経由した呼び出しを
対象とする。

KVMでroot/euid 501の双方の試験が成功し、TLS・実LLVMSupport・ELF PHDRの
既存試験も通った。初回は診断ランチャーがsystem.time.readを持たず、
既存ClockReadのCapability確認でEACCESになった。
診断用のstaged service-manager manifestだけに明示的なrequiresを追加し、
通常のmsign built-in-record生成とイメージ認証を経由して成功系を検証した。
本番のサービスmanifestや権限確認は変更していない。
診断用manifestとverificationは試験後に元のbytesへ戻した。

LLVM buildと並行した起動ではBinder readyがtimeoutした。
既存LLVMジョブのPIDと開始時刻を照合して一時停止し、同じイメージでBinder readyを
確認した後にC API試験へ進んだ。失敗した起動記録は保持している。

証跡は`out/checks/mochios-{clang-pipeline,wall-clock}-231-*`。

元のmanifestとverificationの完全一致を確認し、診断ELFを外した通常イメージで
input READY、Binder ready、Dockの表示を確認した。既存Network readiness失敗(-6)は
未修正。検証用VMは停止し、一時停止した同じLLVMジョブをPID・開始時刻を照合して再開した。
