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

## 通常TerminalのPTY入力とmshのjob制御（311）

通常Terminalはopenptyのmasterを保持し、slaveだけを既存の検証済みapplication launcherで
mshのstdioへ明示的に委譲します。mshはlogin_ttyでcontrolling terminalを取得します。
この処理のためにカーネルへ端末・shell・サービス名の知識は追加していません。

binary InputEventをstdinへ書く経路を通常Terminalから外し、UTF-8、制御文字、端末キー列を
送ります。入力は64 KiBを上限とするキューで順序を保ち、部分書き込みとEAGAINでは
未送信の末尾を残します。各tickの書き込み・出力読み取りにも上限があります。
実際の表示領域の行列数を起動時と変更時にPTYへ渡します。

mshは子の実行中にstdinを読みません。既存のcommand builder・PATH検索・環境継承を使い、
子のprocess groupとforeground terminalをfork/execの間で設定します。
waitpidで終了・停止・再開を追跡し、停止したjobをjobs / fg / bgで扱います。
末尾の&も解釈します。shellに戻る際にはforeground group、blocking status、canonicalな
端末属性を復元し、fgでは停止したjob自身の端末属性を復元してからSIGCONTを送ります。
端末切断時の書き込み失敗もio::Resultで処理し、保持中のjobの後始末へ戻します。

実際のSDKのCヘッダーとの照合とKVM試験で、共通Rust libcに三つの不整合を発見しました。
termiosはc_lineなし・速度フィールド二つを含む56 bytesでしたが、汎用newlibの52 bytesの
構造体を公開していました。SDKと同じ構造体、端末定数、openpty / window-size宣言へ揃えます。
WIFCONTINUEDが常にtrueだったため、終了・停止との区別ができる実際のwait status解釈へ
直しました。さらにsignal番号がLinux ABIのままで、SIGTSTPを無視する操作が実際には
newlibのSIGCHLDを無視していました。これがKVMで発見した子の自動reapとECHILDの原因です。
SIGCHLD=20、SIGSTOP=17、SIGTSTP=18、SIGCONT=19等を実際のnewlib ABIへ修正しました。
SDK自体の再生成・書き換えは行っていません。

ホストではSDKのCヘッダーとの構造体・定数・wait解釈の照合、入力の変換とキュー、
job状態と背景構文、実PTY上の入力・pipeline・停止/fg・SIGINTを検証します。
ホストのshell helperは意図的にSIGCHLDを無視してから起動し、shellの初期化が
起動元の自動reap設定を引き継がないことも確認します。

通常rendererはscrollback表示でTERM=dumbです。完全なVT端末、全POSIX shell文法、Vimの
実用動作を完成扱いにはしません。旧stdio / endpointの明示モードは移行中の利用側向けに
残っています。通常PTY経路の失敗を、その旧経路へ暗黙にフォールバックしません。
新しいCapabilityの付与には引き続き保護された提示・入力経路が必要です。
stdinや任意のkey packet、古いscalarな承認返信を新規grantの根拠にはしません。
CLIとGUIの保護された同意frontend、および通常のclangからld.lld・msign・署名済み
standalone ELFの実行へつなぐ作業は未完了です。

