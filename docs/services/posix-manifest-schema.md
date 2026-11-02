# 実行宣言の共通TOML schema（327）

user/crates/package-manifestへ、ツールとサービスで共用するno_std + allocのparserと
PackageManifest型を置きました。platform::packageは同じ型・parserを再公開し、
capability.serviceの既存のmanifest認証・依存関係・要求Capabilityの読取もこれを使います。
msign elf sign --manifestも、埋め込む同じ宣言をこのschemaで検査します。

従来の行単位parserは、重複キーの上書き、不正escapeの受理、コメント・配列中の文字の
取り違えや未完了配列による要求の欠落を起こせました。標準TOML parserのparse / serde機能を
stdなしで使い、syntaxと型を検査してから既存のパス・package・dependency検査へ渡します。
旧parserへのfallbackはありません。旧行parserを撤去し、既存の検査を共通crateへ移しました。

文字列・配列・数値の型不一致、重複キー、曖昧なvendor / developerやpath / entryの併記、
重複・空・制御文字を含む要求Capabilityを拒否します。正規のUnicode escapeやliteral string、
引用符内のcomma / #、複数行配列はTOMLの規約で解釈します。
parserの既定の深さ制限を維持し、極端に深い配列の入れ子も拒否する試験を加えています。
[service] / [driver]の既存entry形式も同じtyped modelへ変換します。

一般manifestは1MiB、主要entry配列は8192、dependencyは256、requiresは256を上限とします。
ELF内manifestは既存codecの64KiB上限に従います。構文検査やpackage ID・provenanceの宣言から
信頼や権限を作りません。Install Recordと元bytesの照合、署名者の信頼、共通の実行認可、
保護された同意と最終grantの結合は各認証境界が引き続き担当します。
通常exec全体への署名検証強制と、完全POSIX環境のgoalは未完了です。

## 検証

関連ホスト79件、既存platform parserの5件、msign既定構成34件、実LLD / driver / msignの3件、
配置済み37パッケージを検査する1件が成功しました。ソース内のpackage manifest群も走査しています。
CExtの[cext] ABI manifestは別形式です。coreとallocだけのx86_64-unknown-none向けcheckも通り、
parserにstdやサービスI/Oを要求しないことを確認しました。

古いdynamic grantのパス再読込を前提にしていた検査は、現在その処理が存在しないことを
検査する形へ更新しました。実manifest読込のI/Oエラー保持・一回の読込と、package認証・依存関係の
試験は継続しています。テストはservices/posix/tests以下にあります。

記録はout/checks/mochios-manifest-schema-327-のhost.log、platform-regression.log、
msign-default.log、real-host.log、installed-host.log、nostd.log、native-build.logとartifacts.jsonです。

検証用targetのincremental cacheがディスクを圧迫していたため、実行中の利用者がないことと
Cargo lockを確認して、再生成可能な同cacheだけを削除しました。ソース、SDK、実行イメージと
検証記録は対象に含めていません。記録は同接頭辞のcache-cleanup.jsonです。

更新済みcapability.serviceで通常起動し、Binder READY、デスクトップ、DockからのTerminalと
msh promptを確認しました。通常PATHのclang / ld / ld.lld / msignでmanifestを含むELFを生成し、
元宣言との一致、入力manifest削除後のstdoutと終了値37、構文エラーと型エラーの両方での署名拒否と
旧ELFのbyte一致、宣言改変の検証拒否、cleanupとprompt復帰が成功しました。
画面は同接頭辞のresult.pngです。追加のCapabilityを付与する試験ではありません。

VM停止・解析処理の復帰後、所有するprobeだけを撤去し、更新したcapability.serviceと対応する
verification record、通常msignを保持してイメージを再生成しました。401ファイルと4つの
command symlinkを読み戻して照合し、probeの不在と6成果物のchecksumを確認しました。
SDKの入力hashは不変で、SDKとRust std overlayは再生成していません。
記録はnormal-image.logとreadback.jsonです。

