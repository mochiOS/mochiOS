# 起動引数ポリシーとネイティブ生成契約の分離

## 境界の整理（181）

`mnu-abi::exec`に残っていた、環境変数とHandle・値の継承を文字列で指定する
3つのprefix定数を`posix-client::launch_arguments`へ移しました。
これらは起動元のユーザー空間エンコーダだけが解釈する規則であり、カーネルABIではありません。
未使用の`SECURITY_IDENTITY_PREFIX`は削除し、互換exportやaliasは置いていません。

core.service、drivers.service、capability.service、service-manager.serviceと対応する試験の
importを更新しています。既存のposix-client依存を使い、新しい依存は追加していません。
カーネルで実際に検証する実行認可のidentity・digest・権限の契約は、この移動の対象では
ありません。カーネルのentry番号やバイナリのprocess_create形式も変更していません。

prefixの文字列値とエンコーダの処理は維持しています。ユーザー空間でargv/envpのスタックを
構成し、Handle継承はnative HandleGrant、値の継承はopaque key/valueに変換して渡します。
カーネルはsource Handleの所有、TRANSFER、権限縮小を検査し、文字列の起動規則や
POSIXのFD番号・環境変数名を解釈しません。

任意のアプリから届いた文字列が、自プロセス内のHandle番号や実行認可を選べるようにしては
いけません。capability側のアプリ起動は、受信済み・認証済みのendpointだけを継承し、
その境界を越える起動文字列を拒否する既存処理を維持しています。

## 調査したネイティブ境界

この整理ではProcessCreate、ProcessImageReplace、ProcessExitとVMの入口を確認しました。
ProcessCreateは既存のimage Handleとserialized startup image/contextを受け取り、
ProcessExitは64ビットの不透明な終了値を保持します。MemoryMap・MemoryProtectは
nativeのaccess/optionsを使い、POSIXの引数やflagsの変換はposix-client/newlib側にあります。
カーネルのプロセス別上限はthreads・IPC・mapping等の汎用機構の上限であり、POSIXの
RLIMIT_*を保持するものとしては扱いません。

これは全カーネルABI・実装の意味論監査を完了したという意味ではありません。
今回の具体的な撤去対象は、カーネル側で使用されていなかった起動文字列ポリシーの公開定義です。
POSIX互換性全体や、Vim・WebKitの実行可否を示す変更でもありません。

## 検証

既存の実エンコーダ試験で、argv/env、binary HandleGrantと権限、opaque値、重複・過長入力、
認可情報と通常引数の分離を確認します。アプリ起動の試験も、リモート入力からのHandle番号や
認証情報の偽造を引き続き拒否します。対象5ターゲット・21件は成功、失敗0件です。
`manifest_launch_retirement.rs`ではカーネル実装だけでなく共有ABIにも、これらの文字列規則の
定義が残っていないことを確認します。カーネル全体のtarget向けcargo checkも成功しました。

既存SDKとRust std overlayを消費する独立ビルドで、利用側4サービスのネイティブビルドも
成功しています。新しいsyscall、Handle転送方式、startupのバイト配置は導入していないため、
この定数配置とimportの変更の確認にVMは起動していません。通常イメージへの配置や、
この4つの新しい実行体の起動試験は行っていません。
kernel checkによるCargo.lockのpatch記録の並べ替えも、巻き戻さず変更へ含めます。