311のホスト10対象27件と通常Terminal・msh・入力サービスのnative buildは成功しました。
KVMでは通常のDockから開いたTerminalで、子への行入力・正常終了・promptへの復帰、
raw modeでの上矢印のESC[A受信、raw / nonblockingを残す子からの端末復元、
SIGTSTP停止とfgでのjob端末属性復元、Ctrl+Cによるforeground子の終了を確認しました。
背景readのSIGTTIN停止、bgで再開後の再停止、fg後の入力と終了、jobsが空になること、
shellの通常exitをTerminalがwaitしてProcess completedを表示することも確認しています。
上矢印の最初の実機試験は未達でした。input.serviceのscan code 0x48 extendedの対応漏れを
一行追加し、入力サービスをビルド・配置した後、同じ物理キーでPASSを確認しました。

pipelineは全面合格ではありません。入力待ちするcopy | copyではデータ転送、Ctrl+DのEOF、
両段の終了とprompt復帰が成功しました。一方、直ちに終了するechoを前段にすると、
次のchildの生成でEAGAINが発生しました。前段終了との競合が疑われますが、原因箇所を
特定したものではありません。失敗を隠す再試行や別の実装へのfallbackは追加していません。
この問題、catの引数なしstdin対応、option付き.appのPTY側dispatch、完全なVT表示、
保護された承認frontendと署名driverは引き続き未完了です。

既存SDK / runtimeの固定済みlink入力を消費し、SDKやRust overlayを再生成していません。
SDKの観測hashはphase310から変化していますが、この作業で書き換え・巻き戻しはしていません。
検証したVMは全て停止し、ホスト解析の再開処理も完了しています。一時native fixtureは
通常配布stageから除去しました。診断用に差し替えたPOSIXサービスも元の実行体へ戻しています。
記録はout/checks/mochios-pty-shell-311-artifacts.json、同接頭辞のhost.log、
keys-vm.logとkeys-*.pngです。通常イメージへの最終反映は同normal-image.logに記録します。

最終の通常イメージでもTerminal・msh・入力サービスのバイト列がnative buildとstageに
一致し、元のPOSIXサービスが維持されていることをimage内から読み戻して確認しました。
一時fixtureはimage内にも存在せず、配布物6件のSHA-256チェックも成功しています。

## fork/execの内部応答待ちと通知（312）

311で残っていた短命な前段を持つpipelineのEAGAINを、独立runtimeの診断buildで調査しました。
通常Terminalから繰り返すと、ProcessCreate・登録・resumeの後にあるchild-ready待機が
errno 11を返し、fork全体が失敗していました。mnuのblocking stream readは、データ待ち中に
native upcallが配送可能になるとNotReadyでユーザー空間へ戻ります。この通知をCの
資源不足EAGAINに変換したままforkの失敗にしていたのが原因です。

newlib-runtimeのprivateなblocking startup streamに読み取り処理を置き、forkの明示的な
初期化ACKとposix_spawnのexec-statusの両方で利用します。typed native NotReadyの場合に
ユーザー空間のsignal配送を処理し、同じstreamへのblocking readに戻ります。
新しいchildを作り直す再試行や、固定回数・sleepによる回避は行いません。
通常のFD read、クライアントがnonblockingにしたdescriptor、カーネルの意味論は変更しません。

部分ACK / exec-error packetの読み取り位置は保持します。初期化ACKが完結する前のEOFは
失敗、exec-status streamの空のEOFは既存どおり成功です。通知処理自体の失敗、nativeの
メモリ不足・権限・selectorのエラー、legacyのEAGAINはそのまま伝播し、読み直しません。
不正な読み取り長はEIOです。元のoperation ownerがstreamとchildを持ち続けるため、
待機中にsignal callbackが呼ばれても、途中の参照や登録を無条件に手放しません。

ホスト5対象27件が成功しました。通知を部分応答の前後へ挿入する実コード試験、execの
成功・子のerrno・切れた応答、本来のエラーを隠さないこと、子所有権の移譲とcallbackからの
abandonmentを確認しています。単独プロセスでsignal下のfork / spawnを各12回行う
試験プログラムもホストで通りました。SDKとRust overlayの再生成は行わず、独立runtimeと
既存の固定済みSDK link入力で実機用バイナリを構成しています。

同じ通常Terminal上の比較試験では、修正前runtimeの12回のforkのうち2回がerrno 11で
失敗しました。修正後は通知下のforkとspawnが各12回とも成功し、正常なwaitとprompt復帰を
確認しました。通常mshのecho | copyも3回とも実データとEOFを渡し、jobsは空へ戻りました。
連続入力では端末echoと先行コマンドのstdoutが表示上で交差しますが、b312 / c312は
それぞれ出力されました。完全なVT表示やshellの再描画を検証したものではありません。

修正後fixtureを裸の名前で呼んだ初回だけENOENTを観測しました。絶対パスでのfork / spawnは
成功していますが、この探索失敗の原因は未特定です。今回の通知修正からPATH探索全般の
完成は主張しません。記録はout/checks/mochios-pipeline-spawn-312-*のhost.log、
trace-vm.log、before-result.png、after-explicit-result.png、after-spawn-result.png、
pipeline-a.png、pipeline-bc-result.pngです。

後続の裸コマンド名での--exitは成功し、初回ENOENTを繰り返し再現したものではありません。
shellの通常exitをTerminalがwaitし、Process completedを表示することも確認しました。
検証VMは停止し、解析処理の復帰処理も完了しています。通常イメージから試験用実行体3件を
除去し、Terminal・修正済みmsh・POSIXサービス・入力サービスをimageから読み戻してstageの
記録hashとの一致を確認しました。配布物6件のchecksumも成功しました。
修正と回帰試験は別々の署名付きコミットに保存し、既存のuser/Cargo.lock更新も保存しました。
通常clangの自動署名・共通実行認可、保護されたCLI / GUI承認frontendと完全なPOSIX互換性は
引き続き未完了です。

## 実行許可の権限世代と最終commitの排他（335）

PendingExecSecurityを発行先processの権限世代へ結び付けます。世代はnative側で設定し、
要求側の値を採用しません。通常Capabilityとnative authorityの可変取得・設定で世代を
更新し、保管中の許可を破棄します。消費済みの許可も、image置換と継承image作成の
後段で現在の世代と照合します。撤回後に同じ集合へ戻しても古い許可は復活しません。
世代の加算は飽和し、上限では新しい発行をLimitExceededで拒否します。

最終確認後の外部processからの更新も直列化するため、ProcessObject内に割当不要の
authority gateを置きます。CapDrop・CapTransfer・CapRestrictはprocess tableを借りる前に
更新guardを取得し、image置換は兄弟threadの静止後、旧imageへ触る前にcommit guardを
取得します。準備中の撤回は許可し、最終確認から確定までは外部更新を凍結します。
競合は待ち続けずResourceUnavailable、消えたprocessはNotReadyとして返します。
失敗時のguard解放、旧VM・権限の保持と、通常の部分集合検査は維持します。
未使用のglobalな権限設定関数は削除し、kernelへPOSIX・承認画面・サービス名を追加しません。

これは既存権限の縮小としての実行認可を強化する変更です。CLI / GUIの保護された承認と、
承認済みの新規grantを親へ経由させず対象imageへ適用する接続は引き続き残ります。

関連12 targetsのhost試験17件とkernelのrelease buildが成功しました。実際のauthority gateを
使い、複数更新と最終commitの排他、競合時のnative error、失敗した取得で既存guardを
解除しないこと、rollback、割当なしの取得と100回のthread間競合を確認しています。
権限世代の試験は撤回・同一集合への復元、消費済み許可、世代の飽和も扱います。

同じ測定済みELFと明示した権限を用いたKVM比較では、修正前kernelは保管中・消費後の
両方で撤回後に古い許可を再利用できました。修正後は通常の許可による実行が成功し、
消費前の撤回とimage I/O中の撤回は、元の権限集合へ戻しても拒否されました。
拒否後のprocess identity保持、再消費の拒否とmsh prompt復帰も確認しています。
記録はout/checks/mochios-exec-authority-generation-335-のhost-verified.log、
gate-status.log、before-result.png、result.pngとartifacts.jsonです。

同じ修正後kernelと通常PATHでclang回帰試験も成功しました。標準target、compile / link /
自動署名、生成C自身による元ELF identityと全canonical Capabilityの0、正確なstdoutと終了値37、
compile / link失敗後の旧ELF保持、msignと通常execによる改変拒否、cleanupとprompt復帰を
確認しています。compile / link / sign / verifyは130,620ms、全試験253,416msでした。
負荷条件を揃えた性能比較ではありません。画面は同接頭辞のclang-result.pngです。

検証VMはPIDと開始時刻を照合して停止し、記録した解析処理の復帰も終えました。
診断用4ファイルだけを撤去し、修正済みkernelを保持して通常イメージを再生成しました。
475ファイルと4つのcommand linkの読み戻し、probeの不在、配布成果物6件のchecksumが
成功しています。SDK入力は不変です。normal-image.logとreadback.jsonへ記録しています。

## 管理権限の撤回と付与の直列化（336）

CapTransferの管理権限による付与は、要求形式を選ぶための事前確認とは別に、
実際に付与するprocess-table lock内でsourceの現在のCapabilitiesManageを再確認します。
異なるprocessへの付与は既存のwith_process_pair_mutを使い、sourceの権限確認、
destinationの実行key確認とfallibleな挿入を一つのlockで処理します。
自分自身への付与も同じ可変borrow内で再確認します。335のauthority guardも維持します。

実際のtransfer本体とprocess pair取得を使うhost試験で、事前確認後・可変borrow直前に
管理権限を撤回すると、修正前は成功を返して付与していました。修正後は拒否し、
destinationへ権限を残しません。self grantと両方のprocess table順序を確認し、
通常の管理付与、挿入失敗時の所有権保持、通常transferと実行許可失効の回帰も通しました。
関連4 targets、5件が成功しています。記録はout/checks/mochios-capability-grant-revalidation-336-
のbefore.logとhost.logです。この変更は承認応答を実行許可へ接続する実装ではありません。

kernelのrelease buildと通常イメージの再生成も成功しました。保持した実行体・開発ツール・
SDKは変更せず、475ファイルと4リンクの読み戻し、診断ファイルの不在と成果物6件のchecksumを
確認しています。SDKとRust overlayは再生成していません。ABIやIPCの形式は変更しておらず、
この変更でKVMは起動していません。hostの撤回注入をnative試験としては数えません。
同接頭辞のkernel-build.log、normal-image.log、readback.jsonとartifacts.jsonへ記録しました。
