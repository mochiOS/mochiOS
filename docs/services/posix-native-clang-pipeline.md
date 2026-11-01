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

## リンカードライバ（313）

tools/devkitの既存リポジトリ内にmochios-linker-driverを追加しました。
SDKを読み取り入力として、crt0 / script / runtime / libc / libm / compiler runtimeを
解決し、実ld.lld、既存msignの署名と検証、最終出力のrenameを順に行います。
SDKとRust overlayの生成処理は変更・実行していません。

ホスト11件の失敗注入・引数・出力所有権の試験と、実ツールを使う2件が成功しました。
後者はコード改変の署名拒否と、driver CLIが既存SDKのCRT・runtime・libcを含むCの
リンクを行い、署名検証できることを確認します。link失敗でも直前の出力を維持します。
実SDKとの最初の試験は補助オブジェクトの重複定義で失敗し、その存在だけで自動追加する
処理を除去してから成功しています。未知の補助ファイルを必要と推測してlinkしません。

mochiOS向けdriverはビルド済みですが、ゲスト内のdriver / msignの実行、clangの
default設定、ELF内manifest、共通実行認可と保護された同意frontendへの接続は未完了です。
通常イメージへ未検証のツールを配置したものでもありません。
初期版の対応範囲・引数・残件は[driverの仕様](../../tools/devkit/docs/linker-driver.md)にあります。
証跡はout/checks/mochios-linker-driver-313-{build,host,real-host,native-build}.logです。
既存msign全体もnative buildを試みましたが、ネットワーク依存のmio / socket2で失敗しました。
mioはmochiOS向けselector / wakerを選べず、socket2はlibcの一部宣言を要求します。
これは成功扱いにはせず、同signer-probe.logへ記録しています。ローカルELF署名を同じmsignの
実装からビルドできる構成と、これらネットワーク依存の移植は引き続き必要です。

## ネイティブ署名までの統合（314）

通常Terminalのmshから`/system/bin/native-signer314`を実行し、診断用に明示配置した
実clang、ld driver、ld.lld、既存msignを使う次の経路を確認しました。

1. clangがCソースをコンパイルし、driverが既存SDK入力とLLDでリンクする。
2. driverがmsignのAdHoc署名と検証を行い、最終ELFを公開する。
3. 生成ELFのstdoutが`guest-build314\n`、終了コードが37となる。
4. ELFの改変をmsign verifyが拒否する。
5. ネイティブ乱数で秘密鍵・公開鍵を生成し、同じdriverから鍵付き署名・検証を行う。
6. 一時ディレクトリを解放し、通常のmshプロンプトへ戻る。

fixtureは`services/posix/tests/native_signer_flow.rs`、成功画面は
`out/checks/mochios-signer-native-314-flow-second-final.png`です。
ソースはheaderを必要としない小さいCで、ツールとSDKのパスを明示しています。
通常の`clang main.c -o main`へのdefault設定や、一般アプリのCapabilityへ縮小した環境での
動作を証明する試験ではありません。改変拒否はCLI検証であり、exec時の署名gateは未接続です。

最初の実機試験ではdriverのcurrent_exeが未対応、msignの権限保持がEINVALとなりました。
runtimeはAT_EXECFNをユーザー空間でcanonicalizeし、コンストラクタ前に所有バッファへ保存します。
getexecname経由でRust stdへ渡し、相対パス・非UTF-8のsymlink名・偽のargv[0]・環境変数・
後からのchdirを含む実機試験を通しました。名前情報は認証やCapability判断に使いません。
POSIX環境のないbootstrapではパス解決せず、サービス起動の循環を作りません。

chmod / fchmodのC境界は、stat由来のfile type bitsを除いてmodeを送信します。
wire protocolの検証やアクセス判定は緩めていません。ldとmsignは同じprivate directoryと
保持中のFDによる公開処理を使い、mode 000でも再openせずsync・renameできます。
lutimes / utimesも既存のatomicなパス時刻更新へ接続し、symlinkだけの時刻変更を確認しました。