全体のホスト検証は656ターゲット・3,182件成功、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-181-final.log`、
`out/checks/mochios-launch-policy-181-focused.log`、
`out/checks/mochios-launch-policy-181-kernel-check.log`、
`out/checks/mochios-launch-policy-181-services-build.log`です。

## image 置換許可と権限取り消し（198）

プログラムの置換許可に保持した権限は、許可発行後に呼び出し側が取り消した権限を
復元してはいけません。mnu の汎用 image 置換で、保存した Capability と native authority が
現在の権限の部分集合であることを二度確認します。

最初の確認は、identity と呼び出し thread が一致する一回限りの許可を消費するときです。
権限が不足していた許可も消費し、後から権限を付与しても再利用できなくします。
identity / thread の不一致では、別の呼び出し用の許可を消費しません。
既存の信頼された broker による ProcessCreate の許可は変更していません。

image の読み込みと VM の準備中には、同じプロセスの別 thread が権限を取り消せます。
そのため、既存 thread を静止させた後、元の image / LaunchContext / Handle に変更を加える前に
再確認します。この拒否経路では準備した VM と静止 guard を破棄し、元の image を維持して
thread を再開します。カーネルに POSIX の exec、FD、サービス名、ディレクトリ配置は追加していません。

ホスト試験 `exec_authority_revocation.rs` は、本体の許可消費と再検証処理を使って
通常権限・native authority の組み合わせ 4,096 通り、一回限りの消費、別 identity / thread、
取り消し後の権限再付与、許可消費後の取り消しを確認します。
`native_exec_authority.rs` は mochiOS 専用の実行試験です。無効 Handle と有効な ELF の両方で
取り消し済み許可を拒否し、同一 ELF を供給する Provider の最初の要求で権限を取り消すことで、
許可消費後の取り消しも決定的に再現します。拒否後も元のプロセスと取得済み FileHandle が
利用できることを確認します。ホストでこの native 試験を起動した場合は SKIP と表示します。

この修正だけで fexecve、未登録のコンパイル済み ELF の起動、Vim / WebKit の実行が
完成したとは扱いません。明示的な FileHandle による実行許可と、POSIX 側の実行権限判定は
引き続き実装対象です。

198 の検証結果はホスト 3,279 件成功・失敗 0 件（674 ターゲット、ignored 8 件）、
KVM の取り消し試験 3 グループ成功です。通常構成でも Binder の READY と Dock / トップバーの
表示を確認しました。最初の全体ホスト試験では sandbox が `process_vm_readv` を拒否しましたが、
自己メモリ読み取りを許可したホスト実行で全件成功しています。試験を弱める代替処理は追加していません。
SDK / Rust std / runtime は再生成せず、既存のものを消費しています。

記録は `out/checks/mochios-exec-authority-198-host-final.log`、
`out/checks/mochios-exec-authority-198-native.log`、
`out/checks/mochios-exec-authority-198-normal.log`、
`out/checks/mochios-exec-authority-198-normal.png` です。
最終の整形後ビルドでも通常起動を再確認しました。記録は
`out/checks/mochios-exec-authority-198-final-normal.log` と
`out/checks/mochios-exec-authority-198-final-normal.png` です。検証 VM は停止済みです。

## 保持済み FileHandle の実行モード確認（199）

`Client::check_executable(file)` と `OP_CHECK_EXECUTABLE`（127）を追加しました。
要求 body は空で、READ | TRANSFER を持つオブジェクトを一つだけ IPC に添付します。
クライアントは WRITE / CONTROL / DUPLICATE 等を添付せず、受信側でも必要権限を確認します。
任意の PID / FD 番号や、確認対象のパスを指定する入口は追加していません。

POSIX service は登録済み session の effective uid / gid と補助グループを取得し、
state mutex を解放してから、添付されたオブジェクトの現在の fstat を一度取得します。
所有者であれば owner bit だけを、そうでなければ所属グループまたは other bit を判定します。
regular file 以外は拒否します。uid 0 でも regular file に実行ビットが一つもなければ拒否します。
peer 構造体の uid / gid を、この判定の根拠にはしません。

既存の filesystem の retained inode 観測を使うため、パスの再検索、open 時点の mode の流用、
共有ファイル offset の変更はありません。rename・同名パスの置換・unlink 後も、同じ open
object の現在の mode を確認します。権限と型の拒否は EACCES、inode 観測のエラーはそのまま
返します。カーネルの ABI や POSIX 知識は増やしていません。

これはチェック時点の POSIX 権限確認です。image 置換許可や、後続の chmod を禁止する
lease は発行しません。実行側では同じ FileHandle を保持して native image の認可・snapshot
経路へ渡す必要があります。今回だけで fexecve、未登録 ELF の認可、set-id 実行が
完成したとは扱いません。既存 execve のパスを変更するものでもありません。

ホスト試験では filesystem 側の実 permission 判定と owner / group / other の全 512 mode を
照合し、補助グループ・root・非 regular object・不正な request envelope・未登録 session・
不足した Handle rights・backend error の保持を確認します。クライアントの試験では
添付権限の縮小、返信 flags / body の拒否、transport failure、予期しない受信 Handle の
破棄を確認します。native 試験は `services/posix/tests/native_executable_check.rs` です。

199 のホスト全体試験は 675 ターゲット・3,284 件成功、失敗 0 件、ignored 8 件でした。
KVM の実 ext4 / IPC 試験は 4 グループ成功です。新しい posix.service を通常構成にも配置し、
Binder の READY と Dock / トップバーを目視確認しました。検証 VM は停止済みです。
SDK・Rust std・runtime は再生成していません。

記録は `out/checks/mochios-executable-check-199-host-all.log`、
`out/checks/mochios-executable-check-199-native.log`、
`out/checks/mochios-executable-check-199-normal.log`、
`out/checks/mochios-executable-check-199-normal.png` です。

## オブジェクトのバイト列に対する image 認可（200）

`AUTHORIZE_EXEC_OBJECT_OPCODE` は版付き request、呼び出し側が計算した SHA-256、
最大 255 バイトの不透明な identity key、明示的に添付した READ object 一つを受け取ります。
PID / FD / 起動対象パス / 新しいアプリ識別情報 / 追加 Capability の指定欄はありません。
requester はカーネルが保証した IPC sender から解決します。

capability.service は user delivery、添付数、reserved、READ のみへの権限縮小と
requester の process.spawn を確認します。受信 object への I/O は一切行いません。
Provider が応答しなくても、サービスの直列ループを待たせないためです。
最初の実装では broker で digest を計算していましたが、この停止要因を見つけたため、
公開 entry point の接続前に呼び出し側の計算へ変更しました。
成功・失敗のどちらでも受信した参照を閉じてから返信します。

この digest は署名や第三者の承認ではありません。呼び出し側は自分の security domain で
実行するバイト列を指定するだけです。native loader は実際に渡された READ object の
全バイトと digest の一致を、image を確定する前に強制します。虚偽の digest は別の identity
や新しい権限を取得する根拠にならず、バイト列不一致なら置換が拒否されます。
署名済み package の認可と違い、package index や対象パスの再 open は行いません。

既存の汎用 AuthorizeExec に `image-replace-inherit` モードを追加しました。
入力として受理するのは kind、unprivileged class、image digest の三項目のみです。
identity の上書きや空でない grants list、privileged class は拒否します。
信頼された broker の CapabilitiesManage は従来どおり必要です。
カーネルは requester の application identity と現在の通常 Capability 全体を、
同じ process-table borrow 内で fallible に複製して一回限りの置換許可に保存します。
ユーザー空間で既知の Capability 名だけを列挙する方法は使いません。
scoped / dynamic な通常 grants も同じ集合から取得します。native authorities は引き継ぎません。

これは呼び出し元の既存 security domain 内でコードを置き換える認可であり、別の署名済み
package の identity や grant を取得する処理ではありません。後続の image 置換では従来どおり
実バイト列の digest、実行クラス、thread、現在の権限の部分集合を検証します。
認可後のファイル変更や権限取り消しは、その許可で新しい image を確定する理由にはなりません。
カーネルに POSIX の uid / gid / FD / directory / service 名は追加していません。

今回の native 試験は低レイヤの image 置換を直接呼びます。libc の fexecve entry point、
exec の FD / signal / record-lock 引き継ぎへの接続、set-id 実行の意味論は未完了です。
FileHandle を起動する POSIX wrapper は、199 の実行 mode 判定とこの image 認可の両方を使い、
同じ object を保持したまま既存の exec startup / LaunchContext 処理へ接続する必要があります。

最終版のホスト全体試験は 677 ターゲット・3,289 件成功、失敗 0 件、ignored 8 件です。
KVM では未登録・unlink 済み ELF への置換、digest 不一致、認可後の権限取り消し、
ambient read 権限を失った後の保持済み FileHandle からの置換、応答しない Provider の隔離の
5 グループが成功しました。新しい image でも process/thread と application identity、
現在の通常 grants が保持されることを確認しています。

記録は `out/checks/mochios-object-exec-200-host-final.log` と
`out/checks/mochios-object-exec-200-nonblocking-native.log` です。
SDK・Rust std・runtime は再生成していません。kernel build で更新された Cargo.lock も
巻き戻さず、対応する変更と一緒にコミットしています。
最終の通常構成でも Binder の READY と Dock / トップバーを目視確認し、VM を停止しました。
記録は `out/checks/mochios-object-exec-200-final-normal.log` と
`out/checks/mochios-object-exec-200-final-normal.png` です。

## libc への接続（201）

FileHandle からの `fexecve`、通常 execve との共通 startup、検証結果と残っている生成認可の
制約は [FileHandle からの POSIX exec](posix-executable-handles.md) に記録しています。

## 検証済みlauncherへの双方向PTYの委譲（310）

Terminalのstdioアダプタは、明示されたローカルFDのアクセスモードを確認し、O_RDWRの
descriptorを読み専用／書き専用へ変更せずに共通のapplication launcherへ渡します。
stdinにはREAD、stdout / stderrにはWRITEが必要です。元のモードがこれを満たさなければ
拒否し、pipe等の片方向descriptorに逆方向の権限は足しません。
既存のmochi_fd_exportが参照を保持し、実際のsource Handleの権限上限を強制します。
任意PID / FDの取得や、添付されていない参照の委譲は行いません。

通常のPTY slaveは3本ともO_RDWRです。stdinを読み専用にすると、login_ttyがそのFDを
stdout / stderrへdup2した際にも読み専用になり、出力できません。この修正ではその
アクセスモードを保ち、既存のCONTROL・TRANSFER等の検査と、brokerによるCLOEXEC解除を維持します。
要求形式、mnuのAPI、capability.serviceの権限判定は変更していません。

ホスト5対象17件が合格しました。KVMでは実Terminalのアダプタ・capability.service・
測定済みmshを通してPTY slaveを3本のstdioへ委譲しました。親のslave FDを閉じた後も、
子のlogin_tty、session leaderとforeground group、全stdioのO_RDWRと端末判定、
UTF-8のcanonical行入力、stdoutとstderr、Ctrl+DによるEOF後の再入力、foregroundへの
SIGINT、子の正常終了とwaitを確認しました。新規の対話承認による権限追加・永続化は行っていません。
診断はservices/posix/tests/support/以下に置き、通常buildでは含めません。

この段階では通常Terminalの入力はまだpipe上のbinary InputEventです。
通常経路へのPTY接続、UTF-8 / 端末キー列の生成とbackpressure、mshが子の実行中にstdinを
消費しない処理、foreground jobの停止・再開と端末属性の復元、window sizeの伝達が残っています。
Vimや通常clangの一連の開発操作、完全なPOSIX互換性の完成を示す試験ではありません。
保護された承認入力も通常PTYの文字列とは分離し、stdinのyで新規grantを許可しません。

検証には既存のSDKリンク入力の固定コピーと既存native runtimeを使用し、SDKとRust overlayを
再生成していません。証跡はout/checks/mochios-pty-launch-310-*です。
診断を除いた通常版でもdesktop・Dock・Terminal・mshと文字入力・echo・プロンプト復帰を
確認しました。通常版2実行体のstage / image hashと6件のchecksumを照合し、全検証VMを停止して
解析処理の復帰も完了しました。変更とlock更新は署名付きコミットへ保存しています。
