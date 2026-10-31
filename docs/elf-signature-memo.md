# ELF Signature v1 設計書

文書日：2026年9月21日  
状態：設計方針の整理。格納方式・ワイヤ形式・信頼ポリシーの詳細は未確定です。

## 1. 目的

`ELF Signature v1` は、mochiOSの実行形式をELFのまま維持し、ELF自体に署名情報を持たせるための共通形式です。

主な目的は、MPKGを作成・インストールしていない単体ELFにも、完全性の検証と実行ポリシーの評価を行える経路を用意することです。既存のMPKG署名を置き換えるのではなく、パッケージとしての認証と、実行ファイルとしての検証を分離します。

AdHocと鍵付き署名を区別し、署名の検証、署名鍵への信頼、実行許可、Capabilityの付与を、それぞれ別の判断として扱います。

本書は、この設計方針と実装に必要な安全条件を整理します。以前提示されたマジック値、固定長160 bytesのDescriptor、フィールドのオフセットなどは確定仕様として扱いません。

## 2. 現在の問題と変更方針

### 2.1. 単体ELFの実行がパッケージ管理に依存しています

現行の設計文書では、Developer Certificateを検証し、その公開鍵でmanifestの署名を確認したうえで、manifestに記録されたサイズとSHA-256により各payloadを検証します。実行時にはPackage Index、Install Record、manifest、実行対象を照合します。[1]

この方式は、MPKGに含まれるELFを間接的に保護しています。「ELFに独立した署名がないから、既存のパッケージ内ELFが無保護」という意味ではありません。

一方、パッケージに登録されていないELFを実行するための独立した経路が必要です。

そこで、ELF自身の署名情報を検証し、パッケージ外の実行ポリシーへ接続します。ただし、署名を付けるだけで自動的に実行可能になるわけではありません。OS側の検証・認可経路も必要です。

### 2.2. 署名者との対応をELFだけでは持ち運べません

現行のパッケージ認証は、実行ファイルの外側にあるmanifestやInstall Recordに依存しています。[1]

そこで、鍵付きELF署名に署名鍵の識別情報と署名値を持たせます。ELFをコピーしても、その署名情報はファイル内に残ります。

ただし、受け取り側が公開鍵と信頼情報を取得できなければ、署名の検証や開発者の認証は完了しません。ファイル内に鍵IDがあることと、その鍵をOSが信頼していることは別です。

### 2.3. 最終成果物を検証可能な形に仕上げる共通工程が必要です

各言語のコンパイラへ個別に署名機能を実装すると、処理や運用が分散します。

そこで、ELFを生成した後の共通工程として署名を行います。`msign`を共通の処理入口とし、将来のリンカラッパーやビルドシステムから呼び出せる構成を目指します。

この変更は、コンパイラの移植やlibc・SDKの提供とは別です。現状利用できないGCCやLinuxApplicationVMを利用可能にする変更ではありません。mochiOS上でのコンパイル直後の実行は、コンパイラ環境の整備後に実現する開発体験です。

## 3. 責務の分離

| 構成要素 | 責務 |
| :-- | :-- |
| ELF Signature v1 | ELFの検証対象と署名情報を表現します。鍵付き署名では、検証に使用した公開鍵との対応を確認します。 |
| Developer Certificate・信頼情報 | 公開鍵とDeveloper Identityの対応、用途、有効期限、失効、許可範囲を判断します。 |
| MPKG・署名済みmanifest | Package Identity、含まれるファイル、その内容、要求Capabilityの対応を保護します。 |
| capability.service | 検証結果とポリシーを基に、実行許可、実行主体のIdentity、実効Capabilityを決定します。 |
| mnu | 認可された実行対象と実際のロード対象の一致を確認し、Capabilityとプロセス隔離を強制します。 |

Package ID、Developer ID、要求Capability、ユーザー許可、Install Provenanceは、ELF署名形式の責務に含めません。

ただし、責務を分けることと、データ同士の対応を検証しないことは異なります。ELF、manifest、証明書、実行認可の間には、検証可能な結び付けが必要です。

## 4. 署名の種類と保証範囲

### 4.1. AdHoc

AdHocは秘密鍵を使用せず、ELFの内容と格納されたダイジェストの自己整合性を確認する方式です。

ファイルだけが変化し、ダイジェストが更新されていなければ不一致を検出できます。しかし、攻撃者がELFとダイジェストの両方を変更できる場合は、AdHoc情報を作り直せます。そのため、AdHocを出所の認証や攻撃者に対する真正性の保証として扱いません。

AdHocからは、開発者、生成したマシン、使用したコンパイラ、ソースとの対応、ビルドの成功を証明できません。「ローカルで自分がビルドした」という事実も、AdHocだけでは判断できません。

AdHocの実行には、Developer Modeやユーザーの許可など、別のポリシーを適用します。AdHocが存在することを、Capability付与やインストール済みアプリのIdentity取得の根拠にはしません。

### 4.2. 鍵付き署名

鍵付き署名では、信頼性が確認された実装と適切に管理された鍵を前提として、署名対象の内容が対応する秘密鍵によって署名されたことを公開鍵で検証します。署名検証にはメッセージ、公開鍵、署名値が必要です。[5]

検証結果として扱うのは、「この公開鍵に対応する署名が、このELFに対して有効である」という事実です。

Developer Identityを名乗るには、さらにその公開鍵と開発者を結び付ける証明書・信頼情報が必要です。署名が有効でも、コードの無害性、脆弱性の不存在、実行許可、Capabilityは保証しません。

本書では名称を「鍵付き署名」とします。`Identified`などのワイヤ上の名称や数値は、形式の詳細を決める段階で扱います。

## 5. 署名鍵の解決と信頼

### 5.1. 検証と信頼判断を分けます

署名情報から取得した鍵IDは、最初は未検証の識別子です。対応する公開鍵を取得し、鍵IDとの一致と署名の正しさを確認してから、検証済みの署名鍵として扱います。

その後、証明書やローカルの信頼設定を基に、その鍵をどの用途で認めるか判断します。この公開鍵の検索と信頼情報の解決を、本書では「署名鍵の解決」と呼びます。独立した新サービスの追加は、この呼称だけでは決まりません。

鍵IDに公開鍵のハッシュを使う場合、鍵IDから公開鍵そのものを復元することはできません。公開鍵または証明書の取得・保存経路が別途必要です。

### 5.2. Developer Certificateを使用する場合

既存のDeveloper PKIを再利用する方向です。証明書の署名、用途、期限、失効、対象範囲を確認し、許可された公開鍵でELFを検証します。

ただし、現行のDeveloper Certificate v1が定義するkey usageはpackage signingです。これを独立したELF認証にどう適用するかは、別途定義が必要です。既存の証明書があるという理由だけで、無条件にあらゆる署名用途へ転用しません。[1]

インストール後もELF署名を検証する場合は、証明書本体または公開鍵と信頼情報を保持し、検索できるようにします。Install Record内の鍵IDだけで公開鍵を取得できる設計とはしません。

### 5.3. ローカル鍵を使用する場合

ローカル鍵の登録は、鍵付き署名をローカル開発に使用するための候補です。AdHocには鍵が不要なため、ELF署名の導入だけを理由に、マシン共通の秘密鍵を自動生成する必要はありません。

