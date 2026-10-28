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

PT_INTERP、DT_NEEDED、他オブジェクトの探索が必要なrelocation、TLS relocation、IRELATIVE、REL は
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
interpreter、共有ライブラリ間のsymbol resolver、動的TLSは引き続き未実装です。

## 単一ELF内の動的シンボル（phase244）

`R_X86_64_64`、`R_X86_64_GLOB_DAT`、`R_X86_64_JUMP_SLOT`を解決します。
`DT_JMPREL` / `DT_PLTRELSZ` / `DT_PLTREL`のRELA形式を読み、PLTは起動前にすべて
即時解決します。元のシンボル値と配置biasを区別し、`SHN_ABS`と未定義weakへbiasを
加えません。定義済みシンボルの範囲はLOAD内で検証します。64-bit relocationはS+A、
GLOB_DAT / JUMP_SLOTはSだけを使います。
計算式は[x86-64 psABI](https://raw.githubusercontent.com/wiki/hjl-tools/x86-psabi/x86-64-psABI-1.0.pdf)、
weakの扱いは[gABIのsymbol table](https://uw714doc.xinuos.com/en/SDK_cprog/OF_SymTable.html)に基づきます。

シンボル表の件数はSysV hashのnchainまたはGNU hashの末尾chainから求め、
すべての読取りをfile-backed LOAD内に制限します。両hashがあれば件数の一致を確認します。
symbol entry size、文字列表、NULLシンボル、参照index、名前の終端、必要なdynamic tagの
欠落・重複を検証します。セクションヘッダーや隣接tableの位置から件数を推測しません。
RELR / RELA / PLTをまたぐ書換え先の重複も拒否します。

これは単一image内でのindexによる解決です。他のDSOを検索する名前scope、version scope、
DT_NEEDEDのdependency graphは未実装です。未定義の強い参照、TLS、IFUNC、GNU_UNIQUE、
version付き参照、audit / filter依存を黙ってローカル解決せず、Unsupportedとして返します。
既存のHandle / Capabilityによるimage認可・execの所有権管理は維持しています。

実GCC/binutilsが生成した依存なしET_DYNについて、SysV / GNU / 両hashの3構成を確認しました。
通常のsection table付きとsection tableを取り除いた両方で、すべてのGOT / PLT / data pointerを
リンカのsymbol / relocation出力と照合しています。合成試験では非ゼロのunused addend、
SHN_ABS、未定義weak、誤ったtable / index / name / LOAD範囲、PLT重複も検証しました。
関連6対象の結果は`out/checks/mochios-symbols-244-results.log`、target runtime buildは
`mochios-symbols-244-runtime.log`です。ELF変換だけの変更なので今回もKVM試験は行っていません。

## 明示的な依存scope（phase245）

`Pie::parse_object`と`pie::link_scope::Scope`で、取得済みのELF群を借用して
共有シンボルを解決します。rootを先頭に、`DT_NEEDED`の記載順で幅優先の検索順を
構築します。共有依存と循環は重複させず、不足した依存、名前の重複、到達不能な余分の
object、配置の重なり、alignmentやuser address範囲の違反を拒否します。
各objectのnative container metadataのmappingも配置検査に含めます。
名前とbytesの取得・対応付け、ファイルのHandleと所有権、実行認可は呼出し側の責務です。
このAPIはファイルを開かず、権限を増やしません。

シンボル検索にはSysV / GNU hashを使います。全tableの繰返し走査は行いません。
SysVの破損chainによる循環には件数上限を設け、GNU chainも検証済み範囲に制限します。
実行ファイル側の定義によるpreemption、最初に見つかった定義（weakを含む）、
local / hidden / internal / protected、DT_SYMBOLIC / DF_SYMBOLICを区別します。
非公開の同名シンボルは外部scopeへ出しません。未解決weakは0、強い参照は失敗です。

`Scope::write`は指定objectの変換済みbytesを作ります。すべての依存を準備してから
一緒にinstallする必要があります。通常の`Pie::parse` / `write`は依存付きimageを
引き続き拒否するため、既存execで依存を黙って省略する経路はありません。
依存objectのTLSは、集約TLS module layoutが未実装なのでUnsupportedを返します。
version、IFUNC、COPY relocation、interpreterも引き続き未対応です。

ホストでは実GCC/binutilsでrootと共有ライブラリを別々にリンクし、SysV / GNU / 両hashの
3構成でGOT / PLT / pointerを照合しました。変換後のLOADをホストの匿名mappingへ
配置し、最終permissionへ変更してから、syscallを含まないfixture関数を実行しています。
ライブラリからrootの定義を参照する値、rootからのPLT呼出し、その後の共有変数の値も
期待通りでした。mappingは試験終了時に解放します。これはホストでのリンク処理の
実行検証であり、mochiOSのProcessCreateや共有ライブラリexecの完成を意味しません。

この段階では依存取得と一括image生成・認可が未接続でした。後続phase246 / 247で
限定的なdirect-entry execへ接続しています。constructor順序、ライブラリ寿命管理、
dlopen / dlclose、TLS moduleとinterpreter起動は残っています。
関連6対象の証跡は`out/checks/mochios-link-scope-245-results.log`、
mochiOS向けruntime buildは`mochios-link-scope-245-runtime.log`です。
カーネル、配布SDK、通常起動イメージは変更していません。

## 依存群を含む単一のnative container（phase246）

`Scope::bundle_length` / `write_bundle`で、scope全体を一つの固定アドレスELFへ
まとめます。各memberは同じscopeでrelocationを完了してからfileへ収め、LOADの
file offsetだけを統合イメージ内の位置へ補正します。programの仮想アドレスは変えません。
rootのentryを使い、依存objectのGNU_STACKは統合tableへ重複させません。
グローバルなprogram header tableは全memberのmappingより下の独立したread-only LOADに
置きます。各memberの元PHDR viewとroot用startup metadataの意味は維持します。

出力全体はcaller-owned bufferで構築します。必要bytes、page alignment、整数overflow、
PHDR件数（extended numberingは未対応）、rootの実行可能entry、global metadataを置く
user address範囲を検証します。短いbufferは書き始める前に失敗します。途中のrelocation
失敗では出力を破棄する契約であり、一部を実行用mappingへcommitしてはいけません。
fileのpaddingはゼロにし、各memberのBSSは従来と同じく必要なrelocation先までだけ
materializeします。残る領域はメモリ上のdemand-zeroです。

ホストで統合ELFを一括mappingして、実共有ライブラリとrootの関数を実行しました。
SysV / GNU / 両hashの3構成でpreemption、GOT、PLT、共有変数の値が一致しています。
同じbytesをmnuの実`map_elf_image`へ渡す試験も成功しました。この試験で置き換えるのは
物理page mapping関数だけで、実際のELF検証・metadata選択・配置処理を使用しています。
不正なroot entry、metadataの余地不足、PHDR件数上限、短いbuffer、元bytesの保持、
BSS、mappingの非重複、W+X禁止も確認しました。

これはuser-spaceでの統合イメージ生成です。この段階ではexecの取得・認可へ
未接続でしたが、次節のphase247でdirect-entry imageへ接続しています。
constructor / finalizer、interpreter、複数objectのTLS、dlopen / dlcloseも残っています。
証跡は`out/checks/mochios-link-bundle-246-{host-final,results,runtime}.log`です。
カーネル、配布SDK、通常起動イメージの変更やKVMの起動は行っていません。

## runtimeの依存取得・exec接続（phase247）

`exec_image`から依存群のcaptureと統合ELF生成を呼び出すようにしました。
`DT_NEEDED`にslashがあればそのpathを、なければユーザー空間の設定
`DEFAULT_SHARED_LIBRARY_PATH`（`/lib:/usr/lib`）を検索します。mainの実行権限確認を維持し、
DSOは`open_at`のread permission / Capability検証を通します。通常のDSOと同様にmodeの
実行bitは要求しません。nonblockingで開いてから、同じ保持Handleの`fstat`で通常ファイルを
確認します。必要なnative rightsはREAD / TRANSFERです。sourceのcursorは変更しません。

検索を続けるのはopenそのもののENOENT / ENOTDIR / EACCESだけです。
Handle取得後のmetadata / readエラーはその候補の失敗として伝播し、他候補で上書きしません。
取得済みbytesは別々の所有bufferに固定し、名前で重複を抑えて依存を幅優先で取得します。
node配列が拡張されてもcapture済みのbytesは移動しません。rootを含むcapture合計と最終出力に
既存の`MAX_CAPTURED_IMAGE_BYTES`上限を適用します。metadata workspaceもnewlib mallocで
fallibleに確保し、Rust allocatorを追加してmallocを再帰させることはしません。

source、名前、作業buffer、一時Handleは既存のcallback resource ownerへ登録します。
read完了後に依存FileHandleを閉じ、認可へ進む前に統合RAM objectのwrite / resize参照を
破棄します。そのbytesのdigestを既存のimage-replace-inherit認可へ渡すため、パッケージの
再検索やpath再openで権限を追加する経路はありません。mnuへのABI追加はありません。

KVMではeuid 0 / 501の両方で、mode 0644の実DSOを使うpathname execを各3回、
unlink済みFDからのexecを各1回実行し、shared function、root定義のpreemption、
relative pointers、BSS、rootのTLS template / PHDR補助情報と共有FD cursorを確認しました。
不足した依存のexecを各12回試し、errno ENOENTとHandle数の維持も確認しました。
各親のwaitはstatus 0です。一般ユーザーを通すためのCapabilityやmodeの緩和はしていません。

ホストでは検索、readable regular Handle、権限不足、metadata / read / RAM / 認可の失敗、
transitive capture中のnode移動、未対応初期化処理の拒否と解放を確認しています。
metadataがENOENT / ENOTDIRを返しても取得済みHandleを別候補で上書きしない検査は、
KVM試験後の最終レビューで追加しました。関連結果は`mochios-dynamic-exec-247-results.log`、
KVMは`mochios-dynamic-exec-247-native.log`と`-artifacts.json`、target runtime buildは
`mochios-dynamic-exec-247-runtime-final.log`です（いずれも`out/checks/`）。

診断ELFを取り除いた通常構成でinput READY、Binder ready、デスクトップとDockを確認し、
VMを停止しています。証跡は`mochios-dynamic-exec-247-normal.{log,png}`です。
既存のnetwork `Failed(-6)`、USB driver creation errno 22は残っています。
配布SDKとRust std overlayは再生成していません。

現在の接続はinterpreterを要求しないdirect-entry ELFに限ります。
当段階ではconstructor / finalizer / arrayと検索指定を未対応としていました。
次節のphase248でRPATH / RUNPATH / LD_LIBRARY_PATHを接続しています。
interpreter、version scope、IFUNC、COPY relocation、
依存TLS module、dlopen / dlclose、constructor順序とライブラリ寿命管理は引き続き必要です。
これは汎用のglibc loaderや完全なPOSIX互換環境の完成ではありません。

## ライブラリ検索metadataと環境変数（phase248）

root / DSOの`DT_RPATH`、`DT_RUNPATH`、`DF_1_NODEFLIB`を解釈します。
[ld.soの検索規則](https://www.man7.org/linux/man-pages/man8/ld.so.8.html)を基準に、
requesterにRUNPATHがなければloaderの親をたどってRPATHを検索し、次に環境変数、
requester自身のRUNPATH、最後に標準pathを試します。RUNPATHは直接依存だけに適用し、
RPATHは親から継承します。RUNPATHが明示的に空でもRPATHより優先します。
NODEFLIBのあるrequesterでは標準pathを省略します。依存名にslashがある場合は
従来通りそのpathを直接開きます。ld.so.cacheは使用していません。

`LD_LIBRARY_PATH`は`exec_inputs`が所有する環境のsnapshotから取得し、getenvで
呼出し元の現在の環境を読み直しません。空の変数は検索指定なし、リスト内の空要素はcwd、
環境リストのcolon / semicolonは区切りとして扱います。各候補の長さを検証し、
切り詰めて別のファイルを開く経路はありません。open成功後のmetadataエラーでは、
引き続き別候補へ進まず、そのHandleを閉じてエラーを返します。

環境指定がある場合、認証されたPOSIX endpointの`session_info`からreal/effective UIDと
GIDを取得します。いずれかに差があればLD_LIBRARY_PATH / LD_PRELOAD / LD_AUDITを
loaderで使用しません。identity取得に失敗しても環境指定を許可するfallbackはしません。
権限差がない場合の非空LD_PRELOAD / LD_AUDITは、まだ注入処理がないためENOEXECです。
この段階ではloaderの検索制御だけでした。次節phase249で、対応済みの3変数を
secureなexecのchild環境から除去します。setuid / file-Capability遷移全体は未対応です。

検索metadataは取得済みの不変buffer内の検証済みspanとして保持します。
候補を試すたびに依存元ELFのrelocation全体を再解析しません。
DT_RPATH / DT_RUNPATHの重複、不正な文字列offset / 終端を検出します。
使われる検索リストや依存名の`$ORIGIN`等のtokenは現在ENOEXECです。fexecveのoriginを
呼出し元のpathから推測したり、unlinked済みファイルをpathで開き直したりしません。
正しいorigin処理には保持FileHandleに基づいた親directory情報の設計が残っています。

ホスト11対象・76テストが成功しました。実リンカのRPATH / RUNPATHの優先順位、
transitive継承の違い、NODEFLIB、cwd要素、環境textのsnapshot、UID / GID差、
identity取得失敗、不正metadata、既存exec認可・context・byte ceilingを確認しています。
KVMではbare SONAMEのDSOをRUNPATHで探すexecがroot / euid 501で成功しました。
別のRUNPATHなしimageについて、通常identityではLD_LIBRARY_PATHから読込み、
real UID 0 / effective UID 501では環境指定を無視してENOENTになることを確認しました。
pathname / unlink済みFDのexec、preemption、共有cursor、失敗時Handle維持も成功です。

証跡は`out/checks/mochios-library-search-248-{results,native,runtime}.log`と
`-artifacts.json`です。constructor / finalizer、interpreter、動的TLS、version scope、
IFUNC、COPY relocation、dlopen / dlclose、ライブラリ寿命管理は引き続き未完了です。

診断ELFを外した通常構成でもinput READY、Binder ready、デスクトップとDockを確認し、
VMを停止しました。通常起動の証跡は`mochios-library-search-248-normal.{log,png}`です。
既存のnetwork `Failed(-6)`、USB driver creation errno 22は残っています。
カーネル、配布SDK、Rust std overlayは変更していません。

## exec entryのidentityとsecure環境（phase249）

exec準備の最初にPOSIX endpointの`session_info`からidentityを一度取得し、
同じsnapshotをDSO検索とstartup stack生成へ渡します。取得失敗ではexecを中止し、
uid 0等の既定値へfallbackしません。
`encode_initial_stack_for_process`はreal / effective UIDとGID、AT_SECUREを最初から
literalな補助情報として出力します。C runtimeの初期化による後からの補正を待つ必要が
ありません。これらにkernel用relocationを追加していません。

`Identity::is_secure`を検索、stack生成、runtimeの補助情報更新で共用します。
UIDまたはGIDのreal / effective値が異なるexecでは、LD_LIBRARY_PATH / LD_PRELOAD /
LD_AUDITをchild環境から取り除きます。空値も除去し、同名prefixを持つ別の変数は残します。
argvや他の環境変数の順序・値を変えません。filterもexec時に取得した所有textを使います。

従来のbootstrap用encoderはidentityが未確定な起動経路の既定値を維持します。
認証済みPOSIX sessionがある呼出し側は新しいProcess contextを渡す必要があります。
今回確認するのはruntimeのexec経路です。service-manager等からの初回launch全経路、
setuid / file-Capabilityのexec時遷移、interpreterそのものの対応が完了したわけではありません。
補助vectorはuserspace向け情報であり、serviceが認証済みpeer identityやCapabilityの代わりに
信頼してはいけません。カーネルへUID/GIDやPOSIXサービスの意味を追加していません。

ホスト12対象・82テストが成功しました。literalなidentity、UID/GID差、u32最大値、
同じkeyの重複防止、環境の保持・除去、identity取得失敗時のexec中止を確認しています。
KVMではrootとeffective UID 501で共有ライブラリ付きexecを実行し、C runtimeを使わない
entryで元のstackを直接検査しました。4つのIDとAT_SECUREが正しく一度ずつ存在し、
通常identityでは空のloader環境3変数が残り、secureなidentityでは除去されていました。
pathnameとunlink済みFDからのexec、共有cursor、依存取得失敗時のHandle維持も成功です。
証跡は`out/checks/mochios-entry-identity-249-{results,native,runtime}.log`と
`-artifacts.json`です。constructor / finalizer、interpreter、動的TLS等は引き続き未完了です。

診断ELFを外した通常構成でinput READY、Binder ready、デスクトップとDockを画面でも
確認し、検証VMを停止しました。証跡は`mochios-entry-identity-249-normal.{log,png}`です。
既存のnetwork `Failed(-6)`、USB driver creation errno 22は残っています。
カーネル、配布SDK、Rust std overlayは変更していません。この検証ではホストの
rust-analyzerが動作しておらず、ホストプロセスの一時停止は行っていません。

## 既存CRTの終了順序（phase250）

共有ライブラリのlifecycleを接続する前に、既存のstaticなCRT経路で
`.fini_array`を先頭から実行していた不備を修正しました。preinit / initは先頭から、
finiは末尾から呼びます。信頼されたlinker所有の範囲を呼ぶ小さなrunnerへ分離し、
カーネルのProcessExitにconstructorやdestructorの意味を追加していません。
既存のatexit登録位置は維持し、アプリが登録した終了callbackがfiniより先に実行されます。

ホスト13対象・84テストが成功しました。追加のホスト試験はproductionのrunnerを使い、
実際のcallback順序と空・1要素の範囲を確認します。KVM試験は優先度付きconstructor /
destructor、preinit、atexitを含むC ELFを現在のSDKと更新済みruntimeでリンクしました。
親がfork / execした子の出力をpipeから最後まで読み、通常終了の`PABMYXba`と
`_exit`の`PABM`を比較しています。rootとeffective UID 501の両方で成功しました。
argv[0]は意図的に異なる値でも動作し、再execにはAT_EXECFNを利用します。
証跡は`out/checks/mochios-image-lifecycle-250-{results,native,runtime}.log`です。

この修正は既存のstaticなCRT経路が対象です。共有objectのDT_INIT / DT_FINIと配列、
依存順序、ライブラリ寿命管理を受け入れる変更ではありません。現在のdirect-entry
DSO scopeはrunner未接続の初期化要求を引き続きENOEXECとして拒否します。

更新済みruntimeで通常service-managerもリンクし直し、診断用ELFを外した構成で
input READY、Binder ready、デスクトップとDockを画面でも確認しました。検証VMは
停止済みです。証跡は`mochios-image-lifecycle-250-normal.{log,png}`と`-artifacts.json`です。
既存のnetwork `Failed(-6)`とUSB driver creation errno 22は残っています。
カーネル、配布SDK、Rust std overlayは変更せず、ホストプロセスの一時停止も不要でした。
