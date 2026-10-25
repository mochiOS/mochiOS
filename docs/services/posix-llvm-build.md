# LLVM本体のクロスビルド（段階216、継続中）

[段階215のC++ランタイム](posix-llvm-runtime.md)を使い、mochiOS向けclang / ld.lldの
構成処理を通しました。clang本体が動作したという記録ではありません。

## 入力と構成

LLVM sourceは18.1.8です。build-host-toolsにはホストで実行するllvm-tblgen / clang-tblgen、
build-runtimesには段階215で作るmochiOS向けC++ライブラリが必要です。
出力先の既定はout/llvm-portです。

```sh
cmake -S out/llvm-port/src/llvm-project-18.1.8.src/llvm \
  -B out/llvm-port/build-host-tools -G 'Unix Makefiles' \
  '-DLLVM_ENABLE_PROJECTS=clang;lld' -DLLVM_TARGETS_TO_BUILD=X86 \
  -DCMAKE_BUILD_TYPE=Release -DLLVM_INCLUDE_TESTS=OFF \
  -DCLANG_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
  -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF \
  -DLLVM_ENABLE_TERMINFO=OFF -DLLVM_ENABLE_LIBXML2=OFF
cmake --build out/llvm-port/build-host-tools --parallel 4 --target llvm-tblgen clang-tblgen
MOCHIOS_LLD=/path/to/host/ld.lld scripts/configure-llvm-target.sh \
  out/llvm-port/src/llvm-project-18.1.8.src
cmake --build out/llvm-port/build-target --parallel 3 --target clang lld
```

MOCHIOS_LLDにはホストのld.lldかrust-lldを渡します。出力内のld.lld symlinkから
ELF linkerとして実行します。clangドライバが未対応targetでホストgccを呼ぶ経路は使いません。
CC / CXXにはクロスコンパイル可能なホストclangを指定します。

SDKは読み取り入力です。既定のruntime archiveはSDK/lib/libmochi_user_newlib_runtime.aです。
今回はSDKを再生成せず、MOCHIOS_RUNTIME_ARCHIVEでstandalone release runtime、
MOCHIOS_REENT_OBJECTでそのruntimeに別途必要なnewlib_reent.oを指定しました。
起動コードは現在のuser/runtime/crt0.Sから別出力先へコンパイルします。

生成するCMake platformはmochiOSとし、UNIXのAPIはユーザー空間のPOSIX層から提供します。
Linuxを名乗る設定はしません。現在のビルドは静的リンクに限定し、共有ライブラリ対応を
完了扱いにはしていません。CMakeのAPI検出では実際にコンパイルとリンクを行います。
STATIC_LIBRARYだけのtry_compileで未提供関数を検出成功にする設定は使いません。
設定の再実行は--freshで古い検出結果を破棄します。

## ここまでの検証と不足

hostのTableGen生成器はビルド成功しました。target構成処理も成功し、実リンクで32 / 64 bitの
C++ atomicを確認しています。最初のatomic検出失敗は、libmの検索pathが抜けていたためです。

LLVMのCrashRecoveryContextが要求するsysexits.hは、newlib内にあるBSD headerを
ライセンスごとuser/libc-port/includeへ公開しました。HAVE_SYSEXITS_Hの再検出後、
同headerを参照する実際のLLVM objectのコンパイル成功を確認しました。

段階216の全体ビルドはllvm/lib/Support/Unix/Path.incの次の不足で停止しました。

- getMainExecutableのmochiOS向け実装
- statvfsのfield名と、ローカルmount判定の扱い
- mapped_file_regionの不要ページを伝えるAPI

自己申告argvだけで実行imageの位置を決める対応、kernelにpathnameを照会する専用API、
未提供関数を成功に見せるstubは追加していません。
これらを解決した後も、残りのコンパイル・リンク・clang起動・Cのコンパイル・生成物の実行を
実際に確認する必要があります。

記録はout/checks/mochios-llvm-host-tools-216-{configure,build}.log、
mochios-llvm-target-216-script-final.log、mochios-llvm-target-216-build-next.logです。

## 段階219: Path.cppの移植

configureスクリプトからllvm-18-mochios-path.patchを適用します。適用済みの場合は
reverseのdry-runで確認し、異なるsourceへの不一致は明示的に失敗します。
getMainExecutableは段階217のgetauxval(AT_EXECFN)とrealpathを使用します。
argv[0]へのfallbackや、kernelへの実行パス照会は追加していません。

mochiOSのstatvfsはlocal/remote metadataを現在提供しません。is_localは既存の
statvfs / fstatvfsで対象を検証した後にoperation_not_supportedを返します。
存在しないMNT_LOCAL bitや、全mountがlocalという仮定で成功に見せません。
LLVM独自のlocality判定は未対応として残ります。

mapped_file_regionは段階218で追加したPOSIX_MADV_DONTNEEDを検出し、
既存のposix_madvise分岐を使用します。内容の破棄や配置最適化は行いません。
新しいtarget構成処理は成功し、生成された本番のdefines / includes / flagsで
LLVM本体のPath.cppをコンパイルできました。全体ビルドは再開しましたが、
clang / ld.lldのリンクとmochiOS上での実行、Cのコンパイルはまだ未確認です。

