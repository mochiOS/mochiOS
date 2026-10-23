# 登録済みデバイスの情報観測

端末pathnameの`pathconf`に必要な、データopenを伴わない提供元の観測経路です。
178ではfactoryの観測インターフェースまでを実装しました。
179でfilesystemのパス解決、POSIX層、newlibの`pathconf`を接続しています。

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

178では、filesystemの認証済みパス解決とPOSIX層・newlibの接続が残っていました。
179で追加した接続と、その検証範囲は後述します。

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

## 端末pathnameの接続（179）

newlibの`pathconf(path, name)`は、端末用の3つのnameに対してPOSIXの
`OP_TERMINAL_LIMITS_AT`（122）を使います。payloadはflags 0のStatAtRequestで、
クライアントFDや実アドレスは渡しません。POSIX層はカーネルが保証したpeerのsessionから
credentials・root・cwdを取得し、保持したroot/baseを使ってfilesystem RPCを行います。
RPCを待つ間にPOSIXのSTATEロックを保持しません。execでimageが入れ替わった場合の
応答破棄と、借りたdirectory Handleの回収も既存のpath_request経路に従います。

filesystemの`OP_DEVICE_OBSERVATION_AT`（51）は、STAT_ATのroot/base添付規則を使います。
認証済みcredentialsの後に24バイトのdevice_observation::Requestとpathnameを置きます。
この要求は委譲済みcontrol endpointだけで受理し、global endpointからの自己申告credentialsを
信用しません。inline open/node/mount IDは対象選択に使わず、不正な値は拒否します。

filesystemはsymlinkを辿り、ディレクトリ検索権限とroot境界を検査して一度だけ対象を解決し、
そのNodeに登録されたfactoryを保持して観測します。final componentへのデータREAD許可や
新しいopen descriptionを要求しません。応答が指定容量を超えた場合はEIOです。
filesystemはopaque selectorと情報バイト列を運び、POSIX固有の端末値を捏造しません。

POSIXバックエンドだけがPTY_QUERY_LIMITSを選び、12バイトの端末制限を厳密にdecodeします。
非端末で未提供の観測はEINVALへ変換し、その他の提供元・権限・パス解決エラーは伝播します。
newlibでは`fpathconf`と同じ値の選択処理を使い、`_PC_MAX_CANON`・`_PC_MAX_INPUT`・
`_PC_VDISABLE`を返します。成功時のerrnoを保持し、data openへのfallbackはありません。

ホスト全体は656ターゲット・3,183件成功、失敗0件でした。
実filesystemのmode 000デバイス、検索拒否、symlink、root/base、無効なanchor、
提供元エラーと過長応答を確認しました。POSIXの読み取り専用要求の検証行列にも
端末問い合わせを含め、STATEを保持したRPCやimage変更後の応答採用を防いでいます。
初回は新しいtransport fixtureに必須cleanupメソッドがなくコンパイルに失敗しました。
fixtureを修正して全体を再実行し、成功ログと区別して保存しています。

KVM上の実newlibでは3つのアクセスモードでpathnameの3設定を取得できました。
forkした子を`/dev/pts`へchrootしてUID 501へ変更した状態でも、mode 000端末への
設定問い合わせが成功し、同じ端末のdata openはEACCESでした。
chroot外の元のpathnameはENOENT、非端末はEINVAL、不正ポインタはEFAULTでした。
これは実際のPOSIX→filesystem→PTYの別プロセス間IPCであり、178の独立した
同一プロセス内プローブだけを根拠にした結果ではありません。

ログは`out/checks/mochios-posix-host-audit-179-final.log`と
`out/checks/mochios-terminal-pathconf-179-native.log`です。SDKとRust stdは再生成せず、
独立した出力先の新runtimeをサービスと診断実行体へリンクしています。

通常イメージには新しいfilesystem.service・posix.serviceを配置し、診断用compositorと
service-managerは元のバイナリへ戻しました。同じ通常起動でBinderのREADY、Dockと
ステータスバーを確認しています。ログと視覚確認した画像は
`out/checks/mochios-normal-179.log`・`out/checks/mochios-normal-179.png`です。
既存アプリ全体の新runtimeでの再リンク、Vim・WebKitの起動、POSIX全体の適合を
確認したものではありません。
