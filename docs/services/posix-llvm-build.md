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

全体ビルドでは現在、llvm/lib/Support/Unix/Path.incに次の未対応が残っています。

- getMainExecutableのmochiOS向け実装
- statvfsのfield名と、ローカルmount判定の扱い
- mapped_file_regionの不要ページを伝えるAPI

自己申告argvだけで実行imageの位置を決める対応、kernelにpathnameを照会する専用API、
未提供関数を成功に見せるstubは追加していません。
これらを解決した後も、残りのコンパイル・リンク・clang起動・Cのコンパイル・生成物の実行を
実際に確認する必要があります。

記録はout/checks/mochios-llvm-host-tools-216-{configure,build}.log、
mochios-llvm-target-216-script-final.log、mochios-llvm-target-216-build-next.logです。
