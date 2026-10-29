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

## 共有objectのlifecycle計画（phase251）

captured ELFのDT_INIT / DT_FINIとpreinit / init / fini配列を解析・検証します。
同じtagの重複、アドレスと長さの片方だけの指定、8byte単位でない配列、overflow、
読めるLOADに収まらない範囲を拒否します。DT_INIT / DT_FINIは自身の実行可能LOAD内に
ある必要があります。配列はrelocationで初期化されるzero-fill領域も表現できます。
この検証は通常の`Pie::parse_object`にも適用し、不正なmetadataをexecで見逃しません。

`Scope::lifecycle`はシンボル検索のBFSとは別のDFS postorderを作ります。
呼出し側の4×object数のusize workspaceを使い、再帰・allocation・global状態はありません。
共有依存を一度だけ記録し、循環は決定的に処理します。rootのpreinitを先に、各objectは
依存先を先に初期化します。DT_INITから順方向init配列へ進み、終了はobject順も配列順も
逆にしてからDT_FINIへ進みます。依存objectのpreinit要求は受け入れません。
順序の根拠は[System V ABIのInitialization and Termination Functions](https://gabi.xinuos.com/elf/08-dynamic.html#initialization-and-termination-functions)です。

計画はコードを呼ばず、再配置後のアドレスと配列範囲を返します。配列のfile上の値を
そのままfunction pointerとして使うことはできません。実行側は再配置後の値を読み、
`Lifecycle::is_callable`でcaptured scope内の実行可能LOADにあることを確認する必要が
あります。他objectの関数へ解決された配列要素も扱えます。

ホスト13対象・88テストとmochiOS target runtimeのbuildが成功しました。
追加試験ではroot＋3つの共有ライブラリを実リンカで作成し、diamond依存、DT_INIT /
DT_FINI、優先度付き配列、root preinit、他objectへのsymbol relocation、packed RELRを
含むbundleを生成します。ホスト上でそのbundleをmapし、fixtureのコードを実際に呼び、
初期化と終了の全順序を確認しました。SysV / GNU / 両hashと、section表あり・なしを
確認しています。これはmochiOSのProcessCreateやCRT接続の試験ではありません。
証跡は`out/checks/mochios-dso-lifecycle-251-{host,runtime}.log`と`-artifacts.json`です。

runtimeのexec / startupへの計画受け渡し、once-only実行、atexit登録、root CRTとの
二重実行防止、mapping寿命管理は次の接続段階に残ります。未接続のdirect-entry execは
非空の初期化・終了要求を引き続きENOEXECにします。共有ライブラリのlifecycle全体が
動作する状態には達していません。今回はホストで検証可能な解析・計画の変更であり、
KVMは起動せず、通常イメージ、カーネル、配布SDK、Rust std overlayは変更していません。

## execとCRTへのlifecycle接続（phase252）

execのproducerで、再配置済みbundleからcallbackを読み、captured scopeの実行可能
LOAD内であることを検証してから計画をserializeします。headerはmagic / version /
object数 / 全長 / preinit数を持ち、依存順のobjectごとにinitとfiniの範囲を記録します。
各配列のfile上の未再配置値を実行アドレスとして渡す経路はありません。
不正なcallbackではexecへ進まず、元のimageとFDを維持します。

`Process::lifecycle`のbinary payloadは初期stackへ8byte境界でコピーし、
`AT_LIFECYCLE_IMAGE`を既存の汎用StackAddress relocationで参照します。
kernelはpayloadも補助情報のkeyも解釈しません。現在のstartup stack容量を越える場合は
E2BIGです。callerによるkeyの上書きや、producerのaddress spaceのpointer持込みを防ぎます。

rootのdynamic symbolにstrongな`__mochios_lifecycle_v1`があり、その定義が自身の
実行可能LOADに属する場合に、対応CRTとして扱います。このmarkerは実行認可やidentity
ではなく、ユーザー空間のstartup contractです。対応しないrootへ共有objectの初期化要求を
渡したり、未実行のまま起動成功にしたりはしません。通常の認可経路は維持しています。

CRTはruntime / identity / thread / signal / stdioの初期化後に計画を読み、従来の
linker-symbol配列との二重実行を避けます。既存のatexit runnerから終了計画へ接続し、
登録失敗も起動失敗として扱います。payloadがない従来のstatic CRT経路は維持します。
payloadが存在するのに不正・読取り失敗の場合はstatic経路へfallbackしません。

共通のcodecとStateはallocationを使いません。header / extent / descriptorの連続性と
全callbackの読取りを検証し、bounds外やoverflowを拒否します。objectの開始数とphaseを
一つのatomic値で更新し、初期化途中で終了した場合も開始済みobjectだけを逆順に処理します。
二重初期化、二重終了、終了callbackからの再入で同じ処理を繰り返しません。
callback中にruntimeのlockを保持しません。stackと状態はforkのaddress spaceに残ります。
この計画が表す起動時objectはprocess imageのmappingとしてProcessExitまで保持します。
後からのdlopen / dlcloseの寿命管理を実装したわけではありません。

ホスト15対象・95テストとtarget runtime / 通常service-managerのbuildが成功しました。
実producer→startup stack→codecの接続、未対応CRTの拒否、壊れたcallbackでの元FD維持、
partial init、再入、faultable read、CRT adapterを確認しています。実リンカ生成の
DSO試験も、serializeした計画をproductionのcodec / Stateで実行します。
証跡は`out/checks/mochios-dso-exec-252-{host-final,runtime}.log`と`-artifacts.json`です。

KVMではroot＋3つの共有ライブラリをexecし、preinit、DT_INIT / DT_FINI、優先度付き
配列、他objectへのcallback、RELR、依存順、once-only実行を確認しました。
pathname execと、unlink済みFDからのexecがroot / effective UID 501の両方で成功し、
元FDの共有cursorも維持しています。証跡は`mochios-dso-exec-252-native.log`です。

実OSのconsumerはproductionのcodec / Stateをそのままincludeするfreestanding PIC entry
です。host runtimeをリンクせず、mnuのMemoryCopy / ProcessExitだけを利用します。
newlib CRT adapterはホストでproductionのsourceを検証し、targetにもbuildしましたが、
PIC版newlib SDK全体をリンクした動的CRTを実OSで確認したわけではありません。
SDKのPIC化、interpreter、依存TLS、dlopen / dlclose等は未完了です。

診断ELFと3つのDSOを外し、更新したruntimeでリンクした通常構成に戻しました。
input READY、Binder ready、デスクトップとDockを画面でも確認し、VMは停止済みです。
通常起動の証跡は`mochios-dso-exec-252-normal.{log,png}`です。既存のnetwork `Failed(-6)`、
USB driver creation errno 22と、画面に残るdebug logの一部は残っています。
カーネル、配布SDK、Rust std overlayは変更していません。

## 固定配置の動的rootと実CRT（phase253）

明示したlink scopeのrootとしてET_EXECを扱います。load biasは0で、entry、LOAD、
symbolの配置を変えません。固定imageを依存ライブラリとして使うことや、biasを与えて
再配置することは拒否します。ET_DYNの依存objectは従来のplacementで配置し、fixedな
rootの末尾とmetadata領域を越えた位置を使います。重複やuser window外の範囲は拒否します。

fixed rootの補助metadataは最後のLOADの後ろへ置きます。元PHDRがどのLOADにも含まれない
場合は、そのbytesを補助領域へそのままコピーし、AT_PHDRが参照するread-onlyなviewを
用意します。load bias＋file offsetから未mappingのprefixのアドレスを推測しません。
combined containerの補助情報も、全memberより低い領域に余地がなければ、全memberの
後ろの空き領域へ置きます。4 MiBから始まるrootを動かす必要はありません。

execのprobeはET_EXECのPHDRもpositional readで確認します。PT_DYNAMIC / PT_INTERPが
あればuserspaceの準備経路へ進みます。PT_INTERPそのものは引き続き未対応として拒否し、
カーネルへinterpreterの名前やpolicyを追加していません。静的なET_EXECは元Handleを
そのまま使い、RAM imageや依存探索を追加しません。従来native経路で扱えた56byte以上の
PHDR strideもprefixだけを読んで判定します。offset / extent / EOFは検証します。

ホスト15対象・99テスト、target runtimeとservice-managerのbuildが成功しました。
fixed rootの配置維持、bias拒否、固定依存imageの拒否、元PHDRのmapped / private-copy
両経路、copyのread-only保護、execの補助vector、静的imageの直接経路を確認しています。
静的imageについては通常のPHDRと、strideを拡張してfile末尾へ移したPHDRも確認しました。
証跡は`out/checks/mochios-fixed-dso-253-{host-final,runtime-final}.log`と
`-artifacts.json`です。

KVMでは既存SDKのcrt0.o / libc.a / libm.a / compiler runtimeと、更新済みnewlib runtimeを
使ってリンクしたET_EXECを実行しました。試験用linker scriptにdynamic / GOT等のsectionを
定義し、配布SDKのscriptやarchiveは変更していません。3つの共有ライブラリは0644で配置し、
root / effective UID 501の両方で次を確認しました。

- 実CRTからpreinit、依存先のconstructor、rootのconstructor、mainへ進む。
- stdio / getenv / pipe / fork / exec / waitを使い、親が子の出力から全callback順を検査する。
- atexitを共有objectのfiniより先に実行し、rootと依存objectを逆順で終了する。
- `_exit`ではatexit / finiを呼ばない。
- 最初の依存ライブラリのconstructor内で`exit`すると、開始済みのprefixだけを終了する。

このnative試験はphase252のfreestanding consumerではなく、実newlib CRTの経路です。
証跡は`mochios-fixed-dso-253-native.log`、生成ELFの構成は`-linker.log`に記録しました。
PIC SDK全体やglibcを作成したわけではありません。依存TLS、interpreter、COPY / IFUNC、
version scope、dlopen / dlclose、初回native launchへの同じ準備経路の共用は未完了です。

診断ELFとDSOを外して通常構成へ戻し、更新済みruntimeで通常service-managerも
リンクしました。input READY、Binder ready、デスクトップとDockを画面でも確認し、
VMは停止済みです。証跡は`mochios-fixed-dso-253-normal.{log,png}`です。
既存のnetwork `Failed(-6)`、USB driver creation errno 22は残っています。
カーネル、配布SDK、Rust std overlayは変更せず、生成されたservicesのCargo.lock更新は
今回の試験コミットに含めています。

## 複数moduleのstatic TLS配置と初期化（phase254）

`posix-client::static_tls::modules::Plan`に、root-firstのTLS template列から各moduleを
配置する共通処理を追加しました。heap allocationやkernel APIを使わず、TLSのないobjectも
元のobject indexを維持します。rootの従来のTP相対offsetを保ち、依存moduleはその前に
配置します。全moduleのalignmentとp_vaddr residueを満たし、領域を重複させません。
単一templateのLayoutも同じ計算を使います。Variant IIの配置条件は
[lldのgetTlsTpOffset](https://github.com/llvm/llvm-project/blob/main/lld/ELF/InputSection.cpp)
と整合する形です。

初期化はcallerが所有するzero-filledなprivate mappingへ行います。各moduleの初期値だけを
boundedなchunkでコピーし、.tbssとpaddingはzeroのまま維持します。全moduleのコピーが
成功した後にTCBのself pointerを書き込みます。templateの不正なalignment / extent、
累積sizeのoverflow、短いmappingやmapping終端のoverflowはcopy前に拒否します。

newlib runtimeの既存の単一TLS経路もこの共通処理へ接続しました。VMの確保とAS-limitの
判定は従来のnative mechanismを使い、copy失敗ではMappingを解放します。ThreadControlの
TP / mapping情報は成功後にのみ公開します。このallocatorへ複数templateを渡す経路も
hostからproduction sourceを直接使って検証しました。

ホスト15対象・104テストとmochiOS向けruntimeのbuildが成功しました。異なるalignment /
residue、root offsetの維持、非重複、TLSのないobject、zero-only module、各threadの独立性、
すべてのread / write / TCB書込み失敗でのrollbackを確認しています。実リンカで生成した
3つのTLS templateもproduction parserで読み、nmで得たsymbol offsetが初期値とzero領域を
指すことを検査しました。証跡は`out/checks/mochios-tls-modules-254-{host,runtime}.log`と
`-artifacts.json`です。

これはTLSの配置・初期化の共通化までです。起動情報へのmodule列のserialize、link scopeの
TLS symbol解決 / 再配置、DTV / __tls_get_addrは未対応です。依存TLSを持つobjectは従来通り
明示的に拒否し、部分対応のまま起動しません。新しいnative boundaryは追加しておらず、
今回はKVMを起動していません。host processの一時停止も不要でした。image、kernel、
配布SDK、Rust std overlayは変更していません。POSIX全体のgoalは未完了です。

## TLS起動情報のproducer / stack / CRT接続（phase255）

TLS template列を表すuserspace-onlyのcodecを`static_tls::image`に追加しました。
headerはmagic / version / object数 / 全長 / reservedを持ち、各objectをsource address、
file size、memory size、alignment、residueの5wordで表します。TLSのないobjectは全wordを
zeroにしたdescriptorで表し、object indexを変えません。sourceはprepared childのLOAD内に
置くtemplateのアドレスであり、producerのcapture bufferを指すpointerは渡しません。

decoderはfaultable copyを使い、未知version、reserved、非正規な長さ、件数・容量超過、
alignment / extent / 累積sizeの不正を拒否します。scratchへのdecodeとPlanの検証が
成功するまでVMは確保しません。codec自身はallocationもkernel APIも使いません。

`Process::tls`のbinary dataを初期stackへ8byte境界で配置し、`AT_TLS_IMAGE`を既存の
StackAddress relocationで参照します。lifecycle payloadとの非重複と、両方を合わせた
MAX_STACK_BYTESの上限を検査します。callerによるaux keyの上書きは禁止します。
kernelへTLS専用のkey解釈や構造体は追加していません。

exec producerは捕捉した各objectのPHDRからchild biasを適用したtemplate列を生成します。
CRTがstrongな`__mochios_static_tls_v1`を自身の実行可能LOADに持つときだけ新しい情報を
渡します。このmarkerは認可ではありません。依存TLSを扱えないCRTは拒否し、rootのみの
TLSで従来のPHDR経路を使うimageには、解釈できない補助情報を追加しません。

CRTはmain / pthreadのTLS確保時に起動情報をdecodeし、phase254の複数module allocatorを
使います。metadataが存在する場合の不正・読取り失敗ではrootのPHDRへのfallbackをせず、
失敗を返します。forkでは元の起動stackとimageのmappingを継承します。descriptorの列は
初期scopeのものとして保持し、dlopenによる追加moduleの管理はまだ実装していません。

ホスト16対象・110テストとmochiOS向けruntimeのbuildが成功しました。codecのround trip /
corruption / truncation / read error、stackのalignment / relocation / 合計容量、production
CRTによる複数moduleの初期化と不正metadataの拒否を検証しました。実リンカ生成のroot TLSを
捕捉してexec producerを通し、startupに含まれるtemplateが正しいchild LOADと初期値を
指すことも確認しています。証跡は`out/checks/mochios-tls-image-255-{host,runtime}.log`と
`-artifacts.json`です。

依存TLSを許可する前に必要なTLS symbol解決・再配置、DTV / __tls_get_addrは未対応です。
Scopeの依存TLS拒否は維持しています。この段階で共有ライブラリのTLSが実OSで動作したと
主張するものではありません。今回はKVMを起動せず、image、kernel、SDK、Rust std overlay
も変更していません。first native launchへの準備経路の共用など、既記の残件も未完了です。

## 初期scopeのTLS再配置とresolver（phase256）

link scopeがSTT_TLSを扱い、R_X86_64_DTPMOD64 / DTPOFF64 / TPOFF64を解決します。
TLS symbolの値はvirtual addressではなくmodule内offsetとして扱い、sizeを含む範囲が
PT_TLSのmemory sizeに収まることを検査します。通常のaddress relocationとTLS symbol、
TLS relocationと非TLS symbolの混同は拒否します。section headerは利用しません。

module IDは元のobject index＋1です。TLSを持たないobjectも番号を消費するため、
execのtemplate列とCRTの参照表で同じIDを維持します。symbol lookupには従来のBFS scopeを
使い、rootによる定義の上書き、local / hidden / protected / symbolicの規則を維持します。
symbol zeroのTLS relocationは呼出元objectのmoduleとして処理し、local-dynamic codeの
module参照にも使います。TPOFFはphase254と共通の配置計算から求め、rootのlocal-exec
offsetを変えません。modularなaddendはELF64の値として処理します。
再配置番号と式は[LLVMの定義](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm/BinaryFormat/ELFRelocs/x86_64.def)
および[x86-64の再配置処理](https://github.com/bminor/glibc/blob/master/sysdeps/x86_64/dl-machine.h)
と照合しました。これらのソースをコピーした実装ではありません。

CRTは各threadのprivate TLS mappingのTCBに参照表へのpointerを置き、その後にmodule数と
各moduleのbase / sizeを記録します。TLSのないentryはzeroです。全templateと参照表を
書き終えてからself pointerとThreadControlを公開し、途中のcopy失敗ではmappingを解放
します。参照表の領域もmappingの容量計算に含め、同じthreadの退役時に一緒に解放します。

`__tls_get_addr`はcompilerが渡すmodule / offsetを受け取り、現在のthreadの登録済みcontrolと
参照表からaddressを返します。参照表・module・offsetはmappingの境界内で検査します。
通常の参照でPOSIX service RPC、heap allocation、追加mappingは行いません。kernelには
TCB、module ID、ELF TLSの概念や専用APIを追加していません。

依存TLSを持つimageは、対応CRTのmarkerを確認してphase255の起動情報を渡す経路でのみ
execします。単独imageでもTLS relocationがあれば明示scopeへ進め、以前の単独PIE処理で
未解決のまま起動しません。未対応CRTでの依存TLS、未定義weak TLS、TLSDESCは明示的に
拒否します。dlopen後に増えるmoduleの配置や世代管理は未対応です。

ホスト17対象・114テストとtarget runtime / service-managerのbuildが成功しました。
実リンカ生成のGD / LD / IE codeについて、sysv / gnu / both hash、section headerあり・なし、
TLS定義の上書きと全再配置値をnm / readelfの結果と照合しました。symbolの型・範囲、
local module、未定義weak、未対応CRTの拒否、参照表の破損、全copy失敗でのrollbackも
確認しています。証跡は`out/checks/mochios-dso-tls-256-host-final.log`、`-runtime.log`、
`-linker.log`、`-artifacts.json`です。

KVMでは既存SDKのCRT / newlibと更新runtimeを使うET_EXECを実行しました。rootのlocal-exec、
共有ライブラリのGD / LD / IE、TLSへのsymbol preemption、constructorからの参照、
8192byte alignmentと.tbss、並行するpthreadの独立性、fork COW、fork後の新しいpthreadで
初期値へ戻ることを確認しました。pathnameとunlink済みFDの両exec経路がroot / effective
UID 501で成功し、元FDのcursorも維持しています。証跡は`-native.log`です。

診断用imageとライブラリを外し、更新runtimeでリンクした通常service-managerへ戻しました。
input READY、Binder ready、デスクトップとDockを画面でも確認し、VMは停止済みです。
証跡は同phaseの`-normal.log` / `-normal.png`とartifact recordです。
既存のnetwork `Failed(-6)`、USB driver creation errno 22と、画面に残るdebug logの一部は
今回の対象外です。
SDKやRust std overlayは再生成していません。interpreter、COPY / IFUNC、version scope、
dlopen / dlclose、動的TLS追加、first native launchへの準備経路の共用などは未完了です。
この初期TLS scopeの動作確認をもって、完全なPOSIX環境のgoalを完了とはしません。

## 初期scopeのTLSDESC（phase257）

R_X86_64_TLSDESCを16byteの再配置として扱います。parse時のLOAD範囲、重複、overflowと、
書込み先のmaterializationを同じ幅で検査します。後半8byteだけがLOADを越える場合や、
別のrelocationがdescriptorの後半に重なる場合も拒否します。従来の8byte relocationとの
隣接は許可し、RELRなどの既存の検査は維持します。

descriptorの先頭にはroot CRTのstrongなresolver、後半にはTP相対offsetを設定します。
rootの実行可能LOADに対応するresolverがなければ準備を失敗させます。任意の依存先から
同名のfunctionを探して代用しません。定義のあるTLSは既存のscope / 配置規則を使います。
未定義weak TLSのTLSDESCは専用resolverでaddend−TPを返し、callerがTPを加えた結果を
addend、通常はnullにします。これはclassic GDの未定義weak TLSへの対応とは別です。

CRTのresolverはdescriptorをRAXで受け取り、RAX以外のregisterを変更せずに戻ります。
通常の場合はdescriptorのoffsetを返すだけで、weakの場合だけ現在のTCBのself pointerを
差し引きます。service RPC、allocation、runtime lockや追加mappingは行いません。
[x86-64のTLSDESC ABI](https://github.com/bminor/glibc/blob/master/sysdeps/x86_64/dl-tlsdesc.S)
を照合し、呼出規約を満たす短いassemblyとして独立させました。kernelへの変更はありません。

ホスト18対象・116テストが成功しました。GNU / GNU2 dialect、sysv / gnu / both hash、
section headerあり・なしで実リンカの結果と照合し、未対応CRT、不正な16byte範囲や重複を
検証しました。productionのresolverを直接呼ぶABI試験では、RAX以外の汎用registerと
XMM0〜15をliveにしたまま保持を検査し、通常offsetとweakのzero / 正負addendを確認しました。
証跡は`out/checks/mochios-tlsdesc-257-host-final.log`、`-linker-final.log`、
`-runtime.log`、`-linker.log`と`-artifacts.json`です。

KVMではphase256のC / newlib CRT試験をGNU2 dialectでも実行し、未定義weak TLSのnullを
確認する試験も追加しました。constructor、並行pthreadの独立性、fork COWと新しいthreadの
初期値、pathname / unlink済みFDのexecがroot / effective UID 501で成功しました。
診断用ファイルを外して通常構成へ戻し、input READY、Binder ready、デスクトップとDockも
画面で確認しました。VMは停止済みです。証跡は同phaseの`-native.log`、`-normal.log`、
`-normal.png`とartifact recordです。既存のnetwork `Failed(-6)`、USB driver creation errno 22、
画面に残るdebug logの一部は残っています。SDKとRust std overlayは再生成していません。
dlopen後のTLS追加・世代管理、interpreter、COPY / IFUNC、version scope、first native launch
への準備経路の共用などは未完了で、POSIX全体のgoalは継続中です。

## native launchへの起動情報の共用（phase258）

`launch_arguments::encode_native_launch_for_process`と、capability serviceの
`native_app::metadata_for_process`が、execと同じ`Process`のentry / PHDR / identity / lifecycle /
TLS情報を受け取り、共通の初期stack encoderへ渡します。従来のentry pointはdefaultのProcessで
同じ処理を呼び、静的imageの既存のstartup形式とHandleGrant / Valueを維持します。
引数の追加コピーは作らず、既存のexec snapshotとlauncherのString列を使います。

secureなidentityでのLD_LIBRARY_PATH / LD_PRELOAD / LD_AUDITの除去をprocess_imageへ集約し、
exec側の独自の除去処理を外しました。除去後の件数・text sizeから初期stackを構築するため、
pointer列とpayloadの間に除去した環境変数を残しません。同じ名前の重複entryも除去します。
他の変数の順序・重複・非UTF-8 bytesは維持し、不正なembedded NULは除去対象でも拒否します。
新しい名前を追加するときも共通のIdentity policyを使います。

`Process`はlocal loaderが確定した起動用dataであり、サービスの認証情報ではありません。
remoteのアプリ起動requestに新しいfieldは追加していません。remote argvからHandle番号、
identity、prepared payloadを選ぶことはできず、native_appは従来通りargv / environment以外の
`__MNU_EXEC_`指定を拒否します。唯一の継承endpointはIPCで受け取ったlocal Handleです。
imageの実行認可、POSIX child登録、resumeの順序も変更していません。

ホスト20対象・131テスト、target runtimeとcapability serviceのbuildが成功しました。
共通encoderでのsecure environment、除去対象を含む容量計算、nativeのprepared PHDR / entry /
lifecycle / TLS、正しいHandleGrant、remote指定の拒否を確認しました。既存exec経路のsecure
identity / environment検査も通っています。証跡は`out/checks/mochios-launch-startup-258-host.log`、
`-runtime.log`、`-capability.log`、`-artifacts.json`です。今回はportableなencode処理の変更で、
KVMは起動していません。image、kernel、SDK、Rust std overlayも変更していません。

この段階ではnativeアプリの最初の起動にDSO capture / relocationを接続していません。
調査したnative_appはcapability brokerが起動を仲介しますが、execのdependency openはcallerの
POSIX environmentで行います。共有preparerのreaderをbroker自身のfs.read.allへ単純に置き換える
ことはしません。対象launchのnamespace / credentialsとCapability ceiling、捕捉したroot Handle、
prepared bytesに対する認可を明示してから接続します。kernelへサービス名やPOSIX専用の起動
interfaceを追加せず、今回用意した同じProcess startupへ接続する方針です。その他の既記の
残件も未完了で、完全なPOSIX環境のgoalは継続中です。

## 明示的に委譲する読み取りスコープ（phase259）

native launchの依存先をbroker自身の権限で開かないため、POSIX client / serviceに
読み取り専用のスコープを追加しました。作成requestにはsubject PIDやUIDを載せず、
IPCで認証した呼び出し元自身のroot / cwd、real / effective UID / GID、補助group、umaskを
捕捉します。既存のpathname snapshot処理でdirectory Handleを保持し、filesystem RPC中には
POSIX stateのmutexを保持しません。作成完了前後のnative imageの検証も既存経路を使います。

スコープは既存のnative Provider objectで表現します。返す権限はDUPLICATE / TRANSFERだけで、
利用requestにはTRANSFERに縮小したHandleを一つ添付します。所有endpointのthreadだけが
ProviderObjectIdentifyで確認でき、wire整数や他providerのobjectからスコープを選べません。
receiverのroot / cwdや資格情報へ置換せず、保持したものだけで読み取りを行います。
ファイルはatomicなopen後にREAD / DUPLICATE / TRANSFERへ縮小し、WRITE / CONTROL / SEEK /
MAPを渡しません。縮小の失敗時も元のファイルHandleを閉じます。

読み取りrequestはrootまたはcwdを基準とし、既存open-atのwire形式を使います。
create / truncate / append / executeや任意directoryの注入は拒否し、FIFOのopenで制御threadを
停止させないようNONBLOCKを必須にします。対象ファイルの内容・属性やnamespace自体を
freezeする機能ではありません。確定したファイルHandleからimage bytesを捕捉する処理と、
最終的な実行認可は別に必要です。

作成元の資格情報・cwdの変更や終了は、すでに委譲したスコープを失効させません。
通常の委譲Handleと同様に、最後の参照が失われた時点のnative OP_RELEASEでdirectory参照と
管理entryを解放します。通常IPCで同じbytesを送っても解放できません。object IDを再利用せず、
遅れた通知による別entryの解放を防ぎます。保持数は全体256、作成元processごと16に制限し、
上限ではEAGAINを返します。close専用RPCやkernelのPOSIX専用機構は追加していません。

ホスト21対象・638テストが成功しました。新規8件では資格情報とdirectory参照の固定、
不正な権限・object・IDの拒否、作成・保持・権限縮小の失敗時の後始末、上限と解放を検証しました。
KVMではUID 501の子からUID 0の親へスコープをIPC転送し、子のcwd変更・終了後も元のdirectoryを
参照すること、親が通常openで読めるroot専用ファイルをスコープでは読めないことを確認しました。
nativeでWRITE / CONTROLへの権限拡大、別providerによる偽装、通常IPCによるrelease偽装を拒否し、
連続40回の作成・解放で最終参照の回収も確認しました。

証跡は`out/checks/mochios-read-scope-259-{host,service,manager,native}.log`と
`-artifacts.json`です。通常構成へ戻し、input READY、Binder ready、デスクトップとDockを
画面でも確認しました。VMは停止済みです。同phaseの`-normal.log` / `-normal.png`へ記録しました。
既存のnetwork `Failed(-6)`とUSB driver creation errno 22は残っています。
許可されたrust-analyzer等の一時停止対象は見つからず、ホストで停止した処理はありません。
SDK、Rust std overlay、kernel、通常のpackage権限は変更していません。
この段階ではnative_appへ共有DSO preparerを接続していません。スコープを利用するreader、
prepared bytesの実行認可とchild登録の接続、その他の既記の残件を含め、goalは継続中です。

## execと委譲readerの共通ローダー（phase260）

newlib内に残っていたimage capture、依存先の探索・取得、scopeの組立て、配置・再配置、
lifecycle / TLS情報の生成を`posix-client::image_preparation`へ移しました。execはこの共通処理を
呼び、旧`exec_image/dependencies.rs`は削除しました。ELFや探索順の実装をbroker用に複製せず、
nativeのread / write / random / MemoryObject作成とライブラリ取得をbackendから受け取ります。
出力は読み取り専用の準備済みimageと起動情報であり、実行の認可・置換・ProcessCreateは行いません。

メモリ確保とHandleの所有は、従来通り外側のPreparationへ先に公開します。newlibは同じ
callback resource chainへownerを登録し、資源の確保・公開・解放ではcallback deliveryを
deferします。I/O用の一時配列やELF viewは資源を所有せず、C handlerがexecを放棄した場合にも
外側のownerが回収します。既存のmalloc / freeを使うため、共通化のために別のglobal allocatorや
SDK変更を導入していません。allocatorとcallback管理のunsafe契約はbackendへ明記しました。
準備はownerごとに一度だけ許可し、失敗後の部分状態を再利用しません。

`image_preparation::scoped::Scoped`はphase259の委譲Handleから取得した資格情報を保持し、
ライブラリのopenをそのスコープへ限定します。backendの通常openへのfallbackはありません。
取得した資格情報をsecure searchに使い、起動情報の構築にもこの値を取得できます。
brokerのsessionやgetenvから資格情報・探索条件を代用しません。
ファイルの種類は開いたHandleで確認し、内容はcursorを変更しないpositional readで捕捉します。
スコープはcallerからの借用であり、loaderが勝手に閉じません。

ホスト21対象・640テストが成功しました。従来のexec / TLS / lifecycle試験は共通処理を通ります。
追加試験では委譲されたsecure identityでのLD_*除外、読み取り拒否時にambient openへ逃げないこと、
ownerの再利用拒否、全allocation位置での失敗注入と部分資源の回収を確認しました。
KVMでもUID 501の委譲スコープをUID 0側で使い、実IPCでのDSO取得、secure search、固定root、
lifecycle、TLS templateと空module slot、元ファイルのcursor維持を確認しました。

さらに既存SDKの実CRT / newlibで、GD / LD / IE / TLSDESC、constructor、並行pthreadの独立性、
fork COWとfork後の新thread、pathname / unlink済みFDからのexecがUID 0 / 501で成功しました。
試験初回はIEライブラリにもPT_TLSがあるという試験側の誤った期待値で停止しました。
readelfで同ライブラリがTLS参照のみであることを確認し、空slotを保持する検査へ修正して再実行しました。

証跡は`out/checks/mochios-shared-loader-260-{host,runtime,manager,native}.log`、
`-linker.log`と`-artifacts.json`です。診断用ファイルを外し、通常構成へ戻してinput READY、
Binder ready、デスクトップとDockを画面でも確認しました。VMは停止済みです。
同phaseの`-normal.log` / `-normal.png`へ記録しました。既存のnetwork `Failed(-6)`とUSB driver
creation errno 22は残っています。rust-analyzer等の一時停止対象は今回も見つかりませんでした。
SDK、Rust std overlay、kernel、通常packageの権限は変更していません。
native_appの最初の起動ではまだこのpreparerを使っていません。起動requestへのスコープ添付、
準備済みbytesへの実行認可、捕捉した資格情報・namespaceとchild登録の整合を保つ接続が必要です。
その他の既記の残件と完全なPOSIX環境のgoalも継続中です。

## スコープと子登録の整合（phase261）

`register_launched_child_in_scope`を追加しました。requestは空payloadで、読み取りスコープを
TRANSFER、requesterをWAIT、停止中の子をWAIT / SIGNALへ縮小してIPC添付します。
PIDや資格情報をrequest bytesから採用しません。serviceは自身のProvider objectを識別し、
二つのProcessHandleをnative APIで観測してから、POSIX stateのlock下で登録を判定します。

スコープの所有元とrequesterの一致、実際のbroker sessionのPOSIX初期化権限を確認します。
requesterのnative image revisionを再観測し、捕捉時のrevision、real / effective / saved IDs、
groups、umaskが現在と一致することを検査します。root / cwdは保持したnative objectとの
同一性を既存Handle APIで確認し、path文字列や再利用されるselectorだけでは比較しません。
観測・比較はnativeの局所操作であり、state lockを持ったfilesystem RPCは追加しません。

変化していればEAGAINを返し、子のPOSIX登録やHandle所有の引受けは行いません。
callerは停止中の子を破棄し、新しいスコープとimageを準備し直せます。観測失敗も伝播し、
無関係なscopeや別ownerを借りて登録することはできません。読み取りスコープ自体の寿命は
従来通りで、資格情報変更・exec・終了後に委譲済みの読み取り権限を暗黙に失効させません。
子登録の条件だけを、準備と起動時の環境が整合する形に限定しています。

成功時は既存のadopt処理でrequesterの親子関係・process group / session・signal・rlimit・
directory参照を継承し、fresh imageとしてsaved IDsとsignal dispositionを処理します。
失敗時の部分的なdirectory複製も回収し、成功時だけ子のProcessHandleをserviceが保持します。
native Capabilityや実行認可は変更しません。kernelへの変更・専用interfaceの追加はありません。

ホスト21対象・646テストとtarget service / managerのbuildが成功しました。新規6件では、
正常な継承、資格情報・groups・umask・root / cwdの変化、資格情報が同じでもnative imageが
変わった場合、foreign owner、観測・保持失敗、余計な返信Handleの回収を検証しました。
KVMでは実際のProcessCreateで停止した子を使い、UID 501のrequesterからのスコープを
UID 0のbrokerで扱いました。子の実newlib CコードでUID/GID、親PID、cwd、groups、umaskと
RLIMIT_NOFILEを確認し、requesterからのwaitも成功しました。stale cwd / credentialsと別ownerは
子を未登録のまま拒否し、callerがnativeの子を停止状態から破棄できることも確認しました。

試験初回は試験用Rust FFIでgid_t配列を32bitと宣言していました。SDKとruntimeの16bit ABIに
合わせて修正し、再実行で全ケースが成功しました。SDKそのものは変更していません。
証跡は`out/checks/mochios-scoped-launch-261-{host,service,manager,native}.log`と
`-artifacts.json`です。診断用の子プログラムとpackage recordを除き、通常構成へ戻しました。
input READY、Binder ready、デスクトップとDockを画面でも確認し、VMは停止済みです。
同phaseの`-normal.log` / `-normal.png`へ記録しました。既存のnetwork `Failed(-6)`とUSB driver
creation errno 22は残っています。一時停止対象のrust-analyzer等は見つかりませんでした。
SDK、Rust std overlay、kernel、通常packageの権限は変更していません。

native_appの既存起動requestはまだこのスコープを添付していません。callerが選んだDSOを
そのまま認可済みapplicationのCapabilityで実行しないよう、root / DSOを含む準備済みbytesと
packageの実行認可の対応も確定してから接続します。状態の整合だけで実行認可を代用せず、
両方を保つことが次の作業です。完全なPOSIX環境のgoalとその他の残件は継続中です。

## 捕捉済み入力とpackage認可（phase262）

共通preparerのBackendに`validate_captured`を必須の方針として追加しました。rootは全byteの
捕捉後・ELF parse前、DSOも捕捉後・parse前に呼びます。DSOにはDT_NEEDED名だけでなく、
実際にopenが成功した検索候補のpathを渡します。Scoped readerもこの検証を委譲します。
拒否はそのまま伝播し、出力imageを作らず、取得済みHandleとbufferをownerが回収します。
既存newlib execは自身のsecurity domain内の置換なので入力を受け入れる方針を明示します。
この入力検証自体は、実行permitや新しいCapabilityの付与を行いません。

capability側には、verification.binで認証した同一manifest byte列をparseして返す
`verified_manifest`と、そのsnapshotから作る入力Catalogを追加しました。先にparseした
package indexのコピーへ、後で読んだ別manifestの認証結果を付ける形にはしません。
既存install recordのmanifest digestと、`[[file]]`のsize / SHA-256を使い、ELF署名の
暗号処理や鍵管理は追加しません。

rootは`kind = "application"`、DSOは同manifest内の`kind = "library"`を明示したbinaryに
限定します。両方とも`file` IDで対応するdigest recordへ結び付け、binaryの絶対pathと
fileのインストール先を一致させます。`$/`は既存application bundleの配置規則で解決します。
選ばれたpathはrecordを選ぶためにだけ使い、真正性は捕捉済みbyte列のsize / hashで確認します。
basename、別packageの署名だけ、通常のdata file、相対pathやpath aliasへのfallbackはありません。
共有する別packageのDSOを認可する依存宣言は、この段階ではまだ追加していません。

ホスト22対象・658テスト、target capability.serviceと単独newlib runtimeのbuildを検証しました。
新規9件は、入力の改変・切り詰め・追記、不正なfile bindingやdigest、manifest更新、
検索候補の取り違え、拒否時の資源回収、捕捉後に元fileが変わっても同一byte列をparseすることを
確認します。証跡は`out/checks/mochios-capture-policy-262-{host,capability,runtime}.log`と
`-artifacts.json`です。この段階ではIPC / native境界を変更せず、KVM試験やimage書換えは
行っていません。SDK主要4fileのhashはphase261と同一で、SDK / Rust std overlayは未変更です。

Catalogはまだnative_appの起動経路へ接続していません。次に、callerからのread scope添付、
Catalogによる全入力の検証、準備済み出力へのpermit、scopeと整合する子登録を一つの起動経路へ
接続します。今回の検証をもって、アプリのDSO初回起動や完全POSIX環境の完了とはしません。

## アプリ初回起動への接続（phase263）

Binderとworkspaceのlaunch requestへ、既存POSIX endpointとcaller自身が捕捉したread scopeを
明示的に添付するようにしました。endpointはSEND / DUPLICATE / TRANSFER、scopeはTRANSFERに
縮小します。capabilityは両方の添付を検証し、処理後は成功・失敗とも受信参照を閉じます。
callerもIPCの成否にかかわらず捕捉したscopeを閉じます。request内のPID、UIDやHandle番号を
読み取り権限・継承元の認証には使いません。

read scopeのopenへREAD / NONBLOCKに加えEXECUTE検査を指定できるようにしました。
root executableのmode検査にも捕捉時の資格情報を使い、brokerのUID 0で代用しません。
返すHandleは従来と同じREAD / DUPLICATE / TRANSFERで、実行permitを付与する操作ではありません。

権限確認・promptへ進む前に、scopeの所有元も確認します。`OP_SOURCE_INFO`は空requestとscopeの
添付だけを受け、POSIX endpointが認証したobjectから捕捉時のprocess IDとsession情報を返します。
brokerはその所有元をnative IPC senderのprocess IDと照合し、別processのscopeを早期に拒否します。
grant判定にはscopeのUID / GIDを使い、途中の資格情報の再観測やbrokerの資格情報を代入しません。
子登録直前には、既存の検査で現在の状態との整合も再確認します。requestにPID selectorはありません。

アプリのCapability、identity、入力Catalogは同じ認証済みmanifest / install recordのsnapshotから
決めます。prompt中のpackage更新によって別snapshotのcertificate allowanceへ取り違えません。
rootをscope経由で一度捕捉し、Catalogで検証してからparseします。DSOも同じscopeのroot / cwd /
credentialsで取得して共通preparerから検証します。rootは専用RAM objectへ捕捉済みbyte列を置き、
新たなpath readや変更可能なsource objectをpreparerへ渡しません。

ファイル表のない既存の静的組込みアプリには、brokerのインストール先から読んだbyte列を
基準とする従来の認可方針を維持します。ただし、callerのscopeから捕捉した内容との一致も
検証します。ファイル表のあるpackageにはこの規則を適用せず、file bindingを必須とします。
動的rootにはこの規則を使わず、DSOにもインストール済みpathだけを根拠とした認可は行いません。

共通preparerは、出力RAM objectと同一のbyte列をownerから借りられる形で返します。
capabilityはその準備済み出力のdigestへpermitを発行してからnativeの停止状態で子を作成します。
startupへscopeのidentity、準備したentry / PHDR、lifecycleとTLSを入れ、scopeと整合する子登録が
成功してからresumeします。登録・resumeの失敗時は停止した子の破棄と登録の取消しを行います。
新しいkernel APIやkernel側のpath / service policyは追加していません。

ホスト27対象・676テストと、runtime / capability / POSIX / workspace / Binder / managerのtarget
buildが成功しました。KVMでは実際のアプリ起動IPCを使い、9ケースを検証しました。effective UID 0 / 501の
動的アプリで実newlibのconstructor、GD / LD / IE / TLSDESC、pthread、forkと、親PID、cwd、groups、
umask、rlimitの継承を確認しました。静的アプリも成功し、許可されていない検索候補、承認済みpathの
DSO改変、chroot内のroot改変、実行mode不足、stale / foreign scopeをそれぞれ拒否しました。
失敗した起動がcallerへPOSIX childを残さないことも確認しました。

最初の診断fixtureは二つのpackageが同じlibrary pathを登録し、indexの重複検査で拒否されました。
fixtureだけを修正しました。次の試験は既存launch replyの正数errnoを負数と期待していたため、
試験側を修正して全ケースを再実行しました。これらのログも`-first-native.log` / `-second-native.log`
へ保存しています。
所有元の早期検査を追加した後は、request opcode一覧とclientの返信flags検査への登録漏れを
ホスト試験が検出しました。両方を登録してホスト全対象と実IPCの9ケースを再検証しました。

診断用アプリ / packageを除いた通常構成へ戻し、input READY、Binder ready、デスクトップとDock、
DockからのFiles起動とHomeの表示を画面でも確認しました。最初の通常起動では、診断用runtimeの
trace-ioがDebugWriteで画面を上書きしていました。単独runtimeをtraceなしでbuildして新規binaryを
再linkし、表示の復旧を確認しました。UIのsourceは変更していません。VMはすべて停止済みで、
一時停止管理も解除済みです。今回は停止対象のrust-analyzer等はありませんでした。

証跡は`out/checks/mochios-scoped-application-263-*.log`、`-artifacts.json`、`-normal.png`と
`-files.png`です。SDK主要4fileのhashはphase262と同一です。SDK / Rust std overlayは再生成せず、
通常packageのCapabilityも変更していません。既存のnetwork `Failed(-6)`とUSB driver creation
errno 22は残っています。別packageの共有DSOの依存宣言、interpreter、versioned symbols、
dlopen等の既記の残件と、完全なPOSIX環境のgoalは継続中です。

## 別packageの共有ライブラリ依存（phase264）

署名済みmanifestに`[[library_dependency]]`を追加しました。package ID、developer、署名鍵ID、
provenance、ABIを必須とし、未知の項目・重複・不完全な宣言は拒否します。利用側がarchitectureを
指定した場合はそれも一致させます。schemaの詳細は`docs/mpkg.md`に記載しています。

capabilityの入力Catalogは、宣言したpackageの認証済みmanifestを起動ごとに取得し、その
`kind = "library"`のbinaryと明示的な推移依存を承認します。利用側へproviderのbinary / file表を
複写しないため、同じexportを複数packageが所有するindex登録を避け、複数アプリで共有できます。
providerのCapabilityや実行identityは加算しません。通常のdata、application、serviceのbinaryも
libraryの承認対象にしません。各libraryのfile binding、配置先、捕捉したsize / SHA-256の検査は
従来どおり必須です。

同一packageは一回の起動で一度だけ認証し、共有依存・循環依存も同じsnapshotを使います。
既読のpackageへのedgeも制約を再検査し、矛盾する鍵やABIの指定を省略しません。更新後の
developer / 鍵 / provenance / ABIが一致すれば新しいfile digestを使えるため、互換性のある
共有library更新のたびに各アプリのmanifestを更新する必要はありません。古い起動のsnapshotは
変更しません。graphはrootを含む256 package / 8192 libraryに制限し、再帰呼出しを使いません。

ホスト27対象・685テスト、capability / signature / package / 診断用managerのtarget buildが
成功しました。新規9テストはschema、認証、互換更新、推移依存、循環、exportの重複、資源上限と
拒否時のfallback不在を確認します。KVMでは実際のlaunch IPCを使う9ケースが成功しました。
二つのアプリで同じ循環依存を共有し、effective UID 0 / 501で実newlibのTLS・pthread・forkと
constructorを実行しました。鍵 / ABI / provenanceの不一致、未インストール、未宣言、DSO改変を
拒否し、元のfileへ戻すと再び起動できることも確認しました。失敗した起動はPOSIX childを残しません。

診断用package / アプリを除いて通常managerへ戻し、input READY、Binder ready、デスクトップとDock、
DockからのFiles起動、Homeの表示、ウィンドウ終了を実画面でも確認しました。検証VMは停止済みで、
一時停止対象のprocessは0件でした。一時停止管理も解除しました。SDK主要4fileのhashはphase263と同一で、
SDK / Rust std overlayは再生成していません。kernel、通常packageのCapability、UI sourceも
変更していません。証跡は`out/checks/mochios-shared-package-264-*.log`、`-artifacts.json`、
`-normal.png`と`-files.png`です。依存先の自動install、interpreter、versioned symbols、dlopen等は
今回の範囲に含めず、完全なPOSIX環境のgoalと既記の残件は継続中です。

## GNUシンボル版管理（phase265）

共通userspaceローダーで、`DT_VERSYM`、`DT_VERDEF / DT_VERDEFNUM`、
`DT_VERNEED / DT_VERNEEDNUM`を扱うようにしました。section headerへ依存せず、捕捉済みの
LOAD範囲から読みます。tagの重複、欠けた組、revision、件数、文字列、hash、15-bitの版IDを
検査します。record / auxiliaryの連結は件数と前向きoffsetで終端を確認し、越境・重複ID・
未定義ID・定義と要求の取り違えを拒否します。固定bitsetでID重複を確認し、heap allocationや
再帰呼出しを追加しません。COPYを必要とするsymbolは引き続き未対応として拒否します。

明示的な版の参照は、各object自身の版IDを名前 / hashへ変換して照合します。object間で
版IDの数値が一致することを要求しません。非defaultの`@`定義も正確な版の参照で選択でき、
異なる版の同名symbolへ代用しません。版0のsymbolはlocalに保ちます。通常のvisibility、
protected / symbolicの扱い、breadth-first scopeは維持します。関数 / data / TLSとも同じ
定義選択を通してから、各種relocationへ変換します。

未指定の旧参照についてはGNUのrelocation lookupに合わせ、版2の互換定義と、それ以降の
非hidden候補の一意性を扱います。版指定のないobjectやGLOBAL定義によるinterpositionも
扱います。GNUの規則を踏襲する範囲は、[GNU linkerのVERSION仕様](https://sourceware.org/binutils/docs/ld/VERSION.html)、
[glibcのsymbol lookup](https://github.com/bminor/glibc/blob/master/elf/dl-lookup.c)、
[version requirement検査](https://github.com/bminor/glibc/blob/master/elf/dl-version.c)で確認しました。
dlsym / dlvsym用の新旧選択APIを追加したわけではありません。

Scopeの作成時に、参照するsymbolがなくても全VERNEED nodeの提供を確認します。providerは
取得済みgraph内のDT_NEEDED名またはSONAMEで照合し、同じSONAMEがあればbreadth-first順の
最初のobjectを使います。提供元の選択にもsymbolと同じ探索順を適用します。
新たなpath readや権限付与は行いません。
versioned providerに必要なnodeがなければ出力を作る前に拒否します。weakのABI nodeは欠落を
許容しますが、強いsymbol参照まで弱めません。未versioned providerの扱いもGNUに合わせます。
単体の準備経路は外部の版要件を無視せず、scopeを必要とする入力として拒否します。

ホスト27対象・692テストが成功しました。実リンカで旧版 / 新版の同名関数・data・初期TLSを
作り、SysV / GNU / 両hash、section headerの有無、root interposition、未指定の旧参照を
検証します。LinuxホストのGNU loaderも別processで同じfixtureを実行し、結果を照合しました。
破損metadata、参照のない必須nodeの欠落、weak nodeと強いsymbolの独立、異なる版を持つ
同じSONAMEのobjectが入力順と異なる順序で現れる場合の提供元選択も確認しました。

単独runtime、capability、POSIX、workspace、Binder、managerのtarget buildが成功しました。
KVMの実アプリ起動IPCでは6ケースが成功し、effective UID 0 / 501で版付き関数 / data / TLS、
pthreadごとのTLS分離、fork後のnewlib execを実行しました。root interpositionと旧参照も成功し、
必要なABI版を持たない認可済みDSOはENOEXECで拒否され、失敗後の正常起動も成功しました。
native fixtureのbuild scriptとsourceは`services/posix/tests/`以下に置いています。

最初のホスト試験では、旧来の「版情報は未対応」というエラー期待値を更新しました。
試験用Cのpthread宣言には既存SDKが要求するfeature macroを指定しました。native fixtureの
初回buildで生じたCOPY relocationはPICで避け、今回の版付きGLOB_DAT / PLT / TLSの検証対象を
明確にしました。参照を非version化する追加試験の期待値もGNU loaderで再照合しました。
同じSONAMEの提供元を探索順に合わせた後、ホスト全対象とnativeの6ケースを再検証しました。

SDK / Rust std overlayは再生成せず、主要4fileのhashはphase264と同一です。kernel、通常packageの
Capability、UI sourceは変更していません。interpreter、COPY / IFUNC、dlopen / dlcloseと動的TLS追加、
その他の既記の残件と完全なPOSIX環境のgoalは継続中です。

診断用の4アプリ / 共有packageを除いて通常managerへ戻し、input READY、Binder ready、
デスクトップ / Dock、FilesのHome表示とウィンドウ終了を実画面で確認しました。既存のnetwork
`Failed(-6)`とUSB driver creation errno 22は残っています。検証VMは停止済みで、一時停止管理も
解除しました。今回の停止対象processは0件です。証跡は
`out/checks/mochios-symbol-versions-265-*.log`、`-artifacts.json`、`-normal.png`、`-files.png`です。

## COPY relocation（phase266）

共通userspaceローダーへ`R_X86_64_COPY`を追加しました。実行file側に確保されたdataを
通常のsymbol参照の参照先とし、initializerの検索時だけrootを除外します。提供元は既存の
breadth-first scopeと版情報で選びます。shared object側の参照も実行file側へ揃うため、
初期値のコピー後に双方が同じdataを読み書きできます。カーネルのELF処理には追加しません。

COPYの長さはsymbolのsizeです。定義済みdataの型、symbol値とr_offsetの一致、書込み可能な
LOAD内の全範囲と、他のfixupとの重複を検査します。COPYを持てるのはscopeのrootだけです。
COPY先へ結び付いた版付きimportだけ、VERNEEDを持ちながらstorageも定義する形式を許可します。
その範囲内を指すaliasも同じ検査を通します。通常の定義へ取り違える形式は拒否します。

提供元はcaptured LOADのfile bytes / zero-fillを読み、重なるfixupを共通の値encode処理で
適用したviewからコピーします。ポインタを含むstructや自身を指すdataにも、準備後の参照先を
使います。部分的に重なる8-byte fixupやTLS descriptorも必要なbyte範囲だけ反映します。
追加のELF buffer、live processのmemory read、pathの再open、resolverの実行は行いません。
DSO側のCOPYを拒否するため、この処理は再帰的なCOPYの評価を必要としません。

size差は[glibcのx86-64 COPY処理](https://github.com/bminor/glibc/blob/master/sysdeps/x86_64/dl-machine.h)
に合わせて小さい側までコピーし、残りは実行file側の元の内容を維持します。weakの未解決も
元の内容を維持します。zero-sizeは書込みや重複判定の対象にせず、COPYのaddendは無視します。
強い参照の未解決、function / TLS / absoluteやprotectedの提供元、範囲外は拒否します。
bundleと単独member出力は同じ処理を使い、size / metadataの検査後に出力します。

ホスト27対象・699テストが成功しました。実リンカのET_EXEC / PIE、SysV / GNU / 両hashと
section headerの有無を検証しました。6個のCOPYには旧版 / 新版の同名data、再配置される
ポインタを含むstruct、self pointer、8193-byteのBSS、7-byteのdataが含まれます。
GNU loaderで同じC fixtureを実行し、共有する参照先と変更後の値も照合しました。サイズ差、
部分pointer、zero-size、weak、範囲・型・権限・重複・import roleの拒否も検証しました。
固定アドレスを使うホスト試験が並列に競合する問題は、全Mappings ownerが共通の排他を保持し、
unmap後に解放する形へ修正しました。コンパイル等の独立した処理は並列のままです。

cross GCCのPIE出力は今回のC sourceにGOT参照を使い、COPYを生成しませんでした。COPYを含む
PIEの試験にはhost ccの実リンカ出力を使い、native ET_EXECはcross GCCで6個のCOPYを生成する
ことをbuild scriptで確認しています。SDKの再生成やcompiler設定の変更は行いません。
この調査で、exportのないGOT型PIEのempty GNU hashとSysVのsymbol数が異なる正当な出力を、
現ローダーが拒否する残件も確認しました。fixtureは
`out/checks/copy-relocations-266-host/root.elf`に残しています。GNU hashが空の場合の参照symbolの
境界決定は別途修正が必要です。interpreter、IFUNC、dlopen / dlclose、動的TLS追加とその他の
既記の残件も含め、完全なPOSIX環境のgoalは継続中です。

単独runtime、capability、POSIX、workspace、Binder、managerのtarget buildが成功しました。
KVMでは実アプリ起動IPCの5ケースが成功しました。effective UID 0 / 501で、版付きdata、
再配置されたpointer、self reference、共有する参照先、BSSを確認し、pthreadとfork後のnewlib
execでも再検証しました。ABI版の欠落はENOEXEC、DSOの改変はEACCESで拒否され、元のfileへ
戻した後の起動も成功しました。失敗した起動はPOSIX childを残しません。
SDK / Rust std overlayは再生成せず、SDK主要4fileのhashはphase265と同一です。kernel、
通常packageのCapability、UI sourceも変更していません。

診断用の2アプリ / 共有packageを除いて通常managerへ戻し、input READY、Binder ready、
デスクトップ / Dock、FilesのHome表示とウィンドウ終了を実画面で確認しました。VMは停止済みで、
一時停止管理も解除しました。今回の停止対象processは0件です。既存network `Failed(-6)`と
USB driver creation errno 22は残っています。証跡は
`out/checks/mochios-copy-relocations-266-*.log`、`-artifacts.json`、`-normal.png`、`-files.png`です。

## exportを持たないPIEのGNU hash（phase267）

phase266で確認したempty GNU hashの残件を修正しました。GNU hashはlookup対象のsymbolを
表し、未定義symbolを含むdynamic symbol table全体の件数を常に表すものではありません。
[binutilsの生成処理](https://github.com/RTEMS/sourceware-mirror-binutils-gdb/blob/master/bfd/elflink.c)
も、lookup対象がない場合を特別に扱います。実際のcross compiler出力には、importがあっても
empty hashのsymoffsetが1となる形式がありました。

SysVのnchainがあれば、引き続きそれを正確なsymbol数として使います。GNU hashが非emptyなら
従来どおりhashed suffixの終端との整合を確認します。GNU hashがemptyの場合は、hashから
未定義symbolの末尾を決めません。検査済みRELA / PLTが参照する最大indexとsymoffsetから
取り込むprefixを決め、全体がcaptured LOADのfile bytesに収まることを確認します。
既知のSysV件数を越えてprefixを拡大することは認めません。隣接tableやsection headerの位置を
境界の推測に使わず、empty hashの後ろをchainとして読みません。

このため、RELA / PLT / RELRの形式と全範囲をsymbol tableの解釈前に検査する順序へ変更しました。
R_NONEはsymbolを参照せず、RELRにもsymbol indexはありません。その他のRELA / PLTのindexに
必要なprefixをchecked arithmeticで取得します。lookupはGNU hash優先を維持し、取り込んだ
prefixをSysV経由でexportするfallbackは追加しません。

ホスト27対象・706テストが成功しました。GOT型PIEと同じC fixtureをGNU loaderでも実行し、
GNU-only / SysV / 両hash、section headerなしで版付きdataと関数を解決し、準備後のcodeを
実行しました。RELAだけ / PLTだけのprefix、symoffsetが1 / 全件数の形式、R_NONEの巨大index、
範囲外の参照・pointer・件数、GNU lookupとSysV lookupの優先も検証しました。
準備後のbundleは既存のnative ELF validationでも検査しました。

単独newlib runtimeとcapabilityのtarget buildも成功しました。SDK / Rust std overlayは再生成せず、
SDK主要4fileのhashはphase266と同一です。kernel、IPC / native境界、UIは変更していません。
今回は純粋なmetadata解析の修正としてホストで検証し、KVM起動とOS imageの更新は行っていません。
証跡は`out/checks/mochios-empty-gnu-hash-267-*.log`と`-artifacts.json`です。
interpreter、IFUNC、dlopen / dlclose、動的TLS追加と既記の残件を含む完全POSIX環境のgoalは継続中です。

## シンボルサイズのrelocation（phase287）

R_X86_64_SIZE32 / SIZE64をユーザー空間のELF準備へ追加しました。
値は選ばれた定義のst_sizeとaddendの和です。symbolのaddressや配置bias、TLSのmodule番号・
thread pointerからの距離は使いません。scopeの既存binding / visibility / version検索を共有し、
rootによるpreemption、絶対symbolとTLS symbolにも同じ定義選択を適用します。
未解決weakのsizeは0として扱い、未解決strongは引き続き拒否します。
計算対象の参照は[glibcのx86-64 relocation処理](https://github.com/bminor/glibc/blob/master/sysdeps/x86_64/dl-machine.h)です。

SIZE64は64-bitのmodulo加算です。SIZE32は計算した結果がu32へzero-extend可能かを検査し、
上限超過と負方向のunderflowをOverflowで返して切り詰めません。
書き換え幅を4 / 8 bytesで区別し、LOAD範囲、重複fixupの検査、BSSのmaterializeとCOPYの参照にも
既存の幅付き処理を使います。隣接する4-byte dataを8-byte writeで壊す処理はありません。
単一imageのwriteと、取得済みobject群を使うScopeの両方へ接続しています。
file取得・認可・Handleや起動情報の形式、kernelは変更していません。

実GNU assembler/linkerのSIZE32 / SIZE64を含む共有objectで、SysV / GNU / 両hashと
section headerあり・なし、rootによる定義差し替えを確認しました。
32-bit fieldの隣のcanary、正負addend、絶対symbolのsizeとvalueの違い、TLS symbolのsize、
未解決weak、u32の両端とその外側、4-byteだけ残るBSS末尾を検査しています。
準備後のbundleをhostの匿名mappingへ置き、最終permissionへ変更してfixture関数も実行しました。
GNU loaderを使う別processでも、定義済みsymbolのsize選択とrootによる差し替えが一致しました。

GNU比較には定義されたweak symbolを使っています。今回のhost GNU loaderは未解決weakの
SIZE relocationを含むfixtureでSIGSEGVとなりました。その結果を成功比較として扱っていません。
未解決weakについてはmochiOSのprepare結果がsize 0になることを別のfixtureで検査しています。
最初のfixture生成ではbare-metal compilerへ-sharedを渡してしまいET_EXECが生成されたため、
既存のcross compiler用試験と同じ-Wl,-sharedへ修正して再実行しました。

この作業に先立ちweak TLSの残件も調べました。GNU loaderの実出力では、未定義weak TLSの
GD / IE / TLSDESCはそれぞれ異なるaddressを返しました。GD / IEまで一律にNULLへ変更する
根拠にはならず、そのrelocationのUnsupportedと既存TLSDESCの扱いは変えていません。
調査用のbytesと実行結果はout/checks/weak-tls-287-researchへ保持しています。

5 targets / 84 host testsとstandalone newlib runtimeのmochiOS向けbuildで検証しました。
新規5件に既存のPIE・TLS・版付きscope・COPY・exec準備の79件を加えています。
exec準備の自己memoryコピーはsandboxでEPERMとなったため、同じ試験をsandbox外で実行して通過しています。
証跡はout/checks/mochios-elf-size-287-host-results.txt、host-native-copy.logとruntime-final.logです。
ELFの計算と書き換えの変更なのでKVMは起動せず、通常imageやSDK、Rust std overlayは再生成していません。
interpreter、IFUNC、dlopen / dlclose、動的TLS追加、weak TLSの残件と、完全POSIX環境のgoalは継続中です。

## 子imageで実行するresolverの共通処理（phase288）

`posix-client::indirect_image`へ、遅延resolverの計画をencode / captureし、一度だけ実行する
allocation-freeの共通処理を追加しました。これはIFUNC / IRELATIVEを子image内で処理するための
下層です。ELF parserやexec / CRTへの接続はまだ行っていません。既存のUnsupportedは維持しており、
この段階でIFUNC対応のアプリケーションが起動できるという意味ではありません。

計画はMIREL001 / version 1で、header 40 bytes、region 24 bytes、fixup 32 bytesです。
regionには既に認可された子imageの実行可能または書き込み可能なLOADの範囲、fixupには
子imageの書き込み先・resolver address・実行後のaddendを記録します。IRELATIVEの呼び出し先は
配置済みのB+A、実行後のaddendは0、symbolic absolute referenceでは実行後にaddendを加える
構造です。アドレスはproducerのポインタではなく子imageの仮想アドレスです。
このデータは権限を付与するものではなく、kernelへ解釈させるABIでもありません。

consumerはheaderのversion / reserved / exact length、件数と計算のoverflow、callerの容量を確認し、
全recordをcaller所有のscratchへコピーしてから検査します。regionはuser address内の非空・昇順・
非重複の範囲に限り、W+Xを表すaccessは受け付けません。resolverは実行可能region内、書き込み先の
8 bytes全体は一つの書き込み可能region内であることを要求します。unalignedな8-byte targetは許可します。
重複・部分的に重なるtargetはsortで検出し、呼び出し順は必ず元へ戻します。
検査後は元のmetadataやregion scratchを参照しないため、resolverによるmetadataの変更で
後続のresolverや書き込み先を差し替えることはできません。

実行stateはimageごとに独立し、初回だけresolverを順に呼び、その結果へaddendをmodulo加算して
8 bytesを書きます。0の結果も切り捨てません。完了後の再実行は何も行わず、処理中の再入・並行実行は
Reentrantで返します。呼び出しや書き込みが失敗した場合、既に発生した副作用は巻き戻さず、
以後Failedとして二重実行を禁止します。Rust unwindも同じ失敗状態にします。
C longjmp等がframeを放棄した場合は処理中のままとなり、後続の実行要求はReentrantとなります。
stateを別のimageや後のdlopen transactionへ使い回してはいけません。

実際のmemory read / writeとresolver呼び出しはadapterから渡すため、この共通処理自体に
生ポインタのdereference、別processのコード実行、Handle取得やCapabilityの追加はありません。
接続時にはread / writeへ既存のfaultable self-memory copyを使い、認可済みobject群の通常relocationと
TLS設定を完了してから、constructorとapplication threadの開始前に実行する必要があります。
rootのruntime contractを確認してから計画を渡し、対応しないCRTへ未解決のGOTを渡してはいけません。

接続前に残る依存も確認しています。現在のlifecycle producerはprepare済みbundleからinit / fini arrayの
関数addressをflattenします。IFUNCがそのarray要素を更新する場合は、子imageでresolver完了後に
要素を取得・検査する形式が必要です。またCOPYはproviderのrelocation後のbytesをprepare時に読むため、
遅延fixupを含むdataのコピーも実行順を決める必要があります。これらを黙って0で埋めたり、
prepareしているprocess内でresolverを呼ぶ迂回は追加していません。

新規20 testsと既存40 testsの計60 testsが成功しました。独立したwire fixtureを使い、全長未満の
truncation、件数・長さのoverflow、予約値、user addressの両端、write幅、unaligned target、
regionの順序とpermission、targetの重複、入力metadataの変更、実行順とaddend、失敗後の再実行禁止、
再入、競合するhost threadとpanicを検証しています。既存CRT試験のself-memory copyはsandboxで
EPERMになるため、その試験だけsandbox外で実行しました。
standalone newlib runtimeのmochiOS向けbuildも成功しています。
SDKとRust std overlayは再生成せず、通常OS imageやkernelも変更していません。
今回は純粋なuserspace共通処理のみの追加なのでKVMを起動せず、rust-analyzerも停止していません。
証跡は`out/checks/mochios-indirect-image-288-*`です。
IFUNC / IRELATIVEのELF・exec・CRTへの接続と実機境界の検証、interpreter、dlopen / dlclose、動的TLS追加、
その他の既記の残件を含む完全POSIX環境のgoalは継続中です。