msignの`developer-ca`はdefault有効のまま、オンライン証明書取得だけをfeatureで分離しました。
native版はこれを無効にし、ELF署名・鍵生成・ローカル証明書・MPKG等の同じ実装を使用します。
getrandom 0.2はposix-clientから既存の汎用乱数primitiveを使います。ネットワークを含む
通常構成に必要なmio / socket2の移植は未完了です。

ホストではmsign通常構成42件、オフライン構成36件、entropy adapter 6件、時刻入力6件、
実行名snapshot 3件、公開処理6件、driver 11件と実LLD・msign連携2件が成功しています。
追加したPOSIX回帰試験はservices/posix/tests以下に配置し、既存msignの試験も実行しました。
既存SDK入力は固定snapshotで読み、
SDKおよびRust overlayは再生成していません。Rust stdの既存source linkを通してforkの修正を
ビルドしています。カーネル変更と新規Capability付与はありません。

試験VMを停止し、同一PID・開始時刻の記録に基づく解析処理の復帰も完了しました。
所有する診断用11ファイルだけを削除して通常イメージを再生成し、6成果物のchecksumを確認しました。
イメージから読み戻したTerminal / msh / posix.service / input.serviceは、試験前の4ハッシュと
一致しています。試験用ディレクトリとprobeがイメージ内にないことも確認しました。
記録は`out/checks/mochios-signer-native-314-{cleanup,readback}.json`です。

試験中にTerminal外の描画が一部崩れる現象も観測しました。署名機能の成功とは別の未解決事項です。
通常clang設定・製品配置・ELF内manifest・共通実行認可・保護されたCLI / GUI同意を引き続き
接続する必要があり、完全なPOSIX環境のgoalも継続します。

## clang標準設定（315、実機確認前）

LLVM 18.1.8のclangへmochiOS専用ToolChainとTargetInfoを追加しました。
従来のx86_64-unknown-mochiosは汎用GCC系のToolChainを選んでいたため、clang自身が
ホストGCCのリンク設定を選ぶ可能性がありました。新しい設定はmochiOS名のtargetだけに
適用し、ELF / LP64、__mochios__ / __unix__、SDKとclang自身のヘッダーを使います。
Linuxを名乗らず、カーネルにコンパイラやSDKの配置を知らせません。

SDK選択はldと合わせ、MOCHIOS_SDK、明示sysrootの親、clangのbinの親を使います。
明示--sysrootはヘッダーと標準ライブラリのrootを独立に選択できます。
CRT・ライブラリ・compiler runtime・署名の追加は共通ldへ任せ、clangへ複製しません。
--rtlibはldのcompiler runtime選択へ渡します。C++はSDKのlibc++ / libc++abi / libunwindを
選ぶ設定がありますが、この段階でC++アプリのコンパイル・実行を検証したものではありません。

-cはコンパイルだけを行い、-nostdinc / -nostdlibinc / -nostdlib / -nostartfiles /
-nodefaultlibs / -rを区別します。-fuse-ld=lldでも共通ldを経由し、署名を迂回しません。
別のbackendは拒否し、--ld-pathは共通ldそのものの場所を指定します。
Clangがリンク失敗後に古い最終出力まで削除しないよう、transactionalな公開を行うtoolへ
後始末を任せるCompilation APIを追加しました。他のToolChainの後始末は変えていません。

パッチはscripts/llvm/patches/clang-18-mochios-toolchain.patchです。
configure-llvm-target.shが既存のPathパッチと合わせて適用し、不一致はエラーにします。
SDKとRust overlayの生成処理は変更・実行していません。

### ホストで確認した範囲

services/posix/tests/clang_driver.cppは実際のclangDriver / clangBasicを使う小さいharnessです。
ソース入力では-###でコマンド選択を調べ、別frontendへの置換は行いません。
object入力では共通ld、実LLD、msignまで実行します。ターゲットのdefine / LP64、空白を含む
SDKパス、include抑制、C++設定、引数転送、ABIとbackendの拒否、署名済みELFの生成、
未定義symbolによるリンク失敗後の旧出力維持を含む8件が通りました。
これはホストで動くDriverライブラリの試験で、mochiOS上のコンパイル成功とは区別します。

