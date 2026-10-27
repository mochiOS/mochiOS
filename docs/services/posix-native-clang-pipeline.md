# ネイティブclangの実行試験

LLVM 18.1.8のclangとld.lldのターゲットbuildを完了した。段階234のKVMでは、
実際のclangでCソースをコンパイルし、実際のld.lldでリンクしたELFをexecし、
TLS初期値・更新、引数、stdout、終了コード37の検査まで成功した。
記録: `out/checks/mochios-native-clang-234-native.log`の`native-clang: PASS`。

この記録は最初の実行（euid 0）の結果であり、euid 501の成功はまだ確認していない。
後片付けはnewlibのremoveが空ディレクトリを扱えず失敗した。
ユーザー空間のCランタイムを修正し、次の段階235では後片付けも成功条件へ含める。
ホストではファイル、空/非空ディレクトリ、symlink、unlink後のFD、
権限/I/O失敗、reentrant errnoの試験が成功し、ターゲット用C objectも生成済み。

LLVM ExecuteAndWaitのredirectは既存ファイルをtruncateしない。段階233の
stdout比較失敗は、同じ出力ファイルへ先行するversion表示が残ったことが原因だった。
試験は毎回captureを空にし、前のコマンドの出力・診断を混ぜない。

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

## 段階235・236: 後片付けとランチャーの待機範囲

段階235では、出力比較と一時ディレクトリの削除まで含む最初の実行が成功した。
記録は`out/checks/mochios-native-clang-235-native.log`。
ただし親によるwait完了とeuid 501の成功ログは得られていない。
CPUを読み取り観測した記録も保存したが、そのRIPに別のELFのsymbolを当てるだけでは
実行中プロセスの同定にはならないため、成功・停止の根拠として使わない。

ランチャーにはソケット試験用の90秒alarmがあり、LLVM経路だけ末尾のalarm解除を
通らないことを発見した。段階236はコンパイラ試験用の有限900秒watchdogを使い、
正常完了時に解除する。各子のeuidケースの開始とwait status 0も明示する。
親の最終`llvm-runtime: PASS child exec and wait`まで確認してから全ケース成功とする。
タイムアウトだけが原因だったという断定は、後続試験の結果が出るまで行わない。

段階236の初回はinput.serviceのreadinessがtimeoutし、コンパイラ試験へ到達しなかった。
辞書read自体は成功し、その後input READYも出たが、起動期限後だった。
ホストでは別のRust buildとrust-analyzerが実行中であることを読み取り確認した。
それらを停止せず、自分の検証VMだけ停止した。記録:
`out/checks/mochios-native-clang-236-boot-timeout.log`。
起動タイムアウトをPOSIXやコンパイラの成功としては扱わない。

通常イメージのstagingとイメージ生成を完了し、診断用の追加system.time.read、
compiler fixture、二つのprobe ELFを除去した。manifestとverificationは段階233前の
保存bytesとの完全一致を確認した。通常managerのビルドも成功している。
通常起動の再検証結果は別途記録する。

通常構成のKVMではinput READY、Binder ready、デスクトップとDockの表示を確認した。
記録は`out/checks/mochios-native-clang-236-normal.log`と`-normal.png`。
既存のnetwork Failed(-6)、USB driver errno 22、画面上の起動ログの断片は残っている。
この通常起動の確認を、まだ未確認のeuid 501によるcompiler試験の代わりにはしない。