採用する場合は、鍵の所有ユーザー、署名を要求できる主体、公開鍵の登録権限、信頼する用途、失効方法を定義します。あるユーザーが登録した鍵を、別のユーザーやシステム全体が自動的に信頼することは避けます。

ローカル鍵を信頼しても、それを正式なDeveloper Identityや強いCapabilityを与える根拠にはしません。

### 5.4. 未知の鍵と不正な署名

公開鍵を取得できない場合、署名の暗号学的な有効性は未確認です。公開鍵を取得して署名を検証できても、信頼の根拠がなければ「有効だが信頼されていない署名」です。

未知鍵、信頼されていない鍵、不正な署名を区別します。鍵付き署名の検証に失敗したものを、自動的にAdHoc検証成功として扱いません。

## 6. Capabilityとの関係

### 6.1. ELF署名には要求Capabilityを重複して格納しません

要求Capabilityはmanifestなどの外部情報に置き、権限を与える判断は`capability.service`が行います。

ただし、「ELF署名にCapabilityを格納しない」ことは、「要求Capabilityを改ざんから保護しなくてよい」ことではありません。

### 6.2. MPKG内のELF

パッケージの署名済みmanifestにより、要求Capabilityと対象ELFのハッシュを結び付けます。これにより、同じ開発者が署名した別のELFへの差し替えも、パッケージの内容不一致として検出します。

ELF署名が有効でも、Package Index、Install Record、manifest、payloadの検証を省略しません。ELFの署名者とパッケージ署名者の関係も確認対象とし、同一鍵を要求するか、明示的な委譲を認めるかは未決定です。

現行の設計文書では、要求Capability、証明書の許可範囲、System Policy、User Grant、Caller／Delegation Ceilingから実効Capabilityを決め、必須要求を満たさない場合は拒否します。この制限をELF署名の追加によって緩めません。[1]

### 6.3. パッケージ外のELF

外部manifestを使用する場合、その要求は権限の自己申告として扱います。要求に記載されたCapabilityをそのまま付与しません。

実行を許可したELFのダイジェストと、その時点で評価した要求内容を結び付けます。ELFまたは要求が変更された場合は再評価し、別の内容に対して以前の許可を無条件に使い回しません。

証明書を持たない実行経路には、証明書の許可範囲を無制限とみなすのではなく、ローカル実行用の権限上限が必要です。初期Capability、ユーザー承認の粒度、再ビルド時に維持できる許可の範囲は未決定です。

## 7. 実行時の処理

### 7.1. 検証結果を実行認可へ渡します

実行要求に対して、まずパッケージ管理下のELFか、パッケージ外のELFかを判定します。

パッケージ管理下なら、既存のパッケージ認証と内容の照合を行います。パッケージ外なら、ELF署名の検証結果をローカル実行ポリシーへ渡します。どちらも、署名検証だけでは実行を開始しません。

`capability.service`は、署名の状態、鍵への信頼、実行元の制限、ユーザー許可、対象の要求を評価して、実行主体のIdentityと実効Capabilityを決定します。

### 7.2. 一回限りの実行認可を維持します

現行の実行認可は、要求元スレッド、パス、実行ファイルのSHA-256、ApplicationIdentity、Capability、実行種別などに結び付けられ、カーネルが実際にロードしたバイト列と照合する設計です。この仕組みを維持します。[1]

新経路でも、検証とポリシー判断の対象にした同一のELFバイト列から認可用ダイジェストを求めます。署名を検証した後でパスを開き直し、別の内容のハッシュを「検証済み」として登録してはいけません。

カーネル側は、実際に実行へ使用するバイト列を再測定します。認可時と一致しなければ、プロセス生成または実行イメージの置換を拒否します。

### 7.3. 失敗したパッケージ検証を別経路で通しません

パッケージの内容不一致やInstall Recordの破損を検出した場合、その失敗を理由にローカル実行へ自動的に切り替えてはいけません。

ローカル実行を認める場合も、パッケージとしてのIdentityや既存の権限を引き継がない、明示的に別の判断が必要です。

### 7.4. 実行後の保護は別の責務です

ELF署名と実行時のハッシュ照合は、ロード対象の認証・整合性に関する仕組みです。実行後のメモリ改変、動的に読み込むコード、共有ライブラリ、JITコードなどを、ELF一つの署名だけで保護できるとは扱いません。

それらはメモリ保護、ローダーの検証、Capability、サンドボックスなどの別の境界として設計します。

## 8. 署名の生成とビルド工程

署名は、保護対象となる最終ELFに対して生成します。リンク後に`strip`、`objcopy`、メタデータ追加などで保護対象のバイト列を変更する場合は、変更後の成果物を改めて署名します。

リンカラッパーを採用する場合は、本来のリンカの正常終了と生成物の種類を確認した後、共通の署名処理を呼び出します。部分リンクの生成物や署名対象外のファイルまで、無条件に実行ファイルとして署名しません。

ただし、すべてのコンパイラが同じ`ld`を呼ぶことを前提にはしません。各ターゲット設定でラッパーを使用させるか、別のリンク経路にはビルドシステムから共通処理を接続します。

自動署名は開発体験のための仕組みであり、セキュリティ境界ではありません。ラッパーを通さず生成されたELFも、OS側では同じ検証・認可規則を適用します。

鍵付き署名を行う場合、署名の実装と鍵へのアクセス制御をコンパイラごとに分散させません。`msign`自身が鍵を扱うか、鍵管理サービスへ署名を依頼するかは、ローカル鍵の運用と合わせて決めます。

## 9. バイナリ形式と検証方式の候補

この節は、実装前に評価する方式の候補です。格納位置、長さ、オフセット、識別値は確定していません。

### 9.1. ELF内の格納場所

ELF NOTEを利用し、`.note.mochios.signature`のような名前を付ける案があります。ELFでは`SHT_NOTE`と`PT_NOTE`にNOTE情報を表現できますが、セクションとプログラムヘッダーは役割が異なります。[3][4]

採用前に、署名をどの構造から発見するか、Section Header TableのないELFを認めるか、`strip`との互換性、アラインメント、署名領域のロード可否を決めます。セクション名を決めただけでは、検証側の発見規則は完成しません。

### 9.2. 暗号方式と識別情報

現在の候補は、ダイジェストにSHA-256、鍵付き署名にEd25519を使用する方式です。鍵IDを公開鍵のSHA-256とする案は、現行のDeveloper CertificateのSubject Key IDと対応させやすい方式です。[1]

形式には、バージョン、署名の種類、使用アルゴリズム、鍵の識別情報、検証値を明示する必要があります。具体的なフィールドの幅、並び、固定長・可変長の選択は未確定です。

### 9.3. 署名対象

ELFヘッダー、Program Header、ロード対象データ、署名の意味を決めるメタデータを保護対象に含めます。実行内容に影響する情報を、任意に署名対象から除外できる設計にはしません。

現在の候補は、ELFファイル全体を対象とし、埋め込んだダイジェスト値と署名値の格納領域だけをゼロとして計算する方式です。これにより、署名値自身を署名対象へ含める自己参照を避けます。

