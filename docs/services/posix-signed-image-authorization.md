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
