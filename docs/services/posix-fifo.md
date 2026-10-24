# 名前付きFIFO

最新の実装範囲は末尾の191です。183〜190は各段階の記録です。

## 共有状態（183）

`services/posix/src/fifo.rs`に、1つの保持されたFIFO inodeに対応するユーザー空間の
`Channel`を追加しました。現時点では状態処理のみで、実行中のサービスの要求処理へは
接続していません。`mkfifo()`、FIFOのpathname open、アプリからの利用は未実装です。

[POSIX open](https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html)に従い、
読み取りのnonblocking openは相手なしで成立し、書き込みのnonblocking openは
読み手がいなければ失敗します。blocking openは反対側のopenとの待ち合わせになります。
状態処理は待機結果を返すだけで、サービスの処理ループを停止しません。
一度成立したopenは相手が閉じても待機状態へ戻しません。

最後の書き手が閉じた後は、残ったデータを読み出してからEOFになります。
読み手が残っている間に別の書き手を開けば再接続できます。読み手がいない書き込みは
型付きのNoReadersを返します。errno変換とSIGPIPE生成は将来の接続部分が担当します。
すべてのopen参照がなくなったときにだけキューを破棄します。

[POSIX write](https://pubs.opengroup.org/onlinepubs/9699919799/functions/write.html)の
不可分性に対応するため、PIPE_BUF以下の書き込みは全量を入れるか、状態を変更せずに
WouldBlockを返します。それより大きい書き込みは空き容量まで進みます。
PIPE_BUF=512はposix-protocolの共通定数に集約し、既存の匿名pipe用clientも参照します。
カーネルはこの定数を解釈しません。

バッファは64KiB、open descriptionは1チャネルにつき1024件を上限とします。
メモリ確保は状態変更より前に行い、失敗時に相手の待機状態や既存データを変えません。
description IDはチャネル内で再利用せず、閉じたIDを使う操作を拒否します。
IDはowner内部の識別子であり、Handleの代わりとなる権限ではありません。

## 接続時に必要な処理

カーネルの既存streamは、1組の端点と、容量・切断・不可分転送要求を提供する汎用機構です。
既存の組へ独立したopenを追加する機構ではありません。FIFOの相手が消えても保持用の
writer参照を残す方法ではEOFを壊すため、その方法には依存しません。
FIFO名やinode、openの待ち合わせをカーネルへ持ち込むこともしません。

183終了時点では、次の接続が未実装でした。

- filesystem.serviceでのFIFO inode作成・識別と、権限・umaskを伴うatomicなpathname操作。
- 保持されたinodeの権限に基づくチャネル取得。自己申告のパスやdev/inode番号だけで
  既存チャネルへ接続するAPIにしません。unlink・再作成による別inodeも区別します。
- 既存Provider Handleによるデータ操作と、参照の最終解放通知。dup・fork・Handle転送は
  同じdescriptionを共有し、個々のFD closeで読み手・書き手を減らしてはいけません。
- native参照確保と状態登録、応答公開の失敗、待機中のキャンセルの接続。
  チャネルへ登録する前にownerが参照を確保し、失敗時は確実に解放します。
- open完了とI/OのEvent通知、nonblocking設定共有、poll/selectのHUP判定、SIGPIPE、
  FDの種類、exec継承、inode metadataとunlink後の寿命。

`Observation`は現在の参照数・データ量・open完了状態だけを返します。
これをPOSIX poll結果として直接公開しません。たとえば相手の書き手が一度も現れていない
読み取りFIFOと、書き手が閉じた後のHUPを区別する処理は接続段階で必要です。
O_RDWRのFIFOも現段階のAPIには含めません。

## 検証

ホスト試験はFIFOの状態10件、メモリ確保失敗の注入2件と、既存ストリーム10件の
計22件が成功しました。待ち合わせの両順序、複数の待機者、キャンセル、EOFと再接続、
最後のclose、古いID、独立チャネル、容量境界、不可分書き込み、方向違いを扱います。
allocation試験では実際のallocatorを失敗させ、open待機者とキュー内容の維持を確認します。

共有定数への変更で、ソース断片を単独コンパイルする既存SIGPIPE fixtureが最初に失敗し、
実際の共通定数をfixtureへ渡して修正しました。途中の失敗と最終成功を別ログに残しています。

ログは`out/checks/mochios-fifo-183-focused-before.log`、
`out/checks/mochios-fifo-183-focused-final.log`、
`out/checks/mochios-fifo-183-allocation.log`です。
posix.serviceのmochiOS向けビルドも成功し、
`out/checks/mochios-fifo-183-native-build.log`へ保存しました。

今回の変更はホストで検証可能な状態処理と共通定数だけです。
IPC・カーネルABI・SDK・Rust stdを変更せず、VM起動も通常イメージへの配置も行っていません。
FIFOの実アプリ統合試験や完全なPOSIX互換性を確認したものではありません。

## ext4 namespaceと保持参照（184）

filesystem.serviceに`create_fifo_at`と`open_fifo_at_retained`を追加しました。
これはサービス内部の操作で、IPC opcodeやlibcの`mkfifo`公開はまだ追加していません。
前節のチャネル状態との接続、native Provider Handleの応答公開、待機通知も未実装です。

作成は既存socket inodeの排他的な作成処理を共通化して使用します。探索、親のsearch/write
権限確認、親setgidからのgroup継承、inode作成とlinkを提供元の直列化された操作内で行います。
事前statと後続createに分割しません。既存の最終symlinkも置換せずEEXISTとし、過長入力と
未存在宛先の末尾`/`も拒否します。FIFO作成ではopen descriptionを消費しません。

[POSIX mkfifo](https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/mkfifo.html)
が要求するmode・umaskの関係に対して、filesystem側には認証済みcredentialsと適用済みmodeを
渡す既存の境界を維持します。umaskの取得と適用は今後追加するPOSIX要求処理の責務です。
今回の内部APIがumaskを取得するわけではありません。

`open_fifo_at_retained`は指定した読み取りまたは書き込み方向の権限を確認して、解決済みの
inodeそのものを保持します。POSIX openの待ち合わせは完了させず、ディスクのFileも作りません。
参照は既存のopen tableとunlink保持処理を使うため、unlink後に同名FIFOを作り直しても
元の参照は元のinodeを指し続けます。別々の保持参照も同じinodeの更新されたmetadataを読みます。
現段階では内部open IDを返すだけで、これをクライアントの権限として公開しません。

通常のopen_atからFIFOをディスクFileとして開く経路は、O_TRUNCや参照公開より前にENOTSUPで
拒否します。直接のtruncateも変更前に拒否します。metadata参照からのread/seekも拒否し、
pager backingは返しません。これはチャネルへのデータ経路がつながるまでの明示的な未対応です。
FIFOを空の通常ファイルとして動作させる代用実装にはしません。

filesystem-protocolのNodeStatusにNODE_TYPE_FIFOを追加し、POSIX metadata変換でS_IFIFOを
復元します。NodeStatus.mode自体は従来どおりpermission bitsです。カーネルのABIや処理に
FIFOの概念を追加していません。

実ext4を使う試験を8件追加しました。作成後にファイルシステムを再読込した場合の型とmode、
ゼロのディスクデータ、通常open/truncate拒否、既存エントリの不変、親権限・group継承、
入力flags/anchorsの拒否、unlink後の保持と同名置換、NOFOLLOW、参照IDの未消費、別volumeの
identity、root制限、非UTF-8名、読み取り専用volumeを確認します。
既存socket作成を含むマウントとファイル操作、metadata変換、FIFO状態・確保失敗の試験を合わせ、
5ターゲット・193件が成功しました。全POSIX試験や実OSからのFIFO利用の確認ではありません。

最初の試験ではNodeStatus.modeに型bitを含める誤った期待と、chown fixtureの誤った要求形式で
3件が失敗しました。既存の表現に合わせて試験を修正し、成功ログとは区別しています。
ログは`out/checks/mochios-fifo-namespace-184-focused-before.log`、
`out/checks/mochios-fifo-namespace-184-focused-second.log`、
`out/checks/mochios-fifo-namespace-184-related.log`です。

filesystem.serviceとposix.serviceのmochiOS向けビルドも成功しました。
`out/checks/mochios-fifo-namespace-184-native-build.log`に記録しています。
実IPCの変更はまだないためVMを起動せず、通常イメージのサービスも変更していません。
SDK・Rust stdの再生成も行っていません。

## mkfifo / mkfifoatの作成経路（185）

newlib runtimeに`mkfifo`・`mkfifoat`を公開し、posix-clientの`mkfifo_at`、
POSIXのOP_MKFIFO_AT=123、filesystemのOP_CREATE_FIFO_AT=52へ接続しました。
既存のpathname保持、FD lease、root/cwd、明示的なdirectory Handle転送を使用します。
絶対パスはdirfdを使用せず、相対パスは保持したディレクトリを基準にします。
mkfifoのためのカーネルABIやサービス名検索は追加していません。

mkdirと共通だった作成引数の型をPreparedNodeCreationへ揃え、同じ解釈処理を使用します。
modeは保存済みsessionのumaskで変更し、uid/gid/groupsもsessionから取得します。
RPC内の自己申告identityやFD番号には依存しません。要求処理ではroot/cwdを保持し、
POSIX stateのlockを外したままfilesystemへ1回の作成操作を依頼します。
既存のimage切り替え時の再検証、拒否時の参照解放も適用します。

filesystemでは明示的に委譲されたcontrol endpointからの配信だけを許可し、
認証済みcredentialsとordered root/base添付を既存のdecoderで確認します。
184のinode作成処理を呼び、成功時はHandleもpayloadも返しません。
POSIXの新opcodeもassigned request認識に追加し、未知opcodeのENOSYSは維持します。

ホストでは既存のsingle-path mutation試験群にFIFOを加え、保存済みcredentials・umask、
root/cwdと添付directory、実行中のPOSIX lock未保持、拒否時の解放と状態変更、
malformed要求の事前拒否を確認しました。client試験では非UTF-8名、3種類のbase、
remote error、予期しない応答Handleの解放を検証します。runtimeの実pathname保持処理にも
FIFOを加え、caller memoryの変化とRPCの成功・拒否・FD置換時の参照解放を確認しています。

全体658ターゲット・3,209件のホスト試験が成功し、失敗0件でした。
`out/checks/mochios-posix-host-audit-185.log`へ保存しています。
最初の試験コマンドに存在しないターゲット名を指定した失敗は実装の試験結果に含めず、
`out/checks/mochios-mkfifo-185-focused-command-error.log`として別に保存しています。

runtime archive・filesystem.service・posix.serviceをネイティブビルドし、SDK・Rust stdを
再生成せずCの`native_fifo_creation.c`をリンクしました。nmでもmkfifo/mkfifoatが実際の
runtimeからELFへ入ったことを確認しています。KVM上の実IPC試験は次の6群が成功しました。

- FIFOの作成、S_IFIFO、umask、inode identity、成功時のerrno維持。
- rename後のdirectory FDを基準とする作成、非UTF-8名、絶対パスでのdirfd無視。
- 既存symlink、未存在末尾区切り、無効ポインタ、NULL、過長パスの失敗と未作成。
- setuid/setgid後の親権限拒否、作成者のuid、親setgidのgroup継承。
- chroot後の絶対パスと`..`のroot内への制限。
- 未対応のFIFOデータopenをディスクI/Oへ流さず拒否し、metadataとunlinkは動作。

ログは`out/checks/mochios-mkfifo-185-native.log`です。診断ではcompositorをC試験へ置換するため、
試験完了後のsecure-uiのCompositorNotFoundは通常起動の証明として扱いません。
データ用Provider Handle、チャネルへの接続、blocking openの待ち合わせ、poll、SIGPIPE等は
未実装です。FIFOを介したアプリ間データ転送や完全なPOSIX互換性はまだ確認していません。

診断後はcompositorとservice-managerを元のバイナリへ正確に戻し、新しいfilesystem・POSIX
サービスを配置した通常イメージを作成しました。別のKVM起動でBinder.app readyと実際の
Dock・上部バーを確認しています。`out/checks/mochios-mkfifo-185-normal.log`と
`out/checks/mochios-mkfifo-185-normal.png`に保存し、確認後のVMは停止済みです。
すべてのアプリを新runtimeで再リンクしたわけではありません。今回の新libc機能を実行した
証拠は新runtimeをリンクしたC試験で、通常画面は既存アプリとの起動回帰の確認です。
配置済みサービス・runtime・C ELFのハッシュは
`out/checks/mochios-mkfifo-185-artifacts.json`に記録しています。

## Native Providerのデータ操作（186）

`fifo_provider.rs`を追加し、ownerが解決済みのdescriptionとnative object IDのbindingに対して、
既存のProvider要求を処理できるようにしました。カーネルのIPC_DELIVERY_NATIVE_OBJECT配信を
要求し、追加attachment・異なるobject ID・偽装した通常endpoint配信は状態変更前に拒否します。
bindingはowner内部のrouting情報であり、inodeやFD番号から権限を取得するAPIではありません。

READ/WRITEは183のキューを操作し、不可分書き込み・容量不足・EOF・EPIPEを適用します。
出力容量を検査してからデータを消費するため、小さすぎる応答bufferで読んだデータを失いません。
位置指定操作はESPIPEとし、OP_MEMORY_OBJECTは明示的に0を返します。
ディスク用pagerやbyte offsetをFIFOへ流用しません。

OP_RELEASEは認証済み・attachmentなし・正しいbindingの場合だけ処理し、応答を返さずに
descriptionを1回だけ閉じます。ネイティブのProvider参照寿命を再利用するので、dupやforkで
同じdescriptionを参照するたびにFIFO側で読み手・書き手を増減する必要はありません。
カーネルのProviderには複製間で共有されるI/O optionsも既にあるため、その状態も重複実装しません。

descriptionにはwriterを観測したかを保持し、データの有無、EOF、hangup、書き手側の相手消失を
分けて取得します。native signalsはEOF/EPIPEも操作完了として通知し、POSIX pollへの変換とは
区別します。最初のnonblocking readerに相手がいない場合のEOFを、直ちにhangupとはしません。
この区別の参照は[Linux v6.16のFIFO poll実装](https://github.com/torvalds/linux/blob/v6.16/fs/pipe.c)です。
再接続後は、そのdescriptionの現在の相手とキュー状態に合わせてsignalsを更新できます。

関連ホスト5ターゲット・42件が成功しました。FIFO状態・確保失敗・新Providerの5件、既存socket
Providerとnative streamの参照処理を含みます。最初はSeek試験frameのbasisに既存ABIで無効な0を
指定して1件が失敗し、HANDLE_SEEK_STARTへ修正しました。失敗と最終成功は
`out/checks/mochios-fifo-provider-186-host-before.log`と
`out/checks/mochios-fifo-provider-186-host-final.log`に分けて保存しています。
全POSIXホスト試験を再実行したものではありません。

`native_fifo_provider.rs`は独立したowner threadでnative参照を確保してからチャネルへ登録し、
実際のHandleRead/HandleWriteとOP_RELEASEでこのadapterを動かします。KVMでは次の4群が成功しました。

- native Handle経由の読み書き、512バイトの転送と空キューのEAGAIN。
- dup間のI/O options共有、元のHandleだけを閉じてもwriterが生存すること。
- 最後のwriter参照の解放がownerへ届き、readerがEOFを返すこと。
- 最後のreaderの解放とowner threadの終了。途中ではREAD-only HandleのWRITEも拒否。

ログは`out/checks/mochios-fifo-provider-186-native.log`です。この試験は同一プロセス内の別threadを
Provider ownerとし、ネイティブ要求のIPCと参照寿命を確認します。別プロセス間のFIFO openやforkによる
FIFO description共有を試したものではありません。診断でcompositorを置換したことによる試験後の
CompositorNotFoundは通常起動の失敗として扱いません。

このadapterはまだ実サービスのprovider loopとpathname openへ接続していません。次に必要なのは、
filesystemが権限確認して保持したinodeのHandleを受け取る境界と、inodeごとのチャネル登録です。
自己申告のdev/inode番号だけで既存チャネルへ接続してはいけません。データHandle公開前の参照確保、
失敗時のrollback、blocking openの通知とキャンセルも接続が必要です。
POSIX poll/select、metadata参照の返却、FDの種別・exec継承、SIGPIPE生成、容量に応じた書き込み待機も
まだ未実装です。特にnative WRITABLEは空き領域の有無だけなので、不可分書き込みを待つときに、
部分的な空きで無制限に再試行する実装にはしません。待機と通知の取りこぼしを防ぐ設計が必要です。

今回の通常サービス4つは診断前の185バイナリへ戻し、ハッシュが一致することを確認しています。
通常イメージも再生成して診断probeを取り除きました。サービスの本番経路へはまだ接続していないため、
同じ通常構成の起動試験を繰り返さず、直近の通常画面の証跡は185を参照します。
SDK・Rust std・runtime archive・カーネルは変更していません。

## 同じopen操作でのFIFO inode保持（187）

filesystemのOPEN_AT_RETAIN_FIFOを追加しました。通常のopen_atの同じ探索・権限確認の中で
FIFOを検出すると、opt-in要求にだけOPEN_REPLY_FIFO付きNodeStatusとmetadata-only Handleを
返せます。別のstatやopenの再試行でFIFO名を探索し直す方式ではありません。
この返信が完了してからデータownerへ渡すことで、filesystemの直列化処理中にfactory側が同じ
filesystemへ同期照会してデッドロックする構成を避けます。

保持はO_TRUNCやdisk File生成より前です。FIFOのO_TRUNCはディスク内容・時刻を変更しません。
O_RDWRは今回のFIFO実装で未対応として拒否し、O_DIRECTORYは従来どおりENOTDIRにします。
通常ファイルはopt-in bitがあっても従来の通常openになります。opt-inなしのFIFOはENOTSUPを
維持します。libcのFIFO data openがこの時点で動作するわけではありません。

公開する権限は要求flagsや応答flagsだけから選びません。open_publicationで保持中のinode種別を
確認し、FIFOにはCONTROL|DUPLICATE|TRANSFERだけを与えます。READ・WRITE・SEEK・MAPは付けません。
通常経路とmemory-admission付き経路が同じ公開処理を使用します。native referenceの作成失敗時は
保持entryを解放し、作成後の寿命は従来どおり最後のnative参照の解放に従います。

posix側のfifo_inode::validate_replyは、保持したfilesystem endpointからの応答を対象に、
要求ID・frame・型・device/inode整合性・metadata-only権限・Handle数を検査します。
これは受信参照を借用して検査する関数で、呼び出し側が成功時の引き受けと失敗時のcloseを担当します。
metadata参照だけではデータHandleのREAD/WRITEを許可しません。接続時には認証済みのopen処理が
許可した方向を保持する必要があり、クライアントの自己申告modeを根拠にデータ権限を増やしてはいけません。

追加したホスト試験は、同じopenでの保持とO_TRUNC無変更、unlink後の別名同名置換、要求権限を
過大に渡してもmetadata-onlyになること、native作成失敗時の参照解放、通常openの維持を扱います。
validatorではframe長、未知flags、誤った型・identity、過大権限、余分なHandle等も拒否します。
既存のPOSIX protocol・filesystem・マウント・FIFO・Provider・確保失敗試験と合わせて、
7ターゲット・685件が成功しました。全POSIXホスト試験を再実行したものではありません。
`out/checks/mochios-fifo-inode-187-related.log`へ保存しています。

実IPC試験は、FS control Handleを既に持つ診断版POSIXサービスにだけ組み込みます。
コードはtests/support/native_fifo_inode.rsに置き、専用cfgがない通常ビルドでは含めません。
サービス名検索、新しい本番向けテストRPC、アプリへのFS control endpoint追加はありません。
最初の診断では試験用Headerにpayload長を指定し忘れてIPC送信前に失敗し、明示的に指定して
修正しました。途中の失敗ログは`out/checks/mochios-fifo-inode-187-native-before.log`です。

修正後のKVM試験は、metadata-onlyのnative権限、拒否されたcredentialsへのHandle未返却、
保持参照のdup・元Handleのclose・unlink・同名置換後も元inodeを指すことの3群が成功しました。
`out/checks/mochios-fifo-inode-187-native-final.log`に保存しています。診断処理の後も通常の
サービス初期化へ戻り、Binder.app readyを確認しています。この診断版を配布イメージへ残さず、
試験cfgを含まない通常POSIXバイナリへ戻しました。

通常版で別のKVM起動を行い、診断メッセージが出ないこと、Binder.app ready、実際のDockと上部バーを
確認しました。`out/checks/mochios-fifo-inode-187-normal.log`と
`out/checks/mochios-fifo-inode-187-normal.png`に保存し、VMは停止済みです。
配置した通常サービスと診断ELFのハッシュは`out/checks/mochios-fifo-inode-187-artifacts.json`へ
記録しています。SDK・Rust std・runtime archiveは再生成しておらず、全アプリの再リンクも行っていません。

データ用Providerへこの保持参照を渡す処理、inodeごとのチャネル登録、data Handle公開、
blocking openの待機とキャンセル、libcのFD種別・poll/select・SIGPIPE等は引き続き未実装です。

## 保持inodeからデータProviderを管理する処理（188）

`fifo_manager.rs`に、検証済みのfilesystem応答を受け取り、inodeごとのChannelとnative
Provider objectを結び付ける管理処理を追加しました。filesystem authorityごとにローカルな
scopeを割り当て、別authorityの同じdevice/inode番号を混同しません。scopeとinode番号は
ルーティング用であり、権限を与える値ではありません。metadata Handleはopenごとに保持し、
READ/WRITE方向は認証済みのopen処理から受け取ります。外部要求の自己申告には使えません。

PTYで使っていた所有付きmetadata参照を`filesystem_reference.rs`へ移し、両Providerで
参照解放と権限縮小の処理を共有します。旧名の互換aliasは残していません。
管理処理は最大128チャネル・合計1024 open descriptionに制限します。チャネルのキュー上限は
64KiBです。登録用の確保をnative object作成前に完了し、失敗時は保持参照を解放します。
object IDは失敗後も再利用せず、最終native OP_RELEASEだけが登録とinode pinを破棄します。
blocking openの登録成功はopen完了を意味せず、FD公開前の待機は接続側の責務です。

ホスト試験は同一inodeの共有、別inode・別authorityとの分離、native作成失敗、上限拒否、
3箇所の確保失敗、metadata所有権と権限縮小、不正releaseの拒否を含みます。
既存FIFO・PTY関連と合わせ13ターゲット614件が成功しました。
`out/checks/mochios-fifo-manager-188-host-final.log`へ保存しています。
native通知エラーの分類試験も2件成功し、`mochios-fifo-manager-188-signal.log`へ保存しました。
全POSIX試験を再実行した結果ではありません。

診断版だけに含める`tests/support/native_fifo_manager.rs`で、実filesystemから取得した
保持参照を同一プロセス内のowner threadへ渡し、実native Handleで読み書きしました。
同一inodeの複数readerが同じキューを消費すること、unlink後の同名置換が別キューになること、
旧writerのcloseで旧readerだけがEOFになること、全参照のcloseでworkerが終了することを確認しました。
既存inode保持試験3群と今回3群のPASSを
`out/checks/mochios-fifo-manager-188-native-final.log`へ保存しています。
これはクロスプロセスのdata Handle転送やlibc FIFO openの証明ではありません。

最初の診断では、最終native closeからOP_RELEASEの配送までの間に、消滅済みobjectへの通知で
workerが停止しました。`mochios-fifo-manager-188-native-before.log`が失敗記録です。
既存socket側のnative通知分類を再利用し、所有IDと通知maskが既に検証済みの場合の
InvalidArgumentだけを「通知先参照なし」と扱います。他のエラーは伝播させ、metadataの
破棄は引き続きOP_RELEASEまで待ちます。カーネル変更はありません。

診断cfgを含まない通常版へ戻してイメージを再生成し、別のKVM起動でBinder.app readyと
実際のDock・上部バーを確認しました。VMは停止済みです。通常版のログと画面は
`out/checks/mochios-fifo-manager-188-normal.log`・`mochios-fifo-manager-188-normal.png`、
サービスと診断ELFのハッシュは`mochios-fifo-manager-188-artifacts.json`へ保存しています。
filesystem・compositor・service-managerは試験前と同じバイナリです。SDK・Rust std・
runtime archiveは再生成せず、全アプリの再リンクも行っていません。

本番のopen経路・libcのFIFOデータFDへはまだ接続していません。認証済みopenからownerへの
privateな受け渡し、blocking openの待機・キャンセル、PIPE_BUF書き込みの容量待機、
FD種別とmetadata操作、poll/select、SIGPIPE、exec/fork継承は引き続き必要です。

## open・読み取り・容量待機の状態処理（189）

`fifo_wait.rs`にowner側の一回限りの待機登録を追加しました。`WaitCondition`はopenの
待ち合わせ、読み取り、指定長の書き込みを区別します。キューに1バイトでも空きがあれば立つ
通常のWRITABLE通知とは別に、PIPE_BUF以下の書き込みでは全量の容量が空くまで待ちます。
それより大きい書き込みは部分進行できるので1バイトの空きで再試行できます。EOFと読み手消滅も
再試行を促す完了条件です。ゼロ長writeの通知登録は不要なので拒否します。

登録時に必ず状態を再確認し、失敗したI/Oから通知登録までの間の変化を取りこぼさない形です。
ownerは各open/read/write/release後、同じ排他区間で該当チャネルの待機を更新する必要があります。
一度readyになった通知は取り消しません。他のreader/writerにデータ・容量を先に使われても、
呼び出し側が起床して再試行できます。これは容量の予約や書き込み順序の保証ではありません。
通知callbackの失敗は伝播しつつ、他のready通知の処理を継続し、失敗した通知も再試行可能です。

待機は最大1024件で、確保はnative通知object作成前に行います。data objectと通知objectの
ID範囲を分け、失敗したnative作成のIDも再利用しません。各登録は取得済みdata Handleの
RAII leaseを保持する構造です。native adapterは明示的に添付されたHandleの識別と方向権限を
検査して、このleaseへ渡さなければなりません。数値のobject IDを権限として利用しません。
最終通知OP_RELEASEでその登録だけを破棄し、leaseを解放します。データキューを直接削除したり、
任意のdata objectをcloseしたりはしません。不正なdelivery、追加添付、別ID範囲の解放は拒否します。

ホスト試験で、空のFIFOのopen完了とreadableの区別、openの待ち合わせ成立後の相手close、
1バイトだけ空いた場合の512バイトwrite待機、512バイトの容量確保、読み手消滅、
登録直前のデータ到着、競合readerによる消費後の通知維持、通知失敗時の他jobへの配送、
不正release、作成・確保失敗時のlease解放、上限回収とID非再利用を確認しました。
既存FIFO関連と合わせ6ターゲット32件が成功し、
`out/checks/mochios-fifo-wait-189-host-final.log`へ保存しています。

今回はownerの状態処理とホスト試験までです。本番workerへの通知Handle発行・配送の接続、
libc側での待機・キャンセル・再試行、実data Handleを保持した状態でのnative最終releaseは
まだ今回の試験範囲に含みません。カーネルABI、SDK、runtime archive、配置済みイメージは
変更しておらず、KVMも起動していません。libc FIFO openが使えるようになったという変更ではありません。

## Native通知Handleの発行と解放（190）

`fifo_wait_ipc.rs`に通知用のIPC adapterを追加しました。要求はversion付きの16バイトで、
待機条件とwrite長だけを持ちます。PID・FD・object IDは載せません。明示的に添付された
READまたはWRITE Handleを、作成元の正確なendpoint threadでnative識別します。
別endpointやstale ID、方向不足、余分な権限、余分な添付は登録前に拒否します。
受信参照は検証後にだけRAII leaseへ移し、失敗時の所有権を二重に解放しない形です。

通知はnative Provider objectとして作成し、WAIT・DUPLICATE・TRANSFERだけを返します。
登録時にreadyなら返却前にREADABLEを設定します。作成・初回通知・replyの失敗では、
未公開または一時的な通知Handleを閉じ、最終native OP_RELEASEでjobを回収します。
公開後も生きた通知を、reply失敗の推測だけで先にjobから削除する処理はありません。
client側も、native IPC失敗時のin/outバッファに残った借用sourceをcloseせず、
成功したIPCで新しく受信した参照だけを検査・回収します。

ホスト試験ではframing/version、方向と権限縮小、識別・adopt失敗時の所有権、
借用Handleの維持、不正なreplyと重複した受信Handleの一度だけの回収を追加しました。
FIFO・Provider通知・PTY metadata関連を合わせ11ターゲット50件が成功し、
`out/checks/mochios-fifo-wait-ipc-190-related.log`へ保存しています。
全POSIXホスト試験を再実行した結果ではありません。

診断cfgにだけ含まれる既存FIFO workerを、このadapterで通知要求も処理する形にしました。
KVMでは、別endpointに作った同じ番号のobjectの拒否、native通知の待機とREAD権限拒否、
dupした通知のラッチ維持と未成立通知のキャンセル、512バイト分の容量が空くまでの待機、
最後の通知参照がwriter参照を保持し、その解放後にだけreaderがEOFになることを確認しました。
通知4群・既存inode保持3群・データ管理3群のPASSと、その後のBinder.app readyを
`out/checks/mochios-fifo-wait-ipc-190-native.log`へ保存しています。
同一プロセス内の別thread/endpointを使う実native IPC試験であり、別プロセスへの転送や
fork/exec、libcのblocking FIFO openまで確認したものではありません。

通常版と診断版のmochiOS向けビルドは成功しています。最初の診断ビルドでは試験コードの
HandleWaitItemフィールド名が既存ABIと合わず、既存のinterests/observedへ修正しました。
`mochios-fifo-wait-ipc-190-probe-build.log`と`mochios-fifo-wait-ipc-190-probe-build-final.log`が
修正前後の記録です。新しいsyscallやカーネル変更は追加していません。

診断cfgを除いた通常版へ戻してイメージを再生成し、別のKVM起動でBinder.app readyと
画面上のDock・上部バーを確認しました。通常版に診断メッセージはなく、VMは停止済みです。
ログと画面を`out/checks/mochios-fifo-wait-ipc-190-normal.log`・同名の`.png`、
サービスと診断ELF・使用runtimeのハッシュを`mochios-fifo-wait-ipc-190-artifacts.json`へ
保存しています。filesystem・compositor・service-managerは試験前と同じバイナリです。

本番のPOSIX起動・open dispatcherにはまだこのFIFO workerを接続していません。
今回のserve/queryはその接続で再利用するnative adapterで、診断workerで実際に呼び出しています。
認証済みopenの登録、libc FDへの公開・待機・キャンセル、metadata/poll/select/SIGPIPEと
継承の接続は引き続き必要です。SDK・Rust std・runtime archiveの再生成は行っていません。

## 本番workerの起動と保持inodeの内部委譲（191）

`fifo_native.rs`に、通常のposix.service起動で開始する専用owner threadを追加しました。
別のOSサービスではありません。継承済みFS endpointをローカルに複製して保持し、
このauthorityに対応するFilesystemScopeをworker内で一度だけ作成します。カーネルへ
サービス名・パス・FIFOの概念を追加していません。

`fifo_open_ipc.rs`の内部要求は、version、認証済みopenが許可した方向とnonblocking方針、
元のFS応答、明示的に添付するmetadata Handleから成ります。native endpoint-owner identityが
posix.service自身のプロセスと一致する要求だけを受理します。別プロセスがmetadataを持って
いても、自己申告の方向から新しいdata権限を作れるAPIではありません。FSの元の呼び出しとの
対応は送信側で検証し、workerでも応答の型・長さ・identity・権限を再検査します。
内部registerは元のFS参照を成功・失敗の両方で消費します。登録結果のready=falseは
blocking open未完了を表し、成功したPOSIX openとして公開してはいけません。

workerはdata要求・通知・最終release・metadata取得を一つの順序で処理します。
data Handleからのmetadata取得はPTYで使っていた検証とreply処理を共有し、inode pinの
権限縮小だけを行います。FSとの再帰的なRPCやパスの引き直しはありません。
既に完了したデータ転送の後でreadiness通知が失敗しても、転送結果をエラーへ置換して
進行済みデータを隠しません。通知失敗は記録し、jobのラッチを維持します。

workerの停止要求も同じプロセスからだけ受理し、data/通知の登録が残る間はEBUSYを返して
権限を維持します。最後のnative releaseを処理した後にだけ、ownerと保持authorityを終了できます。

ホスト試験では、非信頼要求によるREAD/WRITEの取得拒否、framingと添付の所有権、
認証済み方向・nonblocking方針の維持、共通metadata取得を追加しました。
既存FIFO・通知・PTY metadata関連と合わせ14ターゲット57件が成功し、
`out/checks/mochios-fifo-worker-191-related.log`へ保存しています。全POSIX試験ではありません。

通常のpathname open dispatcherとlibc FDへの接続はまだ行っていません。通常起動では
workerを開始しますが、アプリへFIFO openを成功として返す経路は引き続き未実装です。

診断cfgの追加試験は、この本番worker自身に実FS応答とmetadata HandleをIPCで渡します。
KVMで、未成立reader登録→writer登録→open通知→native読み書き、data Handleからの
metadata取得とunlink・同名置換後の元inode維持、使用中の停止拒否と最終release後の終了の
3群が成功しました。既存試験と合わせ13群のPASSを
`out/checks/mochios-fifo-worker-191-native.log`へ保存しています。
これは同一サービス内のthread間IPCであり、libcのPOSIX open完了や他プロセスのfork/execの証明ではありません。

通常版・診断版のmochiOS向けビルドは成功しています。最初の通常版ビルドでは
startup channelのResult型推論とProvider responseのstatus型が合わず、型を明示して修正しました。
`mochios-fifo-worker-191-normal-build.log`と`mochios-fifo-worker-191-normal-build-final.log`が
修正前後の記録です。SDK・Rust std・runtime archiveは再生成していません。

診断cfgを含まない通常版へ戻して別のKVM起動を行い、Binder.app ready、FIFO receive失敗や
診断PASSメッセージが出ないこと、実画面のDock・上部バーを確認しました。VMは停止済みです。
`out/checks/mochios-fifo-worker-191-normal.log`・同名の`.png`へ保存し、
サービス・診断ELF・使用runtimeのハッシュを`mochios-fifo-worker-191-artifacts.json`へ記録しました。
filesystem・compositor・service-managerは試験前と同じバイナリです。