この方式を採用する場合、除外する範囲は検証済みの構造から一意に導きます。除外領域が実行コード、実行に使用するデータ、ELFの制御構造と重複しないことを検証し、入力により除外範囲を自由に拡大できないようにします。

鍵付き署名のメッセージには、用途を識別する固定の文脈、形式・アルゴリズム・鍵の情報、ELFのダイジェストを結び付ける案です。正確なバイト列はワイヤ形式と合わせて定義します。

なお、SHA-256の値を含むメッセージに通常のEd25519で署名する方式と、RFC 8032で定義される`Ed25519ph`は同じ方式ではありません。実装・仕様で両者を混同しません。[5]

### 9.4. 署名用と実行認可用のダイジェストを区別します

署名値の格納領域を除外して計算するダイジェストは、ELF内の署名を生成・検証するために使用します。

一方、mnuへ渡す実行認可用のダイジェストは、署名値も含めた実際のファイル全体のバイト列から計算します。署名用の正規化ダイジェストを、そのまま実行認可用の値として流用しません。

## 10. 必要な安全条件

| 対象 | 必要な条件 |
| :-- | :-- |
| 構造の解析 | 範囲外参照、整数オーバーフロー、不正な長さ、矛盾する構造を拒否します。入力サイズと処理量にも上限を設けます。 |
| 署名の解釈 | 重複や曖昧な署名情報を拒否します。同じ領域が複数のヘッダーから参照される場合の扱いも一意にします。 |
| 形式の拡張 | 未対応の形式・アルゴリズム・フラグを、既知の形式として推測して処理しません。 |
| AdHoc | 鍵への信頼、Developer Identity、Capabilityの根拠にはしません。 |
| 鍵付き署名 | 公開鍵、署名値、メッセージを正しく検証し、鍵IDと公開鍵の対応も確認します。[5] |
| 外部メタデータ | 許可の対象となったELFと、評価した要求Capabilityの対応を維持します。 |
| 認可とロード | 検証したバイト列を認可へ結び付け、ロードしたバイト列が異なれば拒否します。 |
| 失敗時の扱い | 不正な鍵付き署名や失敗したパッケージ検証を、弱い経路へ自動的に降格して成功させません。 |
| 鍵とキャッシュ | 失効、信頼設定、ポリシーの変更を検証結果に反映します。キャッシュだけを永続的な信頼根拠にしません。 |

署名情報を削除したELFを、以前の署名者のIdentityや権限で実行してはいけません。未署名実行を認める場合も、独立した明示的ポリシーで扱います。

## 11. 実装前に決める項目

### 11.1. 署名形式

格納方式、発見規則、対応するELF class・endianness・ファイル種別、署名数、Descriptorの構成、マジック値、長さ、アラインメント、予約領域、アルゴリズム、署名メッセージ、ハッシュの正規化規則を決めます。

### 11.2. 鍵と信頼の運用

公開鍵・証明書の保存先と検索方法、Developer CertificateのELF認証への適用範囲、鍵の失効、オフライン時の判定を決めます。ローカル鍵を採用するか、その登録権限とユーザーごとの信頼範囲もここに含めます。

### 11.3. ローカル実行のIdentityと権限

AdHoc、鍵付き、未署名の各経路について、Developer Mode、ユーザー承認、初期Capability、永続許可、再ビルド時の扱いを決めます。

現行コードの`Development`は、`development-package-signing`用途を持つIssuerで検証された開発用パッケージに付く分類です。単体ELFのローカル実行と同じ意味にはしません。`LocalDevelopment`などの新しいProvenanceを追加するか、別の実行Identityを設けるかは未決定です。[2]

### 11.4. MPKGとの組み合わせと移行

既存MPKGにELF署名を必須とするか、導入初期は任意とするかを決めます。また、ELF署名者とパッケージ署名者の一致規則、再署名のタイミング、古いOSとの互換性を定義します。

MPKGのmanifestがELF全体のハッシュを保持する場合、ELFの署名を変更した後は、その最終ELFに対応するmanifestとパッケージ署名を作り直す必要があります。[1]

### 11.5. ビルドツールとの接続

`msign`のコマンド体系、署名の生成・検証API、自動署名の既定動作、リンカラッパーの対象、後処理との順序、失敗時の出力ファイルの扱いを決めます。

暗号検証とポリシーの意味が未確定のまま、リンカの自動実行だけを先に信頼境界として導入しません。

## 12. 検証の受入条件

正常な生成・検証だけでなく、次の挙動を確認します。

| 検証項目 | 期待する結果 |
| :-- | :-- |
| ELFのコード・ヘッダー・保護対象メタデータを変更します。 | 完全性検証に失敗します。 |
| 鍵IDや署名値を変更します。 | 鍵の対応確認または署名検証に失敗します。 |
| 改変したELFのAdHoc情報を作り直します。 | 自己整合性は成立し得ますが、署名者の信頼や権限は得られません。 |
| 有効な署名でも鍵が未知・失効済み・用途不適合です。 | 信頼済み署名者として実行認可されません。 |
| パッケージ内のELFを、同じ鍵で署名された別のELFへ差し替えます。 | パッケージの内容照合に失敗します。 |
| 外部manifestの要求Capabilityだけを変更します。 | 以前の許可で権限が自動的に増えず、再評価されます。 |
| 署名検証から認可まで、または認可からロードまでにファイルを差し替えます。 | 別のバイト列が検証済みとして実行されません。 |
| 署名を削除するか、形式を壊します。 | 元のIdentityと権限を引き継ぐ経路へフォールバックしません。 |
| 異常な長さ、重複、範囲の重なり、過大な入力を与えます。 | クラッシュや過大な資源消費を起こさず、明確に拒否します。 |

形式を確定した後は、生成側と検証側で共有できる正常系・異常系のテストベクトルを用意します。

## 13. 得られる効果と制約

最大の効果は、単体ELFをパッケージ化前から検証可能な成果物として扱い、独立した実行ポリシーへ接続できることです。鍵付き署名では、コピーや転送をしても署名情報がELFと一緒に残ります。署名処理は、ELFを出力する各言語のビルド工程から共通に利用できます。

既存MPKGの安全性が、ELF署名の追加だけで大幅に高まると位置付けるものではありません。既存の署名済みmanifestによる内容保護と実行時照合は維持し、主にパッケージ外の成果物へ適用範囲を広げます。

追加の署名構造の解析、ハッシュ計算、鍵の管理、失効処理にはコストがあります。実行速度への影響は測定が必要であり、検証を省略する理由にはしません。

本設計の境界は、「ELFの検証結果を得る」「鍵をどの用途で信頼するか判断する」「実行とCapabilityを認可する」「カーネルで強制する」を混同しないことです。

## 参考資料

現行方式の記述は、2026年9月21日に確認した以下の設計文書・ソースを根拠とします。実機動作の検証結果を示すものではありません。新方式については、本書の設計方針・安全要件・未決定事項を区別して扱います。

[1]: https://github.com/mochiOS/mochiOS/blob/master/docs/certificates.md
[2]: https://github.com/mochiOS/services/blob/main/signature/src/database.rs
[3]: https://gabi.xinuos.com/elf/03-sheader.html
[4]: https://gabi.xinuos.com/elf/07-pheader.html
[5]: https://www.rfc-editor.org/rfc/rfc8032.html