## Capability確認設定も同じ認証済みschemaで読む（331）

capability.serviceの[prompt]には別の行parserが残っており、通常manifestの末尾カンマ付き
配列を空の確認設定として扱っていました。interactive_capabilitiesを共通PackageManifestへ
追加し、requiresと同じ文字列・配列・件数・重複の検査を使います。

起動時と更新時の両方で、既存resolverが認証したmanifestの同じsnapshotから設定を読みます。
org.mochios.capabilityのbuilt-inなSystem identityを要求し、古いindex内の解析結果を
現在の設定として使用しません。入力・認証・schema・Capability分類の失敗を空設定へ
置き換えず、起動を失敗させるか更新前のindexとpolicyを一緒に保持します。
認証済みmanifestが設定を宣言しない場合と、明示した空配列は引き続き空設定です。

本番policy / resolverを使う7件と、初期化・更新の5件を含む関連58件のhost試験、
配置済み38 manifestのschema検査が成功しました。通常の3対象権限、複数行・escape・comment、
欠落・改変・異なるpackage / provenance、不正な型と重複、I/O失敗と更新時の原子性を確認します。

更新したサービスでKVMを起動し、input READY、BinderとDock、通常Terminalのmsh prompt、
外部echoの出力とprompt復帰を画面で確認しました。未承認の確認対象を許可する変更や、
CLI / GUI同意と最終grantを接続した変更ではありません。その接続と完全POSIXのgoalは継続します。
既存のnetwork Failed(-6)とUSB errno 22も残っています。

証跡はout/checks/mochios-code-bindings-331-のprompt-final-host.log、installed-host.log、
native-build.log、native.logとresult.pngです。所有VMを停止して解析処理を復帰し、
通常イメージの403ファイルと4リンクを読み戻して照合しました。SDKとkernelは変更していません。

## 配置済みのコードと宣言の結合（332）

host msignへpackage bind-built-inを追加しました。コピー済みの各binaryを保持したFileから
末尾までSHA-256で測定し、sizeと実際のUnix modeをfile entryへ記録してbinaryと結び付けます。
既存の結合がある場合は同じbytes・size・modeを要求し、不一致を新しいhashで上書きしません。
通常のbuilt-in-recordも共通schemaで検査し、正確なmanifest bytesへ認証recordを結び付けます。

rootfs生成ではコードを配置してからこの処理を実行します。/binから/system/binへの対応は
ビルド側の明示的なprefix mappingで指定し、カーネルへ配置知識を追加しません。
無効なmperfとfilesystem selftestsだけを.configに従い明示除外し、それ以外の欠落は失敗です。
アプリケーションの追加設定、promptと要求権限はTOMLの値として保持します。
一般の署名・実行権限・同意を生成する操作ではなく、所有するstaging treeの整形です。

実CLIの7試験は全bytesの測定、既存結合の改変拒否、prefixの境界・最長一致、
root外へのsymlink、codeと出力のalias、失敗時の旧出力保持と、実際のrootfs関数を使う
4通りのfeature設定を確認します。msignの既存34試験と関連の既存30試験も成功しました。
初回の証明書4試験はsandboxがlocalhost bindを拒否したため失敗し、同じ試験を
localhostを利用できる環境で再実行して34件すべて成功しています。

実際のrootfs生成スクリプトで43 package / 65 binaryの一時treeを作り、本番resolverと
Catalogで全宣言の受理、codeの変更・切詰め・追記、manifest変更の拒否を確認しました。
通常イメージの既存38 package / 60 binaryについても同じ試験が成功しています。
通常イメージにはこの38 packageの76 metadataファイルだけを反映し、実行体は保持しました。
追加アプリを含む一時treeは認証記録を保存して削除しています。

hello、selftest-capability、selftest-processには既存のbinary宣言がありません。
この3コマンドの扱いと、通常exec / spawn / GUI起動の共通認可への接続は残件です。
既存のstatic manifest向けfallbackも残っており、その撤去は今回の完了範囲に含みません。
完全POSIX環境のgoalは継続します。