記録はout/checks/mochios-llvm-target-219-{configure,build}.log、
mochios-llvm-path-219-build.logです。Path.cppの単独試験は全体ビルドのobjectを
上書きせず、out/checks/mochios-llvm-path-219.oへ出力しました。

## 段階220: 待機ヘッダーの修正

全体ビルドはProgram.cppで停止しました。sys/wait.hのソース側とSDK側に同じ
include_nextラッパーが重なると、共通のinclude guardで基礎の宣言が消えていました。
wait / waitpid、status macro、waitidの宣言を自己完結させました。
sys/syslimits.hも既存newlibのBSD headerをライセンスごと公開し、POSIXの
引数サイズ最小値_POSIX_ARG_MAXを追加しました。実装値ARG_MAXは変更しません。

SDKを直接変更せず、インストールされたheaderだけの場合とソースoverlayを
重ねる場合の両方をC / C++でコンパイルしました。既存wait出力試験も含め5件成功です。
後者のhost試験は自プロセスへのprocess_vm_readvの実行権限を必要とします。
LLVMのProcess / Signals / Memory / Threading / DynamicLibraryは本番設定で
コンパイルできました。Program.cppにはwait4とru_maxrssの不足が残ります。
記録はout/checks/mochios-wait-headers-220-host-final.logと
mochios-llvm-{process,program,signals,memory,threading,dynamiclibrary}-220-build.logです。

## 段階221: 選択した子の待機とCPU使用量の同時観測

posix.serviceにwait_child_usage_with_replyを追加しました。子を回収する直前に、
その子自身と、その子が実際にwaitした子孫のCPU使用量を同じtransactionで公開します。
プロセス全体のRUSAGE_CHILDRENを前後で引き算する実装は、別threadのwaitと
競合するため使用しません。既存waitと同じ選択・認証・返信後の回収処理を共有します。

native会計の失敗、不正な単位、overflow、返信失敗ではイベントを消費せず、
親の累計も増やしません。WNOWAITは使用量を返しても回収せず、停止イベントも
取得失敗時には再試行できます。WNOHANGでイベントがない場合は会計を読みません。
既存waitのWNOWAITは従来どおり会計を要求しません。

hostのprotocol 497件とchild_cpu 2件が成功しました。新経路の返信拒否と再試行、
資格のない親からの取得拒否、子孫の集計、WNOWAIT、停止と空のWNOHANGを含みます。
記録はout/checks/mochios-wait-usage-221-host-final.logです。
これはサービス内部の処理までであり、新しいwire replyとwait4 C APIへの接続は
まだ行っていません。最大常駐メモリの計測も未実装です。clang / ld.lldの
全体ビルドはProgram.cppで失敗した状態で停止しており、再起動していません。

段階222で[wait4のwire / client / libc接続](posix-wait-usage.md)を実装し、
KVMの実IPC試験を通しました。Program.cppを本番設定で再コンパイルしたところ、
wait4の宣言エラーは解消し、ru_maxrssの不足だけで失敗しています。
記録はout/checks/mochios-llvm-program-222-build.logです。
clang本体のビルド・実行とCプログラムのコンパイル成功はまだ確認していません。

段階223 / 224で[RAM参照の計測](posix-memory-residency.md)と
[Process Handleによる取得](posix-process-memory.md)を実装し、KVMで実際のページ増減と
終了後のpeak保持を確認しました。rusageのfield / libc / POSIXサービスへの接続は
まだ残っているため、Program.cppのru_maxrssエラーは未解消です。


## 段階225: rusageへの最大常駐量の接続

[最大常駐量のPOSIX会計](posix-rusage-memory.md)を追加しました。
Program.cppを本番のdefines / includes / flagsで再コンパイルできました。
記録はout/checks/mochios-llvm-program-225-build.log、objectは
mochios-llvm-program-225.oです。clang / ld.lld全体のリンクとmochiOS上での
コンパイル・実行成功を意味するものではありません。


更新したrelease runtimeを指定し、SDK / Rust std overlayを作り直さずに
LLVM targetを再構成し全体ビルドへ進む処理を再開しました。
記録先はout/checks/mochios-llvm-target-225-{configure,build}.logです。
完了状態はそれぞれの処理結果を確認する必要があります。


## 段階226: 公開アカウント取得と次の不足

LLVMSupport.aの実物を使うnative_llvm_support.cppを作り、ファイルbuffer、offset付き
mmap、rename、ExecuteAndWait、redirect、wait4のstatisticsを検証する準備をしました。
最初のリンクではPath.cppからgetpwuid_r / getpwnam_rへの未定義参照を検出したため、
[公開アカウント取得](posix-public-accounts.md)を実装し、Cの実機試験を通しました。