[1] mochiOS「mochiOSの証明書と署名検証」：現行のPKI、MPKG検証、Install Record、実行認可の設計です。

[2] mochiOS/services `signature/src/database.rs`：Issuerの用途と既存の`Development`分類の対応です。

[3] ELF gABI「Sections」：Section Header、`SHT_NOTE`、セクション属性の仕様です。

[4] ELF gABI「Program Loading」「Note Sections」：Program HeaderとNOTE情報の仕様です。

[5] RFC 8032「Edwards-Curve Digital Signature Algorithm (EdDSA)」：Ed25519の署名・検証と各方式の区別です。


## ローカル開発と共通実行認可の実装目標（2026-10-29）

利用者が求める動作は次の一続きの操作です。POSIX互換環境全体のgoalは保持しつつ、
この実用経路を優先して接続します。以下はこれから接続する仕様であり、完成報告ではありません。

```sh
clang main.c -o main
./main
```

コンパイル時は`clang → mochiOS linker driver → ld.lld → msign`、
実行時は`execve / posix_spawn / GUI launch → capability.service → 実行認可`へ集約します。
確認が必要な場合は信頼済みmsh内の確認、またはsecure-uiの標準GUI popupを使います。
署名検証、Capabilityの分類、要求の妥当性、許可の保存と消費をfrontendへ重複実装しません。

### ビルドと署名

- SDKの生成は利用者側が担当します。OS側からSDKを再生成しません。
  linker driverはそのSDKを入力として使い、ELFの配置・relocationは必ずld.lldに任せます。
- `-c`や`-r`の中間生成物を実行用として自動署名しません。最終的な実行ELFとDSOを
  検査し、既存の`msign elf`を呼びます。driverに暗号処理や秘密鍵管理を複製しません。
- 新しい出力を同じfilesystem上の一時ファイルへlink・署名・検証してから公開します。
  linkや署名の失敗を成功扱いせず、既存の実行可能出力を途中の生成物で置換しません。
- 単体の`main`でも要求権限が実行体と分離しないよう、Capability宣言を含むmanifest情報を
  ELFの署名対象bytesへ含めます。後から差し替えられる未署名sidecarを権限の正本にしません。
  既存のmanifest.tomlの宣言を共用し、ELF内の表現は共通parserで検証します。
- ローカル自動署名は既存の明示的なAdHocモードを利用できます。これは自己整合性の確認で、
  built-inとしての信頼やCapability許可にはなりません。鍵付き署名を選んだ処理の失敗から
  AdHocへ黙ってfallbackしません。確定済みv1形式は[msignの仕様](../tools/devkit/docs/elf-signature-v1.md)に従います。

### 共通の実行ゲート

1. 実行対象をFileHandleから捕捉し、署名・manifest・要求Capabilityを同じbytesから検証します。
2. capability.serviceが、署名者の信頼、要求権限のclass、呼出元・POSIX sessionの正当性、
   保存済み許可、今回の操作の範囲を判断します。署名があるだけでは権限を付与しません。
3. 追加の確認が必要な権限について、serviceが確認待ちtransactionを作ります。
4. 登録済みの信頼できるfrontendへ確認内容を渡します。mshはtext、GUIはpopupとして表示します。
5. 応答をtransactionに照合し、許可された同じimageだけへ一回限りの実行認可を発行します。
6. 実際のロード対象が認可時のdigestと一致することを、既存native execのcommit境界で再確認します。

署名の検証対象は変換前の元ELFです。PIEの再配置等で生成したnative containerへ元の署名が
そのまま有効だとは扱いません。検証済みsourceと依存DSOから共通preparerが導出した出力を、
serviceが最終image digestへ結び付けて認可します。未認証callerの「この変換後bytesは署名済み」
という自己申告を受け入れません。captured RAM objectは型・権限・サイズを検査し、serializedな
brokerが任意Providerへの無制限I/Oで停止しない実行形態にします。
FD番号とCLOEXEC等の表は従来どおりposix-client/runtimeが保持し、capability.serviceへ移しません。

新しい`main`へmshの広い権限やtrusted-brokerの扱いを自動継承しません。
通常のstdio等は明示的なHandle委譲で継承し、追加の名前付きCapabilityは宣言と認可に従います。
user-grantable以外の権限は確認画面の`y`で昇格できません。
起動時と実行中の追加要求は同じ認可controllerを使いますが、許可の有効範囲を区別します。
「この実行だけ」は一回のlaunch認可、「このプロセスだけ」はprocess scopeです。
単一操作にしか有効でない許可をprocess全体の権限へ拡大しません。frontendには、その要求で
実際に提供できる選択肢だけを提示します。

### 確認transactionとfrontendの責務

serviceが保持する正本には、request ID・ランダムchallenge、実行体digest、宣言権限の集合、
対象resource、認証済みrequester/session、実行世代、期限、許容する選択肢、frontendの
登録済みendpointと表示世代を含めます。wire上の自己申告PIDや`trusted=true`を認証に使いません。

mshとGUIは、共通requestを表示して共通responseを返すだけです。frontend自身はgrantを
実行せず、応答のためにcapability.serviceへ二重の認可要求を送り返す必要もありません。
serviceは確認待ち中も別要求・取消し・process終了を処理できる形にし、serializedなserverを
frontendへの同期callのまま停止させません。応答はkernelが保証したIPC sender/Handleから
認証し、別transactionへの流用、再送、古い表示世代、期限切れ、終了済みrequesterを拒否します。

### mshの信頼と入力

`/system/bin/msh`という名前やpath、継承されたpackage IDだけで確認役を信用しません。
検証済みbuilt-inの実行イメージと、そのinstance・世代に結び付いた確認channelを使います。
必要なimage measurementの取得は汎用のProcess/Handle機構として扱い、カーネルにmshや
capability.serviceの名前・ディレクトリ・UI policyを持たせません。

この基盤として`ProcessImageRead`と`ProcessImageInfo`を追加しました（phase299）。
呼出元自身、または取得済みWAIT Process Handleの、commit済みELF digestとimage revisionを
同じprocess-table lockの下で読みます。既存のProcessStatusやThreadSecurityContextのlayoutは
変更しません。未計測とSHA-256計測を区別し、終了・消滅した実行を古いdigestで認証しません。
これは実行中メモリや後からmapしたcodeの検証でも、署名者の信頼判定でもありません。
取得だけでは実行体を固定できないため、確認channelの登録・消費時には同じinstanceと世代の
再検証が必要です。既存brokerのpackage IDだけの判定を、このAPI追加だけで修正済みとは扱いません。

通常のstdin、pipe、PTYへの書込み、clipboard paste、合成InputEvent、起動引数の自動許可指定を
新しいユーザー承認の根拠にしません。確認入力は通常のcommand/stdio経路と分け、信頼された
入力元からの新しい操作だけを、その確認transactionと表示世代に結び付けます。
確認開始前にqueueへ入っていた`y`や押下中keyのrepeatを承認に使いません。
foreground/backgroundや別terminal/sessionの取り違えも拒否します。