証跡はout/checks/mochios-code-bindings-332-のbind-final-host.log、msign-tests-localhost.log、
full-stage.log、full-stage-auth.log、installed-auth.log、regression.logにあります。

反映後のKVMでもinput READY、Binder、Dock、Terminal、外部echoの出力とmsh prompt復帰を
確認しました。所有VMを停止して解析処理を復帰し、475ファイルと4つのcommand linkを
生成イメージから読み戻して照合しています。SDK入力・既存実行体・kernelは不変で、
6成果物のchecksumも成功しました。native.log、result.png、readback.jsonに記録しています。
通常のmmake msign-toolも成功し、新しいhostコマンドを利用できます。

## インストール済みコードの旧fallbackを撤去（333）

Catalog::for_staticと、bootstrap rootにfile bindingがない場合の例外を削除しました。
アプリ起動は認証済みmanifestのroot / library結合と、保持したFileHandleから取得したbytesを
照合します。宣言に結合情報がない場合もエラーとなり、別のパス読込で信頼を補いません。

起動用イメージの準備と初期サービス起動も、verified_manifestの同じsnapshotから
実行主体・要求権限・コード結合を取得します。キャッシュされたindexの宣言は認証情報として
使用せず、初期サービスでは選択したpackage IDも認証結果と一致させます。
取得した元bytesのsize / SHA-256を確認してからnative認可を発行します。
イメージ準備の認証・policy・code失敗はnative割当より先に返し、失敗時の参照回収は維持します。

関連77件のhost試験が成功しました。初期サービスの認証は本番resolver / Catalogと実際の
Install Recordを使い、古いindexの無視、一回のmanifest / record読取、正確な取得bytesの
認可、改変・結合なし・package差し替え・読取失敗での非発行を確認します。
native割当・作成の順序と参照回収は既存の境界試験でも確認しています。
通常イメージの38 package / 60 binaryについても、実コードの受理と変更・切詰め・追記、
manifest変更の拒否が本番resolver / Catalogで成功しました。

この変更は通常POSIX execのAUTHORIZE_EXEC_OBJECTを置き換えるものではありません。
単体ELF / installed sourceの選択、通常exec / spawnへの共通認可接続、保護されたCLI / GUI
同意と最終grantの結合、未宣言の3診断コマンドと完全POSIX環境のgoalは引き続き残ります。

証跡はout/checks/mochios-bound-images-333-のhost-final.log、bootstrap-auth.log、
installed-auth.log、native-final-build.logです。SDKとRust std overlayは再生成していません。

更新したcapability.serviceと、そのコードを測定したmanifest / verificationを通常イメージへ
反映しました。KVMで初期サービス、input READY、BinderとDock、Terminalの外部echoと
msh prompt復帰、FilesのHome表示が成功しています。所有VMを停止して解析処理を復帰し、
475ファイルと4つのcommand linkをイメージから読み戻して照合しました。
kernelとSDKは不変です。native.log、result.png、files.png、readback.jsonに記録しています。

## 通常execも保持済みコードから実行主体を選ぶ（334）

通常libc execのAUTHORIZE_EXEC_OBJECTを、署名付きRAM認可と同じsource検証へ接続します。
以前の「呼出元と同じidentity・通常Capabilityを継承して置換する」発行処理は削除しました。
要求内のdigestは一致検査にだけ使用し、それ自体を認証情報として扱いません。
Providerを読み始める前にnative RAMであることと長さを確認し、取得したbounded snapshotの
全bytesを測定します。受信した参照の回収と、失敗時に旧imageを保持する処理は維持します。