Support試験のリンクにはまだ成功していません。次のエラーはCrashRecoveryContext.cppの
STT_TLS symbolに対するPT_TLS segmentの欠落です。user/runtime/linker.ldには.tdata /
.tbssはあるもののPT_TLSの宣言がなく、runtimeのpthread_createもthread_local_baseを
0にしています。初期threadのcompiler TLS template配置、pthreadごとの独立したTLSと
終了時の解放を揃える必要があります。segmentだけ追加して動作確認済みとはしません。
記録はout/checks/mochios-llvm-support-226-link-final.logです。

全体ビルドではTargetParser/Host.cppからsys/utsname.hが見つからないエラーも検出しました。
これは新しい未解消箇所です。mochios-llvm-target-225-build.logに記録しています。
失敗を検出した後も同じmake jobが既に起動した別targetを処理している間は、
そのjobが終了するまで重複ビルドを始めません。
clang / ld.lldの完成、mochiOS上でのCコンパイルと実行はまだ未確認です。


## 段階227: 現在threadのlocal-base設定

[汎用のlocal-base設定](posix-thread-local-base.md)を追加し、KVMでCPU値と保存値が
syscall / yieldを越えて維持されることを確認しました。TLS templateやPOSIXの管理は
カーネルへ追加していません。compiler TLSの配置は次の段階に残ります。

段階225で再開した全体build jobは、他の実行中targetも終了した後、exit status 2で
停止しました。TargetParser/Unix/Host.incのsys/utsname.h不足は未解消です。
重複したビルドを起動しておらず、現在この全体buildは実行中ではありません。

## 段階228: 読み取り可能なプログラムヘッダー

[ELFメタデータの配置](posix-elf-metadata.md)を修正しました。AT_PHDRは、実際のLOAD内の
表、またはuser-image window内のread-only / non-executableなコピーを指します。
LOAD内外の2形式を通常の資格情報と実効UID 501で実行し、元ファイルとの完全一致と
書き込み拒否を確認しました。初回に検出したfork回帰も配置境界を守る修正で解消しました。
TLS初期化、PT_TLS segment、sys/utsname.hは引き続き未実装です。

## 段階229: 静的ELFのコンパイラTLS

[ユーザー空間の静的TLS初期化](posix-static-tls.md)を実装しました。
PT_TLSを持つ実際のC++実行ファイルで、初期thread、pthread間の独立性、8KiB alignment、
デストラクタ、forkの継承を確認しました。段階226でTLS不足によりリンクできなかった
実LLVMSupport試験も、KVM上のroot/euid 501で成功しました。
カーネルへのTLS ABI追加はありません。SDKとRust std overlayは再生成していません。

全体のclang/ld.lld buildは停止中です。次の既知の未解消箇所は
TargetParser/Unix/Host.incが必要とするsys/utsname.hと実際のuname実装です。
clangによるネイティブのコンパイル・リンク・生成物実行は引き続き未確認です。

## 段階230: unameとホスト名

[システム情報とホスト名](posix-system-identity.md)をユーザー空間で実装しました。
実際のrelease manifestから生成する情報を`posix.service`で公開し、
newlibとRust libcの`uname` ABIを揃えました。hostnameの変更はサービスが保持する
実効UIDで認可します。ホスト507テストとKVMのroot/euid 501で検証しました。

LLVMのconfigureを新しいruntimeで完了し、停止していた全体buildを再開しました。
`TargetParser/Host.cpp`のコンパイルと`LLVMTargetParser`の生成が成功したため、
sys/utsname.h不足は解消しています。clang/lld全体のbuildと実行の完了はまだ未確認です。
実行中jobのログは`out/checks/mochios-system-identity-230-llvm-build.log`。

## 段階231: newlibの壁時計とネイティブコンパイラ試験の準備

生成済みarchiveの参照を調べ、newlibの`_gettimeofday_r`が必要とする
`gettimeofday`本体の欠落を確認した。[壁時計adapter](posix-wall-clock.md)を追加し、
ホスト7テストとKVMのroot/euid 501で実際の`time()`呼び出しまで検証した。
診断環境で必要なsystem.time.readはmanifestで明示し、試験後に元へ戻した。

[ネイティブclangの統合試験](posix-native-clang-pipeline.md)も追加した。
試験objectとELFのクロスビルド、およびfixtureのCソースとheadersの組み合わせは確認した。
実clang/ld.lldからobject・ELFを生成して実行する試験の成功は、まだ確認していない。
ツール不在やcompile/link/exec失敗をskipや成功へ置き換えない。

既存の全体buildは再起動していない。KVM試験中は所有するプロセスを開始時刻まで
照合して一時停止し、通常イメージの検証後に同じjobを再開した。

## 段階232: LLDの実出力経路

[実FileOutputBufferの試験](posix-llvm-output.md)を追加し、KVM上のroot/euid 501で
commit、実行権限、rename置換、旧FDの維持、offset付き共有mmapの書き込み、
unmap後の内容とdiscardを確認した。mmapの失敗を代替bufferの成功で隠さない。
追加のkernel/runtime機能やLLVMの回避patchは不要だった。

全体buildは同じjobで続いている。clang/ld.lld本体によるコンパイル・リンク・
生成物実行の成功は引き続き未確認。