確認表示中はアプリ出力でtrusted promptを上書き・偽装させません。実行体名や理由などの
外部文字列はtextとして扱い、terminal制御文字をescapeします。GUIにも同じ検証済み内容を
渡し、アプリ自身が作った見かけ上の許可popupを本物の応答元として扱いません。
安全な対話経路がない場合は未許可要求を明示的に失敗させ、既存の許可や明示委譲Handleだけで
実行可能な操作は継続できるようにします。

### 現在の実装との隔たり

- ネイティブclang/ld.lldのcompile・link・実行は専用fixtureで検証済みですが、通常の
  `clang main.c -o main`へのdefault driver・自動署名接続は未完了です。
- `msign elf`にはELF署名・検証があります。現在のcapability.serviceのobject execは、
  callerの既存domain内のimage replacementを認可し、そこでELF署名を検証していません。
- packageアプリのlaunch、実行中のgrant、workspace/secure-uiの各種確認が分かれています。
  mshには確認入力のIPC senderを検証せずInputEventとして処理する箇所があります。
  また、built-in package identityだけのbroker判定では、image置換後の確認役の信頼を証明できません。
- これらを共通gateと保護された確認channelへ移行する必要があります。署名や権限確認を
  省略した開発専用fallbackを最終経路として残しません。

実装の優先順は、共通transaction/subjectと保護された確認経路、captured ELFの署名・manifest検証、
exec/spawn/GUIの共通gateへの接続、SDK driverからのmsign呼出し契約の接続です。
受入試験には、通常のビルドから実行までに加え、改変ELF・未宣言権限・偽sender・stdinからの`y`・
paste/repeat・応答再送・実行体差替え・別session・取消しを含めます。成功した画面や狭いunit testだけで
この経路全体の完成とは扱いません。

### 共通確認transactionの状態管理（phase300）

`capability-protocol::consent`に、text / GUI共通のversion付きfeedback形式を追加しました。
transaction ID、32byte challenge、表示世代、提示した選択肢、独立した入力証拠のserialを使い、
返答から実行体・Capability・resource・PIDを書き換える形式にはしません。
challengeはserviceの乱数生成器から渡す必要があります。番号や自己申告senderは認証になりません。

`capability.service`が所有する`consent::Controller`は、確認待ちを最大32件保持します。
同じrendererへの確認は重ねず、待ち状態を返します。同期IPCでfrontendの応答を待つ処理、
暗号、署名検証、画面描画、Capabilityのgrantは、この状態機械へ入れていません。
確認待ちの実行体・元ELF / prepared ELF / 要求権限とresourceのdigest、session、policy世代、
renderer登録世代、input routeを固定します。応答にこれらの差し替え欄はありません。

表示完了の返答後、別processにある登録済み入力authorityからserialと押下状態のsnapshotを受けて
入力をarmします。新しいphysical pressだけを記録し、古いserial、repeat、synthetic inputは
承認証拠になりません。表示前から押されていたキーは、一度releaseされる必要があります。
frontendが自分で送った「入力証拠」や、提示されていない許可範囲を拒否します。
通常のstdinやInputEvent packetをこの証拠として受け入れるtransportは実装していません。

受理時には、service adapterが取得した現在のrequester / presenter / input instance、session、
focus、policy世代を照合します。取得失敗はcached identityへfallbackせず欠落として扱います。
実行体変更・終了・session切替等は、応答を待たずownerのobserveで取り消すこともできます。
期限はmonotonic clockで判定し、巻戻り・overflow・ID再利用を拒否します。
許可判断は外部I/Oへ戻る前に一回だけ消費され、grantや永続化が失敗しても再利用できません。
一回限りの許可をprocess全体のgrantへ拡大せず、「すべて許可」という暗黙の範囲も追加しません。

package / policy refreshは成功時だけauthorization世代を進めます。世代の枯渇はrefresh全体を
失敗させ、古い世代で新しいpolicyを公開しません。この世代も確認transactionへ固定します。

この戻り値は、ある時点での確認結果であり、kernelの許可Handleではありません。
最終的なgrant / exec authorizationは同じ対象bytesとexecution instanceへatomicに結び付ける
必要があります。ユーザー空間でrevisionを読んだ後、世代と無関係なCapTransferを呼ぶだけでは
execとの競合を防げません。

状態機械と共通feedback codecは実装・検証済みです。既存のCAPABILITY_DECISION経路、
mshのstdin確認、secure-ui popupのtransportをこのcontrollerへ置換する接続はまだ残っています。
既存brokerのpackage IDだけの信頼判定や、実際の保護入力channelを修正済みとは扱いません。

### 確認役の実行イメージ認証（phase301）

従来のCAPABILITY_DECISIONとapplication launchのbroker判定に、`TrustedImages`を接続しました。
package ID・developer・provenanceだけでは通さず、カーネルが記録した実行イメージを照合します。
IPC senderからWAIT Process Handleを取得し、image情報→security context→image情報とendpoint
identityを照合します。途中のexec、retireされたendpoint、owner不一致、未計測、範囲外のidentity、
reserved fieldや取得失敗は拒否し、PID検索・caller自身・cached contextへfallbackしません。
保持したProcess参照は全失敗経路を含めRAIIで閉じます。

信頼ルートはcapability.serviceの起動時に、不変なSystem世代の明示的なbootstrap readerから
取得します。msh / workspace / secure-ui / input / compositorのbuilt-in identity、検証済みmanifest、
宣言済みfile bindingがあればそのsize / digest、実際の元ELF bytesを確認します。
その後の認証でruntime pathを開いて信頼の正本を差し替えることはありません。
mshのmanifestも実際のSystem配置である`/system/bin/msh`へ揃えました。
これらのrole名・package名・pathはユーザー空間のポリシーであり、mnuには追加していません。

変換が不要な元イメージは、bootで捕捉したdigestと比較できます。PIE等で変換が必要な場合は
元ELFと変換後ELFを同一視しません。既存の共通preparerが全入力を検証し、native processを
作成してからresumeする前に、元manifest digest・元code digest・実際の出力digestを照合し、
そのProcess ObjectのWAIT参照とimage revisionへ認証記録を結び付けます。
単に「署名済みの変換結果」とcallerが申告する入口や、出力hashのglobalなallowlistはありません。
別processやexec後の世代はこの記録を使えません。記録は最大128件で、終了・変更済みの参照は
新規登録時に除去します。通常のアプリをこれらのroleへ登録することもありません。

KVMではbuiltin msh identityの正しいコードがbroker判定を通ることと、そのidentityを保って
別digestへexecした後は拒否されることをeuid 0 / 501で確認しました。別の試験では、実際のPIEを
共通preparerで変換して起動し、作成されたProcess参照への結合によってbrokerとして認証できました。
確認roleの判定だけを試験し、不正な短いdecision requestを使うためCapabilityのgrantは行っていません。

この認証は観測時点の実行イメージの確認であり、実行中メモリの完全性保証やprocessの固定では
ありません。legacy入力経路、同期promptの待機、Terminalからのmsh起動時のdomain設定は、
新しい共通controllerへの移行が必要です。実際の保護入力channelと最終grant時のatomicな
instance / generation結合も未完了で、偽の`y`対策全体が完成したとは扱いません。

