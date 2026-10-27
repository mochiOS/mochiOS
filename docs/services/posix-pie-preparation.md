# ユーザー空間の PIE image 準備

## 実装した範囲

`posix-client::pie` に、割り当てを行わない ELF64 / x86-64 PIE の解析・配置・相対 relocation の
処理を追加しました。libc の execve / fexecve は、ET_DYN の場合にこれを使います。
今回の対象は interpreter / DT_NEEDED / symbol resolver を要しない static PIE です。
静的TLSのテンプレートも検証し、元のprogram headersとload biasをruntimeへ渡します。
動的リンク全体や glibc の loader が完成したという意味ではありません。

元の FileHandle を positional read で所有バッファへ読み、R_X86_64_RELATIVE を user space で
解決し、既存の native loader が扱える固定アドレス ELF を RAM object に構築します。
relocation の値は ELF64 の語として B + A modulo 2^64 で計算し、負の addend による
折り返しを許容します。file / virtual extent の範囲計算は別に overflow を検証します。
配置は native RandomFill の entropy と元 segment の alignment を使って選びます。
新しいカーネル API やサービスは追加していません。

生成した RAM object の書き込み参照は閉じ、READ | TRANSFER の参照だけを使います。
その bytes の digest を既存の同一 domain の image-replace-inherit 認可へ結び付けます。
カーネルは実際の生成 image を検証してから置き換え、application identity と権限上限を保ちます。
source の pathname の再オープンや、package index での権限追加は行いません。

## 配置と補助情報

source の PT_LOAD の相対配置を維持し、entry と relative pointers に load bias を適用します。
BSS の relocation 先が file extent の外にある場合、その先までだけゼロを materialize します。
残る BSS の大きさは元の p_memsz に保ち、全域を file bytes として確保しません。
GNU unwind / RELRO / non-executable stack の記述は保持します。
RELRO の protection 遷移をこの変換処理が実装したわけではありません。

native container 用の program headers は独立した read-only LOAD に配置します。
プログラムに渡す AT_PHDR / AT_PHENT / AT_PHNUM / AT_ENTRY は、元の program の view を使います。
そのため posix-client の stack encoder に、user loader が選んだ metadata を渡せる入口を追加しました。
native container の値へ再 relocation されないようにし、既存の FD context / resource policy /
stack-address relocation は保持します。AT_BASE は interpreter がないので 0 です。
元のPHDRにある未補正の仮想アドレスには、ユーザー空間専用の
`AT_IMAGE_LOAD_BIAS`（`0x6d000004`）を使います。固定アドレスELFでは0です。
カーネルはこの補助情報の意味を解釈しません。

## 境界と所有権

すべての file / virtual extent、alignment、entry の executable range、W+X、relocation target、
重複・重なる fixup、table size、整数 overflow を検証します。
input / output の上限は既存の native exec と共通の
`MAX_CAPTURED_IMAGE_BYTES`（256 MiB）です。
source buffer、prepared buffer、RAM Handle は callback-resource owner に保持し、
失敗や C callback が exec を放棄した場合も回収します。source の cursor は変更しません。

PT_INTERP、DT_NEEDED、symbol-dependent relocation（TLS用を含む）、IRELATIVE、REL は
現在未対応なので ENOEXEC を返します。元の image を commit してから失敗する経路や、
未知の relocation を黙って無視する経路はありません。
元 program headers が PT_LOAD に含まれない場合も、この段階では未対応です。

## 検証

`pie_image.rs` は layout / relocation / BSS / metadata と不正な範囲を検証します。
`fexecve_runtime.rs` では実コンパイラが生成した PIE を実装本体へ渡し、
部分 read / write、RAM 作成・書き込み・参照の縮小・認可・loader の失敗時の解放を確認します。
`process_image.rs` は program metadata の値と native ELF relocation の分離を確認します。

native_static_pie.c は runtime / Linux ABI を使わない本物の PIE です。
relative な data / function pointers、128 KiB BSS、配置範囲、元 PHDR と entry の補助情報を検査します。
native_pie_exec.c から pathname と unlink 済み FD の両方で実行し、
未対応 interpreter の失敗で image / Handle が維持されることも確認します。

ホスト全体回帰は 679 targets / 3,302 件成功・失敗 0 件・8 ignored、
最終の関連 3 targets は 18 件成功でした。KVM で PIE の 3 groups と
既存 pathname / descriptor exec の 9 groups が成功しています。
折り返しを伴う負の addend と、元 program headers / entry の検査も実行済みです。
通常構成へ戻した後の input READY、デスクトップと Dock も確認し、VM は停止しています。