同じbytesがインストール済みコードに一致する場合は、indexを候補検索にだけ使い、
実際に認証した一つのmanifestからidentity、要求権限、sizeとdigestを取得します。
認証失敗、非application、曖昧な複数宣言は拒否し、単体ELFへ格下げして再認可しません。
インストール済みコードでなければ、既存の共通ELF署名検証と単体ELFの選択を使います。
AdHocの要求なし実行体は元ELF全体のhashに対応するdevelopment identityと空のgrantを持ち、
mshのidentityや通常Capabilityを取得しません。保護された確認が必要な要求は未承認のまま
発行せず、通常execの失敗を旧継承経路へ戻す処理もありません。

署名付きRAM protocolは同じopcodeのversion 3とし、以前の版を拒否します。
通常exec transportとELF署名v1の形式は変更しません。SDKとRust std overlayは再生成しません。
静的ELFをProviderのまま渡す旧runtimeは利用できず、共有preparerで私有RAMへ捕捉する
現行runtimeが必要です。既存clang内に旧exec処理が残り、最初の起動は成功しても
内部のposix_spawnがEINVALとなったため、現在のstandalone runtimeでclang / ld.lldを
再リンクします。既存CMakeのlink commandとobject / libraryを使い、SDKは読み取りのままです。
開発ツールのcode bindingは既存stage-development-tools.pyの生成処理とmsignで更新し、
package identityと要求権限は変更しません。

最初のKVM試験はprobeの起動時にPermission deniedとなりました。通常execが添付するREADだけの
RAM参照に対し、MemoryObjectLengthがMAPを要求していたためです。長さの観測はREADまたはMAPで
可能とし、一回のHandle table参照から保持したobjectと権限を検査します。Providerや
Provider-backed VMは依然として所有者への問い合わせなしで拒否し、mapping権限を追加しません。
kernelへ署名・manifest・サービス名・POSIXの知識を追加する変更ではありません。

動的なprepared outputのsource・依存先との認証結合、CLI / GUI同意と対象imageへの
原子的なgrant付与、未宣言の診断コマンドと完全POSIXのgoalは引き続き残ります。
元bytesと異なる出力を、呼出元のdigest申告や元のパスの再読込だけで認可しません。

関連9 targetsのhost試験は計80件成功、失敗0件、ignored 2件です。
RAMの権限試験はproductionの長さ取得とHandle参照取得を実行し、全2048通りの権限、
READだけ・MAPだけ、権限なし、別種のobject、Provider-backed VM、無効参照とcaller不在を
検査します。既存のRAM生成失敗試験に残っていた旧設定名も、現行のper-object上限へ
合わせました。失敗注入・割当回収・権限検査は維持しています。

KVMでは通常DockのTerminal / mshから、通常PATHのclangによるcompile、共通ld driver、
実LLD、自動AdHoc署名、msign verifyと通常の./mainがすべて成功しました。
生成したCプログラム自身がnative queryで全canonical Capabilityの0を確認し、
元の署名済みELF全体のSHA-256に対応するpackage IDとdevelopment provenanceを検査します。
stdoutと終了値37、compile / link失敗での旧ELF保持、msignの改変拒否に加え、
改変したELFを通常execもPermission deniedで拒否すること、cleanupとmsh prompt復帰を
確認しました。これは新しい権限を承認・付与する試験ではありません。

証跡はout/checks/mochios-exec-source-334-のhost-verified.log、memory-regression.log、
kernel-final-build.log、runtime-build.log、result.pngとartifacts.jsonです。
所有VMはPIDと開始時刻を照合して停止し、解析処理の復帰記録も確認しています。
最初の二つの失敗画面・ログは保存し、成功結果へ置き換えていません。

成功後、再リンクした2ツールを通常のLLVMビルド出力にも反映し、probeとそのpackageの
計3ファイルだけを撤去しました。更新したkernel / capability.service / 開発ツールと対応する
認証情報を保持して通常イメージを再生成し、475ファイルと4つのcommand linkを読み戻して
照合しています。probeの不在と6成果物のchecksumも成功しました。
normal-image.logとreadback.jsonに記録しています。SDKのruntimeを含む入力は不変で、
生成Cからのfork / execや、承認による新規Capability付与までを検証したものではありません。