通常構成へ戻したKVMでもinput READY、Binder起動、デスクトップ表示、Filesの表示と閉じる操作を
確認しました。Files起動時の既知のactivate TransportFailureが1件あり、再操作で表示されました。
NICなしでのNetwork Failed(-6)とUSB driver creation errno 22も残っており、今回修正した問題には
含めません。診断用mshとmanager manifestは復元済みです。全試験VMを停止し、ホスト解析処理の
再開も完了しました。

### raw入力の送信権限と合成入力の区別（phase302）

`input.service`が通常のglobal宛IPCをrawキーボード・マウス入力として受け付ける経路を廃止しました。
既存のカーネルによる`IPC_DELIVERY_ENDPOINT_HANDLE`を照合し、実際に送信Handleを所持していた
場合だけraw packetを解釈します。packet自身のflagやIDは所持の証明になりません。
長さ、予約欄、未知flag、添付Handleも検証し、不要な受信参照は失敗時を含め閉じます。

送信Handleは`capabilities.manage`を持つ起動管理側にだけ、version付きの要求で返します。
service-managerは保持したinput ProcessのIDとIPC endpointのownerを照合し、返答senderと
ownerの再読取り、権利の完全一致を確認します。名前検索だけでは正本を選びません。
そこからdriver discoveryのIPC添付としてdrivers.serviceへ渡し、検証済みlaunch capabilityに
`device.input` / `input.keyboard` / `input.pointer`があるdriverだけへ起動Handleとして委譲します。
最終driverの権利はSENDのみで、RECEIVE、TRANSFER、DUPLICATEを渡しません。
PS/2・USB driverはこの起動Handleを使い、globalなinput.service検索へのfallbackを持ちません。

PS/2 driverのシリアル入力には合成入力の印を付け、通常のkey eventにもその区別を残します。
物理portからの入力とは別に扱うための情報であり、公開InputEventのflagだけで承認を証明する
設計ではありません。既存のgeneric Handle / IPC / LaunchContextを使い、mnuへのAPI追加や
サービス名・ポリシーの追加は行っていません。旧global raw入力との互換性は維持しません。

関連ホスト6対象43件とnative component / driver buildが成功しました。KVMでは、実際の
委譲SEND Handleでraw入力が通り、global宛の通常IPCとobject添付可能IPCはいずれも
偽キーボード・マウス入力として拒否されることをeuid 0 / 501で確認しました。
権限なしのraw Handle取得も拒否され、管理側でも未知versionを拒否しました。

診断用実行体・manifest・recordを復元した通常構成でもinput READY、Binder、デスクトップ表示を
確認しました。QEMUのキーボードデバイスからPS/2 driverを通して、Filesの検索欄へ
`input-route-302`を入力でき、マウス操作とウィンドウを閉じる操作も確認しました。
一方、Terminalのmsh起動はPermission denied (13)になりました。Terminalでのコマンド実行の
成功とは扱わず、起動・認可接続の未完了として残します。初回activate TransportFailure、
NICなしのNetwork Failed(-6)、USB driver作成errno 22も未修正です。
全検証VMを停止し、ホスト解析処理の再開処理も完了しました。

この変更だけで承認経路全体は完成しません。保護された確認中の入力route・新しい操作の証拠、
focus / sessionの結合、msh / GUIと共通controllerの非同期接続、最終grantのatomicな結合は
引き続き必要です。既存mshの通常stdin確認を安全な承認へ変更済みとは扱いません。

## Terminalからの通常execを復旧（phase303）

Terminalのmsh起動先を、インストール先と同じ`/system/bin/msh`へ修正しました。
さらに、Terminalとmshのmanifestへ`system.random.read`を明示しました。
現在のnewlib runtimeはexecの起動情報を作る際に`RandomFill`で`AT_RANDOM`を用意するため、
`process.spawn`だけでは不足します。両者のbuilt-in verification recordも通常の
`msign package built-in-record`で再生成して検証しました。

実際のTerminalで診断すると、子processの生成・対象ファイルの取得・ELF準備は成功し、
その後の起動情報作成で`EACCES`になっていました。リソース制限ページを取得する前の
乱数取得で拒否されており、宣言を加えた通常版ではmshのpromptまで到達しました。

通常版KVMで次を入力し、画面上の出力を確認しました。

```sh
echo shell-route-303
pwd
cd /system/tests/shell-launch-303
./main shell-303
```

検証用Cプログラムは、`uid=1000`、上記cwd、`argv=shell-303`を標準出力へ返し、
標準エラーへの出力も同じTerminalに表示しました。
ソースは`services/posix/tests/native_shell_stdio.c`です。このELFはホストで作ったもので、
通常のmochiOS上のclangや自動署名の検証ではありません。
証跡は`out/checks/mochios-shell-launch-303-*`に保存しています。

現在のPOSIX execは呼出元のsecurity domainを継承します。そのため、この起動成功だけで
Terminalから起動したmshがtrusted prompt brokerになったとは扱いません。
検証済みmshのidentityとstdioを共通launcherで渡す処理、保護された確認channel、
署名・manifestを含む実行認可、通常のclangから既存msignへの接続が引き続き必要です。


## ローカルFDから明示委譲用Handleを取り出す（phase304）

`mochi/handle.h`の`mochi_fd_export(fd, requested_rights, result)`を追加しました。
自プロセスのFDを取得中だけ固定し、独立して所有するnative Handleと共通の
inherited-FD metadataを返します。sourceの権限とFDのaccess modeを超える要求は拒否し、
metadataのaccess modeも縮小します。descriptor-localなCLOEXECはmetadataへ保持します。
元のFDを閉じても取得した参照は維持され、取得した参照はHandleCloseで破棄します。

出力は24 bytesのC ABIです。reservedは0、metadataは既存のPOSIX user-space codecを使います。
書込み先をRust referenceとして借用せず、既存のMemoryCopyでcopyoutします。
sourceのFD leaseと新しい参照のownerをcallback scopeへ保持し、copyout失敗時は新しい参照を
回収します。任意のPIDやforeign FD、pathnameからの取得は提供しません。

ホストでは新しいexportと既存importの7 testsが合格しました。
通常版KVMでは`services/posix/tests/native_fd_handle_export.c`をTerminalから実行し、
権限追加の拒否、書込み不可メモリへのEFAULT、native rightsの一致、write拒否、
元FDを閉じた後のstream読み取りを確認しました。最初の試験はraw native errorを
POSIXの-13と誤比較して失敗したため、mnuのtag付きAccessDeniedとの比較へ修正して再試験しました。
診断結果と画像は`out/checks/mochios-fd-export-304-*`に保存しています。

これはstdioを明示的なIPC Handleとして共通launcherへ渡すための部品です。
launcherへの接続、trusted mshのidentity、確認用入力channelとGUI popupは引き続き必要です。
カーネルのABIや権限チェック、利用者管理のSDK生成処理には変更を加えていません。

## 検証済みmshと明示的なstdioを共通launcherへ接続（phase305）

