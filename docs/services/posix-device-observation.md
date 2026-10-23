# 登録済みデバイスの情報観測

端末pathnameの`pathconf`に必要な、データopenを伴わない提供元の観測経路です。
この変更はfactoryの観測インターフェースまでです。filesystemのパス解決とnewlibの
`pathconf`はまだ接続しておらず、端末pathnameへの問い合わせは引き続き未対応です。

## 権限とメッセージ

filesystemが登録時に保持したSEND endpointと、対象デバイスのfactory Handleを使います。
factoryのTRANSFER権限だけをIPCへ明示的に添付し、提供元はネイティブの
ProviderObjectIdentifyで自分が所有するfactoryか確認します。PID、FD、provider ID、
サービス名やpathnameは要求に載せず、別のオブジェクトや解放済みfactoryは拒否します。
データ用Handle・filesystem metadata用Handleの作成や委譲は必要ありません。

filesystem-protocolの`device_observation`はmagic `DVOB`、version 1、24バイト固定です。
magic 4バイト、version u16、予約u16、opaque selector u64、応答容量u32、予約u32を
little endianで渡します。予約領域は0、応答容量は最大4,096バイトです。
応答は符号付き64bitのwire statusと、その後の情報バイト列です。応答Handleは禁止です。
形式・応答長・status・添付を検証し、異常応答の受信済みHandleは重複を除いて回収します。
ネイティブIPCが失敗した場合は、要求で借りたfactory Handleを回収してはいけません。

`Factory::observe`は未実装の提供元ではENOTSUPを返します。
NativeFactoryは保持済みのendpointとfactoryだけを使い、一度のIPCで実行します。
失敗時の再発見、権限の追加、data openへのfallbackはありません。

## PTY側の読み取り専用境界

PTYは`PTY_QUERY_LIMITS`のみ受理し、既存の端末制限エンコードを使います。
factoryからslaveを開いたり、ロックを解除したり、FDやopen-file descriptionを
確保したりしません。属性、readiness、入力queue、signal queueを変更しません。
設定変更・flush・flow等のcontrol selectorは、この経路ではENOTSUPです。
データ用master/slave Handleをfactoryとして渡すことも拒否します。

factory呼び出しはfilesystemの直列化されたworkerで行われる場合があります。
提供元は自分の保持状態から回答し、そのworkerへ同期RPCを返してはいけません。
今回のPTY観測も端末状態のロック内で完了し、filesystemへ呼び返しません。
カーネルABIや、カーネルが知るサービス名・端末selectorは追加していません。

## 検証と残る接続

`device_observation.rs`は実際のfilesystem側交換アダプタとPTY側admissionを接続し、
受信時のHandle番号の変化をモデル化して検証します。端末データが保持され、
新しいopen object IDを消費しないこと、出所・添付数・権限・selector・容量の拒否、
不正応答での参照回収、transport errorで借用参照を壊さないことを確認します。

`native_device_observation.rs`は別threadで所有するネイティブendpointへ実Handleを添付し、
実際のNativeFactoryとPTYのserveを使う独立したプローブです。filesystemへのパス要求や
別プロセスのfilesystem→PTY間の統合検証は、このプローブの対象ではありません。

次に必要なのは、filesystemでroot/base・credentials・検索権限を使って一度だけ
デバイスを解決し、その登録済みfactoryを保持して観測する処理です。
その上でPOSIX層とnewlibのpathname問い合わせを接続する必要があります。
final componentへのREAD permissionを暗黙に要求したり、端末の値をext4の制限で
代用したりせず、symlink・chroot・並行した名前の置換でも対象の同一性を維持します。

## 検証結果（178）

ホスト全体は656ターゲット・3,176件成功、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-178-final.log`です。
KVMの独立プローブではfactory Handleの実添付と所有者照合、変更selectorの拒否、
factoryではないオブジェクトの拒否、入力・属性・readiness・signal・open IDの不変性を
確認しました。`out/checks/mochios-device-observation-178-native.log`に3つのPASSがあります。
このプローブは同一プロセスの別thread間のネイティブIPCです。filesystemからPTYへの
別プロセス間のpathname問い合わせを検証したものとして数えてはいけません。

追加したディスパッチを含むposix.serviceを通常イメージへ配置し、診断用compositorと
service-managerを元のバイナリへ戻しました。filesystemの通常イメージ上のバイナリは
変更していません。BinderのREADY、Dockとステータスバーを同じ通常起動で確認しました。
ログと視覚確認した画像は`out/checks/mochios-normal-178.log`・
`out/checks/mochios-normal-178.png`です。SDK・Rust std・newlibは再生成していません。
