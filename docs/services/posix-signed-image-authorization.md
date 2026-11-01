# 単体ELFの署名検証と実行認可

## 共通検証の資源上限と捕捉内容の計測（322）

`user/crates/elf-signature`の`verify_with_limits`は、ELF全体の長さ、section / program
header数、NOTEとsection名の累積走査量を検査します。同じfile領域を複数headerが参照する
場合も毎回の走査を加算します。section名は上限以内のsliceでNULを探して長さを保存し、
後続処理で同じ名前を走査し直しません。headerとNOTE参照の確保はfallibleにし、署名NOTEは
許されるSHT_NOTE / PT_NOTEの同一領域参照2件を超えると拒否します。

既存の`verify`と署名形式v1は維持します。上限は呼出側の処理予算であり、署名形式の変更や
上限超過時の未署名fallbackではありません。既定値でも累積走査量を制限し、broker等は
明示的により小さな上限を選べます。96 bytesの署名messageは固定長配列です。

`verify_image`は検証した所有側snapshotを借用する`VerifiedImage`を返します。
署名の正規化digestとは別に、署名欄も含む元ELF全bytesのSHA-256を保持します。
native image replacementへ結び付けるのは後者です。snapshotの変更後には再検証が必要で、
安全なRustの借用中に元bytesを変更・差し替えできません。

`SuppliedPublicKey`は呼出側が明示した公開鍵をkey_idと照合するresolverです。
署名の整合性を検証するだけで、開発者の信頼、IdentityやCapabilityは付与しません。
実行認可、manifestと保護されたCLI / GUI承認への接続は引き続き必要です。

`services/posix/tests/elf_signature.rs`の14件は、既存v1の改変・重複・algorithm・flagsの
拒否に加え、正規化digestと全file計測の区別、非LOAD byteの改変、鍵なし / 不一致鍵での
拒否、走査予算のちょうど境界と超過、重複した長い名前、重なるNOTE領域を確認します。
ホスト記録は`out/checks/mochios-signed-image-322-host.log`、mochiOS向けno_std buildは
同接頭辞の`native-build.log`です。SDKとRust std overlayは再生成していません。

共通codecを利用するmsignの既定構成34件も成功しました。localhostの試験用HTTPサーバーを
必要とする4件はsandbox内ではbindを拒否されたため、同じ試験を許可された環境で再実行して
確認しました。同接頭辞の`msign-default.log`に記録しています。

## 明示委譲されたRAM内ELFの検証と認可（323）

`capability-protocol::signed_image`とcapability.serviceの同名moduleを接続しました。
version付き要求は元ELFの長さ、任意の検証用公開鍵、不透明な一回限りの照合keyだけを持ち、
FileHandleはIPCへ明示的に添付します。PID、FD、自己申告digest、Identityの上書きや
追加Capabilityの一覧は受け取りません。要求の長さ、version、予約領域とkeyの有無を検査します。

brokerはkernelが保証したsenderのprocess.spawnを確認し、user deliveryのちょうど1本、
READ | MAPだけを持つHandleを受理します。既存MemoryObjectLengthはnative RAM backing
だけに成功し、ProviderとProvider-backed VM objectを所有者への問い合わせなしで拒否します。
これを最初のreadより前に呼ぶため、直列brokerが任意Providerの応答待ちに入ることはありません。
カーネルへ署名形式、サービス名やディレクトリの知識は追加していません。

元ELFは既存native上限256MiB以内の私有Vecへfallibleに確保し、最大256KiBずつoffset指定で
捕捉します。早いEOF、返却長の超過、末尾への追加、長さの変更とnative errorを拒否します。
headerは各4096件、NOTEと名前の累積走査は各1MiBを上限として共通verify_imageを呼びます。
鍵付き署名は明示された一致鍵を必要とし、鍵を指定したAdHocも拒否します。

成功後は既存のimage-replace-inherit認可へ、brokerが測定した元file全体のSHA-256を渡します。
認可のIdentityと通常Capabilityは対象の現在のsecurity domainから取得し、追加のgrantや
native kernel authorityは与えません。公開鍵は整合性検証用で、開発者としての信頼にはしません。
元RAMへのwriterが残っていても、native replacementが実際に捕捉したbytesと全体digestを
再照合するため、承認後の差し替えを実行できません。受信参照は成否とも返信前に閉じます。

ホストの実broker処理・wire・codecと既存認可の25件、brokerとnative probeのmochiOS向けbuildが
成功しました。通常DockのTerminal / mshからnative_signed_imageを実行し、次を確認しました。

- 署名付きRAMの認可、未署名・非LOAD byte改変の拒否。
- 鍵なし・不一致鍵の拒否、一致鍵での整合性検証。
- 添付権限・長さの不一致拒否、応答serverのないProviderのowner I/Oなし拒否。
- 認可を使う実際のnative image replacementと終了コード37。
- 検証後の非LOAD byte書き換え拒否、元へ戻した後の認可再利用拒否。
- 全試験後の通常msh prompt復帰。

証跡はout/checks/mochios-signed-image-323-のhost.log、native-build.log、probe-build.log、
artifacts.jsonとresult.pngです。検証用のexit ELFはnative ABIからProcessExit番号を得る最小
実行体で、clangで一般アプリを生成する321の試験とは別の実行境界試験です。

