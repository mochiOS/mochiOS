# ネイティブclangの実行試験（準備中）

`services/posix/tests/native_clang_pipeline.cpp`は、完成したmochiOS向けclangと
ld.lldを使い、Cソースから実行可能なELFを作る統合試験である。
現時点では試験本体のクロスコンパイルとリンク、および生成するCソースと
入力headerの整合性をホストのクロスcompilerで確認した。
ネイティブclang本体のbuild待ちであり、mochiOS上でのこの試験の成功は未確認。

## 成功条件

1. 実際のclangとld.lldの`--version`が成功する。
2. 不正なCソースは診断を出し、status 1で終了し、objectを生成しない。
3. 正しいCソースをclang driverでコンパイルし、ELF relocatable objectを生成する。
4. 実ld.lldでSDKのruntime、CRT、script、libc、libm、libgccとリンクする。
5. 作ったELFをexecして、引数とコンパイラTLSの初期値・ゼロ初期化を確認する。
6. 生成プログラムのexit status 37とstdoutの正確な内容を確認する。

ホストでのコンパイルをネイティブの成功として数えない。
ツール不在、exec失敗、診断や生成物の不一致は失敗とする。
shellや別のcompilerへのfallbackは使用しない。
失敗時には対象プロセスの診断ファイルを表示する。

## 診断用fixture

これは製品のSDKインストール場所の決定ではない。診断イメージだけに以下を配置し、
通常イメージへ戻す際にはディレクトリ全体を取り除く。

```text
/system/tests/posix/llvm/
  bin/clang                  # build-target/bin/clang-18の実ELF
  bin/ld.lld                 # build-target/bin/lldの実ELF
  resource/include/          # build-target/lib/clang/18/include
  sdk/lib/crt0.o
  sdk/lib/linker.ld
  sdk/lib/libmochi_user_newlib_runtime.a
  sdk/lib/newlib_reent.o
  sdk/lib/libgcc.a
  sdk/sysroot/include/
  sdk/sysroot/lib/libc.a
  sdk/sysroot/lib/libm.a
```

SDKは読み取り入力として使用し、ソース側header overlayをfixtureへ反映する。
SDKとRust std overlay自体は再生成しない。
CRTとscriptは現在の`user/runtime`由来、runtimeはstandalone release archiveを使う。

`-nostdinc`を指定するためSDKのheaderに加えてclangのresource/includeを明示する。
`stddef.h`や`stdarg.h`など、SDK単体にはないcompiler付属headerも必要である。
対象は現在のLLVM portと同じ`x86_64-unknown-none-elf`と`__mochios__`。
clangのネイティブmochiOS target driver対応が完了したという意味ではない。

既存`native_llvm_runtime.cpp`を
`MOCHIOS_NATIVE_CLANG_PIPELINE_PROBE_231`付きでビルドし、試験objectをリンクする。
既存の`MOCHIOS_LLVM_SUPPORT_PROBE_226`も併用すると、そのchild dispatchを使用できる。
実時刻を利用するライブラリのため、診断ランチャーは`system.time.read`を
明示的に要求する。権限不足時に架空の時刻や別経路を使用しない。
