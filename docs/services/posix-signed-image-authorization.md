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