これは共通の署名検証付き認可境界を追加した段階です。従来の同一security domain向け
AUTHORIZE_EXEC_OBJECTはまだ存在し、通常のlibc execすべてを新経路へ移行したものではありません。
ELF manifest、信頼方針、変換後imageへの接続、保護されたCLI / GUI同意と最終grantの結合、
完全POSIX環境のgoalは引き続き必要です。署名失敗後に旧経路で再認可するfallbackは追加していません。

protocolのno_std / allocなしcheckも成功しました。検証VMは記録したPIDと開始時刻を照合して
停止し、解析処理の復帰処理も終えました。一時probeだけを撤去して通常イメージを再生成し、
更新済みcapability.serviceと検証record、既存アプリ・SDK・toolの計400ファイルと4つの
command symlinkをimageから読み戻しました。probeの不在、配布物6件のchecksumと供給SDKの
入力hash不変も確認しています。記録は同接頭辞のnormal-image.logとreadback.jsonです。

## 静的ELFも共通preparerの捕捉を通す（324）

libc execの静的ELFだけが共通preparerを迂回し、元FileHandleの再読込でdigestを計算する
経路を廃止しました。execveとfexecveはともに、callback-abandonmentの資源管理へ登録済みの
Preparationへ元ELFを一度捕捉し、そのsnapshotで形式を分類してRoot検証hookを呼びます。
分類のために元ファイルを先読みする経路も使いません。

静的ELFはheaderや非LOAD部分を変更せず、同じsnapshotを私有RAMへ書き出します。
writer / resize参照は認可前に閉じ、既存のREAD | TRANSFER参照だけを残します。
動的な出力と同じPrepared APIで元sourceと実行用bytesを借用でき、元sourceも所有側の
Preparationが保持します。空の入力はENOEXECです。

exec_handleはPrepared.bytesから直接SHA-256を計算します。元FileHandleやRAM出力の再読込は
不要になり、認可とnative replacementが同じ内容を使います。捕捉は最大256KiBのoffset読込、
私有RAMへの公開も最大256KiBの部分write対応とし、元ファイルの共有cursorは変更しません。
失敗やcallbackによる中断では、登録済みownerがbufferと一時Handleを回収します。

これは通常execの捕捉境界を揃えた変更です。この段階のRuntimeのRoot検証hookはまだ同一
security domainの入力を受理する方針で、323の署名検証付き認可への自動接続や、package /
standaloneの認証方針を完成させたものではありません。署名検証が必要なsourceを捕捉後に
差し替えられない前提を用意し、次の接続でこの同じsourceを検証対象にします。

ホスト44件で静的・PIE・共有library・interpreter・TLS・IFUNC・COPY・RELROと、元FDの
権限 / startup metadata / fault-safe pointer検査を確認しました。静的sourceの検証hookで
元ファイルをゼロに書き換えても、元snapshotをそのまま公開すること、検証拒否時は出力objectを
作らないこと、単一ownerの再利用拒否と各失敗時の回収も検査しています。

native_fexecveも通常Terminal / mshから実行しました。不正なFD・mode・pointer・形式の拒否後に
元imageとdescriptorが維持されること、unlink済みELF、入れ子のfork / wait、argv / env、
CLOEXEC、共有offset、signalのmask / pending / disposition、record lock、pathname execが
成功しました。325の更新カーネルでも同じ全試験とprompt復帰を確認しています。

最初のC probeは私のビルド指定に__mochios__がなく、SDKのgeneric signal-set macroとruntimeの
signal bit規約が異なったため失敗しました。SDKやsignal serviceの障害としては扱いません。
対象ABIを欠くビルドをcompile errorにし、pendingがexec前に成立していることも試験してから、
正しい対象指定で再ビルドして成功しています。最初の失敗記録もartifacts.jsonへ残しています。

clangの回帰試験は、私有RAM作成の既定上限16MiBに達してENOMEMとなりました。
execだけの迂回経路を戻さず、[汎用RAM容量の設定](posix-native-memory-capacity.md)を修正しています。
証跡はout/checks/mochios-exec-capture-324-のhost.log、runtime-build.log、native-build.log、
artifacts.jsonと、mochios-memory-object-capacity-325-fexecve.pngです。

容量修正後、SDKやtargetの追加指定なしに通常PATHのclangで全gateが成功しました。
target取得14,162ms、compile / link / 自動署名 / msign verify 101,028ms、全試験206,316msです。
生成した./mainの正確なstdoutと終了値37、compile / link失敗後の旧ELFのbyte一致、
msignによる改変拒否、cleanupと通常msh prompt復帰を確認しています。
画面はout/checks/mochios-memory-object-capacity-325-clang-complete.pngです。
この時間は同KVM構成での観測で、Linuxとの性能比は測定していません。

VM停止と解析処理の復帰後、今回の4つのprobeだけを撤去し、更新済みの通常mshとkernelを
保持して通常イメージを再生成しました。既存実行体・SDK・toolの401ファイルと4つの
command symlinkをimageから読み戻して照合しました。probeの不在、配布物6件のchecksumと
供給SDKの入力hash不変も確認しています。記録は324のnormal-image.logとreadback.jsonです。
SDKとRust std overlayは再生成していません。

通常execへの署名検証の強制、ELF manifest / packageの認証、保護された同意frontendと
最終grantの結合は引き続き必要です。msign verifyの改変拒否を、通常execの認証方針が
完成した証拠にはしません。完全POSIX環境のgoalも継続します。

権限を絞った実行体がPOSIX endpointを利用するための
[委譲EndpointだけによるIPC](posix-delegated-ipc.md)は、328で実装・KVM検証しています。
