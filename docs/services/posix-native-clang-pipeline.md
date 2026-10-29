# ネイティブclangの実行試験

LLVM 18.1.8の実clangとld.lldをmochiOS上で動かし、Cソースのコンパイル、
リンク、生成ELFの実行をeuid 0と501の両方で確認した。
両ケースとも生成プログラム内のgeteuid、コンパイラTLS、引数、正確なstdout、
終了コード37、一時ファイルの削除、親のwait status 0まで成功した。
記録は`out/checks/mochios-native-clang-240-native.log`。

この試験は同じ継承Capabilityの下でeuidを変更する。一般アプリのCapabilityを
すべて減らした状態での動作証明とは区別する。POSIX互換環境全体の完成でもない。

## 検証内容

`services/posix/tests/native_clang_pipeline.cpp`は次を順に実行する。

1. 実clangとld.lldの`--version`を実行する。
2. 不正なCはstatus 1と診断を返し、objectを残さないことを確認する。
3. 正しいCをclang driverでコンパイルし、ELF relocatable objectを確認する。
4. 実ld.lldでCRT、script、runtime、libc、libm、libgccとリンクする。
5. 生成ELFをexecし、実効UID、TLS初期値とゼロ初期化、引数を確認する。
6. TLS更新後のstdoutと終了コード37、一時ディレクトリの削除を確認する。

`native_llvm_runtime.cpp`はexec直後のgeteuidも期待値と照合する。
`native_llvm_runtime.rs`はseteuid(501)後も含めて子を実行し、実際のwait statusを検査する。
最後の`llvm-runtime: PASS child exec and wait`までを成功条件とする。
段階240ではcompilerのPASSが2回、親のstatus 0が4回で、残り2回は既存の
inline PHDR試験である。TLS、LLVM出力、mmap、rusage等の既存試験も同時に通した。

ホストのクロスコンパイルをネイティブ成功として数えない。別compilerやshellへの
fallback、権限確認の省略、エラーを成功に変える代替処理は使わない。

## 診断用fixture

診断イメージだけに以下を配置する。製品のSDKインストール場所を定義するものではない。

```text
/system/tests/posix/llvm/
  bin/clang
  bin/ld.lld
  resource/include/
  sdk/lib/crt0.o
  sdk/lib/linker.ld
  sdk/lib/libmochi_user_newlib_runtime.a
  sdk/lib/newlib_reent.o
  sdk/lib/libgcc.a
  sdk/sysroot/include/
  sdk/sysroot/lib/libc.a
  sdk/sysroot/lib/libm.a
```

SDKは読み取り入力とし、source header overlayをfixtureへ反映する。
SDKとRust std overlayは再生成しない。CRTとscriptは現在のuser/runtime由来、
runtimeはstandalone release archiveを使う。
`-nostdinc`に合わせてSDKとclang resourceのincludeを明示する。
対象は現在のLLVM portと同じ`x86_64-unknown-none-elf`と`__mochios__`である。

ディレクトリは0755、通常ファイルは0644、二つの実行ファイルは0755とする。
ホストのprivateなmkdtemp領域の0700を配置先へ持ち込まない。
段階238ではこのmodeの不備でeuid 501のclang起動がEACCESとなった。
正しいsearch permission拒否であり、権限チェックを緩めず配置側を直した。

時刻を使う試験のため、診断ランチャーのstaged manifestだけにsystem.time.readを
明示し、msign built-in-recordと通常のイメージ認証を通す。本番manifestへは追加しない。

## 修正と起動前提

LLVM ExecuteAndWaitのredirectは既存ファイルをtruncateしないため、各コマンドの前に
captureを空にする。段階233のstdout不一致は先行するversion表示の残りによるものだった。
newlib標準のremoveはファイルのunlinkしか試さないため、ユーザー空間のC adapterで
空ディレクトリにも対応した。権限・I/Oエラーは別操作で隠さず伝播する。

試験ランチャーはcompiler用の有限900秒watchdogを持ち、正常完了時に解除する。
従来のソケット用90秒alarmと、LLVM経路の解除漏れは修正した。
各UIDケースの開始、exec後の実UID、親waitの完了をログへ残す。

入力サービスは現在のrelease runtimeで再リンクした。IME engineは変更していない。
ホストの自動checkを一時停止しても起動期限切れが再発し、ホスト負荷だけを原因とは
断定できなかった。一時停止した同一PID・開始時刻の処理は再開済みである。

過去のnative-ime-loadは約320万回のallocationと約14～43秒のloadを記録している。
INPUT_READY_TIMEOUT_TICKS=15,000は現行500 Hzで30秒であり、測定済みのcold loadを
収められなかった。入力サービスだけを30,000 tick（60秒）へ変更した。
有限期限、READYの認証、Process exitの観測と失敗伝播、他サービスの予算は維持する。
カーネルへサービス固有の知識は追加していない。
既存prompt_endpoint_readinessの11テストと、通常・診断managerのbuildが成功した。
記録は`out/checks/mochios-input-budget-240-{host,manager,normal-manager}.log`。

大きいcompilerのイメージ取得上限とオンデマンドのヒープ容量については
[posix-image-capture.md](posix-image-capture.md)を参照する。

## 通常構成への復元と範囲

全ケース成功後に診断用fixtureとprobe ELFを取り除き、本来のmanifestとverificationの
保存bytesとの完全一致を確認した。通常managerを配置し、イメージを再生成した。
通常KVMでも入力READY、Binder ready、デスクトップとDockの実表示を確認し、VMを停止した。
記録は`out/checks/mochios-native-clang-240-normal.log`と`-normal.png`。
既存のnetwork Failed(-6)とUSB driver errno 22は残っている。

このgateは専用fixtureでの静的C実行であり、clangのmochiOS default target driverや
ld wrapper、一般的なSDK配置、共有ライブラリのruntime linkerの完成を意味しない。
完全なPOSIX環境のgoalは継続する。


## 次の実用gate

優先する利用経路は、通常の`clang main.c -o main`でlink後の署名まで済ませ、
`./main`の実行時に必要なCapabilityだけを共通gateで確認するものです。
CLIは信頼済みmshのtext確認、GUIはsecure-uiのpopupとし、判断・保存・一回認可の消費は
capability.serviceへ集約します。詳細と未完了箇所は
[ローカル開発と共通実行認可](../elf-signature-memo.md#ローカル開発と共通実行認可の実装目標2026-10-29)を参照します。
この記述は専用fixtureの成功を、通常driver・自動署名・対話認可の完成へ読み替えるものではありません。