同じ試験は次のスクリプトで実行できます。LLVM_SOURCEにはパッチ適用済みの18.1.8 source、
LLVM_HOST_BUILDにはホスト用に構成したLLVMの既存ビルドディレクトリを渡します。

```sh
services/posix/tests/clang_driver.sh LLVM_SOURCE LLVM_HOST_BUILD \
  MOCHIOS_LD HOST_LD_LLD HOST_MSIGN SDK
```

harnessはLLVMのRTTI設定に合わせて-fno-rttiでビルドします。ld.lldのsymlink名は保持します。
LLDはargv[0]でdriver flavorを選ぶため、実体のrust-lld等へ名前を書き換えません。
記録はout/checks/mochios-clang-default-315-host-{tests,workflow}.logです。

### 未確認の実機経路

native_clang_driver.rsは、配置したSDKのbin/clangへ追加target・SDK引数なしで
`clang main.c -o main`を渡し、stdio / stdintとOSのdefine、署名検証、`./main`の出力と
終了コード37、コンパイル・リンク失敗後の旧ELF保持を検査します。
fixture、トレースなしruntime / ld / msign、mochiOS版clang本体のnative buildは成功しました。
診断SDKを明示配置した最初のKVM試験では、`clang -dumpmachine`がstatus 0でもstdoutが空となり、
最初の検査で停止しました。通常PTYへ直接出力した場合は期待するtarget文字列が出ました。
下記の終了処理修正を含むclangでは、pipeで受け取るtargetのstdoutが期待値と一致しました。
続くコンパイル・リンク・署名・実行を同じfixtureで確認しています。

通常環境への製品配置、ELF内manifest、実行時の署名検証と共通認可、保護されたCLI / GUI同意、
完全なPOSIX環境は引き続き未完了です。

## newlibの終了処理登録数（317）

newlibの`_ATEXIT_DYNAMIC_ALLOC`が有効でも、通常のnewlib allocatorを使う構成では
`MALLOC_PROVIDED`が未定義のため、終了処理の33件目を拒否していました。
これはアプリ内の状態であり、カーネルへ終了callbackの意味論は追加しません。
mochiOSではnewlib自身のmalloc / freeを使って登録blockを増減できるようにし、
bit 31の登録・参照で発生していたsigned shiftもunsignedへ修正しました。
他targetのallocator選択条件は維持しています。

`services/posix/tests/newlib_exit_handlers.sh NEWLIB_SOURCE SDK`は実際のnewlibの
`__atexit.c` / `__call_atexit.c`をSDK headerでコンパイルし、host allocatorとprivateなsymbol名で
次の4ケースを実行します。別のregistry実装を試験へ持ち込んでいません。

- 128件を逆順で一度ずつ実行し、動的blockを解放する。
- 複数blockのDSO別callbackを選んで実行し、残りも一度ずつ実行する。
- 33件目のallocation失敗で既存登録を失わず、その後に再登録できる。
- 終了処理から追加したcallbackと、引数なしcallbackの実行順序を保つ。

修正前は33件目の登録とbit 31のUBSan検査で失敗し、修正後はASan / UBSan / LSan付きの
host 4ケースが成功しました。同じprivate registryとC fixtureをnative runtimeでリンクし、
通常TerminalのmshからmochiOS上でも4ケースのPASSを確認しました。
証跡は`out/checks/mochios-newlib-exit-317-host.log`と
`out/checks/mochios-exit317-resume.png`です。

SDKとRust overlayは再生成せず、SDKの入力も書き換えていません。
検証用clangは修正した2つのnewlib objectをbootstrapのlibcより先にリンクしています。
生成アプリは引き続き供給された現在のSDKを使います。一般アプリへこの終了処理修正を
反映するには、SDK管理側で修正済みnewlibを取り込む必要があります。
clangの再検証結果、診断用配置の撤去と通常イメージへの復元はまだ確認中です。