Terminalのmsh起動を、親のsecurity domainを継承するPOSIX spawnから、
capability.serviceの検証済みapplication launcherへ移しました。
既存GUI起動と新しいstdio付き起動は、manifest認証・capability policy・入力ELFのcapture・
native Process作成・POSIX child登録・resumeを共用します。
単体コマンドの既存binary packageも扱い、選択binaryのapplication種別とcode検証は維持します。

共通のversion付きwire protocolはfull argvとenvpを分離し、空の引数を保持します。
自プロセスから取得したread scopeとstdio 3本だけをIPC Handleとして添付します。
Handle番号やPIDを文字列で渡すことはせず、FD metadataから権限を追加しません。
明示したstdio mappingはdup2相当として子側CLOEXECを解除します。
24 bytesのFD export構造体はPOSIX protocol crateへ共有し、C ABIは変更していません。

brokerは認証済みPOSIX endpointを自分の保持参照から複製して子へ渡します。
service-managerはこの委譲に必要なSEND / DUPLICATE / TRANSFERをbrokerへ渡し、
他の既存runtime readerはSENDだけのままです。
クライアントの返信照合にはkernelが保証するthread endpoint senderを使い、
一般アプリへprocess.inspectを追加しません。Terminal自身の乱数権限も不要になりました。

測定済みmshを有効にする前に、旧CAPABILITY_DECISIONによる即時grantと永続化を廃止しました。
通常stdinのyや、transactionと新しい物理入力に結合されていないscalar decisionでは許可しません。
旧decision opcodeはUnsupported（wire 95）を返します。
新しい非同期consent controllerに接続するまで、新規承認が必要な起動は許可されません。

ホストでは関連7対象25件と回帰6対象74件が合格しました。
KVMでは、実際に起動したmshのbuilt-in identityと測定済みimageによるTextPrompt認証、
旧形式の正のdecisionの拒否、mshから別ELFへexecした後のrole拒否を確認しました。
package IDがorg.mochios.mshのままでも、別の実行コードをmshとして信頼しません。
TerminalからC fixtureを実行し、uid 1000・cwd・argv・stdout・stderrも確認しました。
このC実行体はホストで作ったもので、guest clangによるコンパイルの検証ではありません。
証跡はout/checks/mochios-verified-launch-305-*です。

最初の実機試験ではクライアントがProcessInspect専用の照会を使い、次の試験では
loaderが単体コマンドのpackage形式を拒否しました。いずれも原因を修正して再試験しました。
これらの失敗を成功として扱っていません。

診断traceとrole-queryを除外した通常構成でも、デスクトップ・Terminal・msh prompt、
同じC実行体の実行、存在しないコマンドのENOENT後にechoを続けられることを確認しました。
検証用ELFは最終stageから除去し、通常イメージを再生成して6件のchecksumを確認しました。
利用者管理のSDK 6ファイルのhashは変更されていません。全検証VMは停止し、
解析処理の再開処理も完了しています。初回activate TransportFailure、NICなしのNetwork失敗、
USB driver作成errno 22は今回の修正対象ではありません。

これは通常clangからの自動署名や、安全な対話承認までの完成を意味しません。
guest clangから既存msignへの接続、単体ELFの署名・manifestを含む共通実行認可、
保護された入力・focus / sessionとmsh / GUI popupの非同期接続、最終grantとのatomicな結合は
引き続き必要です。mnuのABIやサービス固有の知識は追加していません。

## 入力authorityから共通controllerへの専用通知（phase306）

input.serviceへ、公開InputEventとは別のconsent入力経路を追加しました。
version付きの登録・arm・cancel・status要求と入力通知に、要求のchallenge・表示世代・
入力経路の世代・単調増加serialを結合します。自己申告のPIDや信頼flagはありません。
入力側はkernelのcapabilities.manage判定を経たIPC senderをownerとし、
別の管理processでも既存ownerの経路を横取りできません。

受け付けたdriver packetからキーとボタンの押下状態を追跡します。
合成入力には別のprefix decoderを使い、合成のreleaseで物理キーを解放しません。
観測済みの押しっぱなしはRepeatとし、複数buttonを一つのreportで交換した際にも
途中の「すべて解放」を作りません。確認表示後のarmは現在のserialと押下状態を返します。
input.serviceからbrokerへの通知送信が失敗した経路は破棄し、その後の入力で自動再開しません。
消費側は最終判断前にもstatusで経路の生存と未処理通知を確認する必要があります。

service-managerはinputの起動完了後にcapability.serviceへ接続を依頼します。
brokerはbootstrapの信頼ルートに対してinputのpackage identityと測定済みcodeを照合し、
実際のProcess Handleを保持します。返信のnative senderと前後の実行instanceを照合し、
通知受信時にも同じnative object・image revision・digestであることを再確認します。
異なるsender、古い経路・要求・表示、重複または飛んだserialを承認証拠として受け付けません。
mnuへの専用API追加やinput.serviceへのprocess.inspect権限追加はありません。

ホスト7対象59件と回帰7対象47件が合格しました。
KVMでは実際のmanager・input・brokerを使い、偽senderの入力通知と、もう一つの
capabilities.manage processからの登録・armを拒否しました。
正規driver入口から送った合成yではcontrollerが承認せず、その後QEMUのkeyboard deviceから
送った物理yのpress / releaseと生きた経路の最終snapshotでのみcontrollerの試験が通りました。
試験中のpresenterとsessionの観測は診断fixtureです。Capabilityの付与・永続化・起動は行わず、
実際の確認UIやsession / focusの検証まで完了したとは扱いません。
証跡はout/checks/mochios-consent-input-306-*です。

診断collectorを除いた通常版でもdesktop、DockからのTerminal起動、文字入力、
echo / pwd、ENOENT後の継続操作を確認しました。閉じる初回操作は反映されませんでしたが、
その後もTerminalのコマンド実行は応答し、再操作では閉じられました。
初回不達の原因は特定しておらず、修正済みとは扱いません。
通常版3実行体のstage hash、image hashと6件のchecksumを照合しました。
SDK 6ファイルとkernel commitは未変更で、全検証VMを停止し、解析処理の復帰も完了しています。

この経路は既存の通常入力を置き換えず、通常版では新規承認が必要な操作をまだ許可しません。
実際の表示完了・session / focus・入力の専有route、device初期状態・reset / hotplugと
複数deviceの整合性、mshとGUIのfrontend、実行体に結合した最終grantへの接続が残っています。
通常clangから既存msignへの接続と、単体ELFの署名・manifestを含む共通実行認可も未完了です。

## compositorによる表示確認と実セッションの失効（phase307）

表示済みというfrontendの自己申告を、承認の根拠にしないための共通経路を追加しました。
brokerは、起動時の信頼ルートと照合したrendererのProcess参照をWAITだけへ縮小し、
IPCに添付してcompositorへ登録します。要求にはchallenge・表示ID・session epochを結合します。
IPC endpointと実行imageの整合した捕捉はplatformへ共通化し、既存のbrokerも利用します。
カーネルへサービス名やconsent専用APIは追加していません。