ただし、同じ通常起動ログには network.service の READY 待ち TimedOut が記録されています。
全サービスの正常起動とは扱いません。後続の調査で、試験runnerはNICを付けない設定で、
初期化に失敗したdriverがIPC要求を受け付けず待ち続けていたことが分かりました。
失敗状態の応答経路は[ネットワークの機器不在時の応答](network-unavailable.md)で修正しています。

記録は `out/checks/mochios-pie-204-host.log`、`mochios-pie-204-boundaries-final.log`、
`mochios-pie-204-native.log`、`mochios-pie-204-normal.log` と `.png`、
`mochios-pie-204-artifacts.json`、`mochios-pie-204-evidence.json` です。
配布 SDK と Rust std overlay は再生成していません。

## 続ける範囲

共有ライブラリの dependency graph、symbol scope / version、動的TLS、PLT / IFUNC、
dlopen / dlclose の寿命管理、interpreter の初期化は引き続き必要です。
POSIX 互換環境全体の goal は継続中です。

## 静的PIEのTLS対応（phase242）

共通のTLS parserで、重複セグメント、alignment、初期化データのファイル範囲を
検証します。runtimeは元のPHDRとload biasから初期値をコピーし、スレッドごとに
TLSを確保します。補助情報が存在しない固定アドレスELFはbias 0として扱いますが、
読取りエラーを0へ置き換えることはしません。

ホストではPIE、初期stack、TLS parser、実runtime allocator、補助情報、fexecveの
6対象が成功しました。KVMではeuid 0 / 501の両方で、実コンパイラのTLS-bearing PIEを
pathnameから各3回、unlink済みFDから各1回起動しました。元PHDR、bias、8 KiB alignment、
実際にロードされたTLS初期値、relative pointers、BSS、FDの共有cursorを確認しました。
未対応interpreterの失敗でHandleが増えず、親のwaitが正常終了することも確認しました。
このnative子はFSを初期化しない独立したentryであり、完全なPIC libcのTLSアクセス試験ではありません。

既存SDKで完全なPIEプログラムをリンクする試験は、非PIC runtime archiveの
`R_X86_64_32S` relocationで失敗しています。SDK / Rust std overlayは再生成していません。
共有ライブラリのロードや動的TLS、SDK全体のPIC対応は未完了です。
証跡は`out/checks/mochios-pie-tls-242-{native.log,artifacts.json,sdk-link.log}`、
ホスト試験は`mochios-pie-tls-241-host-template.log`です。

診断ELFを取り除いた通常イメージでもinput READY、Binder ready、デスクトップとDockを
確認し、KVMを停止しました。証跡は`mochios-pie-tls-242-normal.{log,png}`です。
既存のnetwork readiness `Failed(-6)`とUSB driver creation errno 22は残っています。

## 圧縮相対relocation（phase243）

`DT_RELR` / `DT_RELRSZ` / `DT_RELRENT`をユーザー空間で処理します。
[gABIのRELR形式](https://github.com/xinuos/gabi/issues/2)に従い、direct entryと
63-word bitmapを解釈し、空bitmapもcursorを進めます。RELRをRELAの前に処理します。
implicit addendは元のLOADから読み、fileとBSSの境界にかかるwordは残りをゼロとして扱います。
加算はELF64のmodulo演算です。

必須tagの欠落・重複、entry size、tableのfile-backed範囲、bitmapの基準address欠落、
overflow、LOAD外のtarget、RELR内およびRELAとの重複した書換えを拒否します。
未整列でも偶数のdirect addressと、順不同で重ならないtargetを扱えます。
一般的な昇順のtargetは一巡で重複検証し、LOADごとに出力offsetを一度求めて適用します。
BSSは書換え先までだけmaterializeし、残る領域はdemand-zeroを維持します。

ホストの`pie_image` 12件、`fexecve_runtime` 7件が成功しました。
GCC/binutilsの圧縮あり・なしの実ELFについて、symbol tableから求めたdata/function pointerの
期待値を照合し、exec準備の失敗時に元FDと一時Handleの所有権が保たれることも確認しました。
8,065箇所のdense bitmapと、sparse / 上位bit / 空bitmap / 不正入力も検証しています。
mochiOS向けruntimeのrelease buildも成功しました。今回の変更ではnative IPCや
カーネルの境界を変えていないため、KVMの再起動試験は行っていません。

証跡は`out/checks/mochios-relr-243-{host-final,exec-results,pie-results,runtime}.log`です。
interpreter、共有ライブラリ、symbol resolver、動的TLSは引き続き未実装です。
