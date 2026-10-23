# 名前付きFIFO

最新の実装範囲は末尾の184です。183は共有状態を追加した時点の記録です。

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