ViewKitのAppは、brokerが発行した要求をウィンドウの最初の描画前に結び付けられます。
その値だけでは表示を証明できません。compositorは実際のsurface所有者、同一native Process、
image revision / digest、surface role、画面内の可視性、他surfaceによる遮蔽、keyboard focusを照合します。
登録後の新しいcontentを含む全画面presentが成功した場合だけPresentedを返します。
commitへの応答、古いbuffer、失敗したpresent、部分presentだけでは成立しません。
focusが外れた場合は、同じsurfaceへ戻っても古い登録を復活させません。
表面の消滅、所有者・image・content世代の不整合でも失効します。

session epochはservice-managerの実際のログイン・lock・logout遷移から公開します。
brokerは送信者の測定済みcode、built-in identity、保持済みProcessと照合します。
失敗した送信のepochも再利用せず、世代の巻き戻しや同世代の再有効化を拒否します。
session更新では、保留中のcontrollerと入力観測を先に失効させ、compositorの登録も破棄します。
環境変数からsessionの認証情報を採用しません。

ホストの関連7対象47件と回帰7対象47件が合格しました。
native試験では実際のViewKit renderer・broker・compositor・managerを使用しました。
新しいframeの表示と実Processの照合、他processによるbindとsession上書きの拒否を確認しました。
QEMUのmouse deviceからCloseを押してsurfaceを削除すると失効し、実session epochを
無効化・再有効化した後も元の登録はENOENTでした。Capabilityの付与・永続化は試験していません。
診断UIは表示経路の試験であり、製品のpermission popupを完成させたものではありません。
証跡はout/checks/mochios-presentation-307-*です。

初回のVMはloginで停止しました。新しくbuildしたsecure-uiに対し、古いstage manifestが
既存のprocess.inspect要求を欠いていたためです。source manifestと既存msignのbuilt-in recordを
stageへ反映して再試験しました。認証を省略するfallbackは追加していません。

診断collectorを除いた通常版でもdesktop・Dock・Terminalの実表示、文字入力、echo / pwd、
ENOENT後の継続操作を確認しました。閉じる初回操作の不達は今回も再現し、再操作では閉じました。
初回activateのTransportFailure、Network Failed(-6)、USB driver errno 22とともに未解決です。
通常版4実行体のstage hashとimage checksumを照合し、SDK 6ファイルとkernel commitの
未変更を確認しました。全試験VMを停止し、解析処理の再開処理も完了しました。

この段階はグローバルなsessionの生存確認です。requesterがそのsessionに所属するという
認証、mshと実際のTerminal rendererの結合、既存Terminal内の表示領域の検証は未完了です。
Window形式では遮蔽判定が保守的で、自身のdecoration等も遮蔽として扱う場合があります。
入力deviceの初期状態・reset / hotplug・複数deviceの整合性、実frontendとの非同期調停、
captured codeへatomicに結合する最終grantも残っています。
通常版では、これらが完了するまで新しい対話承認によるgrantを有効にしません。
通常clangからld.lld・既存msignへの接続と、単体ELFの署名・manifest検証も引き続き必要です。

## 実プロセスのdesktop所属を明示Handleで認証（phase308）

POSIXのparent / SIDは、孤児回収やsetsidで変わるためdesktop所属の認証には使用しません。
posix.serviceへ、明示的なnative objectで保持・委譲するprocess domainを追加しました。
作成・所属照会は空payloadとHandle添付、起動前の登録は既存形式のPOSIX credentialsと
domain / Process Handle添付で行います。クライアントが選んだPIDやHandle番号をpayloadに載せません。
native Process objectの型・WAIT権限・生存・保持済みobjectとの同一性を照合します。
protocolはno_stdのposix-protocol、transportはposix-clientに置き、mnuは変更していません。

domain所有者だけが新しいrootを登録できます。service-managerはsessionごとにdomainを作り、
workspace / Binder / LinuxのPOSIX stateと所属を登録した後でnative processをresumeします。
これにより、最初のforkやdouble-forkが事後の所属捕捉に先行する隙間を作りません。
forkとbroker経由の新規起動は、実際のPOSIX requesterの所属をサービス内で継承します。
execや親のreparent / reapでは所属を変更しません。

全祖先processの記録を残す実装ではなく、domain所有の共有markerとprocess側のweak参照を使います。
登録数は全体256・所有者ごと16へ制限し、nativeの最終参照解放だけでdomainを破棄します。
破棄されたmarkerは後続forkへ継承しません。object IDは失敗した公開分も再利用せず、
read-only path scopeとは別の正のID範囲を使います。

managerは測定済みsession ownerとして、epochと一緒にdomainをTRANSFERだけへ縮小してbrokerへ渡します。
brokerはdomain objectの有効性を実POSIX endpointで照合してから公開を確定します。
所属照会では、保持したsession ownerとrequesterの実行instanceを再認証し、
requesterの実Process参照とdomainだけをPOSIXへ渡します。MOCHI_SESSION_IDなどの環境変数は
認証情報にしません。session無効化や公開失敗では、古いdomainで継続しません。

ホストでは、親の終了・reparent・waitによる回収・exec後の所属継承、公開失敗時の原子性、
所有者の制限・quota・native release、Handleの型と権限縮小、借用元を閉じない失敗処理、
owner / requesterの消滅・image変更とsession公開失敗を確認しました。
実IPC試験では実session rootの所属、manager自身の非所属、異種objectと自己申告sessionの拒否、
実際のTerminalから起動した測定済みmshのdesktop所属認証が通りました。
この試験でCapabilityの付与・永続化は行っていません。
証跡はout/checks/mochios-process-domain-308-*です。

診断コードを除いた通常版でもdesktop、Dock、Terminalと文字入力を確認しました。
`/system/bin/ls /home`は実行され、root / testuserを表示しました。
裸のlsはmshが/bin/lsを選んでENOENTとなる不整合が残っています。
また外部コマンドの完了前に複数行を連続注入した試験では、後続入力が混ざりました。
プロンプト復帰後の単独echoは成功しました。入力の競合原因を特定したとは扱いません。
通常のコマンド経路全般やclangの利用体験の完成を、この確認から主張しません。

関連・回帰のホスト10対象555件が合格しています。kernel commitは未変更です。
本作業からSDK生成は実行していません。通常版build時のSDK 6ファイルは開始時と一致しましたが、
最終照合では5ファイルの更新を検出しました（主な更新時刻は2026-10-31 17:16 JST）。
開始時と最終観測時のhashを別々に記録し、更新後のSDKを巻き戻していません。
検証済みイメージは更新前の入力で生成したものです。更新後SDKでの再build・再試験は未実施です。
今回の変更は機能ごとに署名付きコミットへ保存し、残ったlock更新も保存しました。
編集中の別のUIソースは含めていません。
初回activateのTransportFailureと閉じる初回操作の不達は残り、再操作では閉じられました。
通常版4実行体とイメージのhashを照合し、全試験VMを停止して解析処理の復帰も完了しました。

所属は「この確認要求がそのdesktopから来た」ことの一条件です。
mshと表示担当Terminalの組み合わせの保持、保護された既存ウィンドウ内の表示領域と入力の結合、
非同期frontend、device状態の整合性、captured imageに結合した最終grantは引き続き必要です。
通常clangの自動署名と単体ELF検証を含む開発経路、および完全なPOSIX互換環境のgoalは未完了です。
