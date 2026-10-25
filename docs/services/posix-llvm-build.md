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
