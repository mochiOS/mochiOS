# プロセス所有のレコードロック

## 現在の実装範囲

`services/posix/src/record_lock.rs`はPOSIXサービス内部の区間ロック管理です。
読み取り共有・書き込み競合、所有者自身の範囲置換、分割・結合・解放を
提供します。カーネルにPOSIXのロック状態を追加しません。

`posix-protocol::record_lock`に要求・応答の固定長codecを追加しています。
Query/Set/Unlock/ReleaseFileとSEEK_SET/CUR/END相当の起点を表し、
要求にはPID・FD・パス・device/inode・ポインタを含めません。
競合する所有者のIDは応答でのみ返します。version・予約領域・長さ・列挙値を
厳密に検査し、ReleaseFileに範囲や種類を載せる形式も拒否します。
`OP_RECORD_LOCK`と`Client::record_lock`で実FileHandleのIPCへ接続しています。

サービス内部の`OwnedLocks::apply`で、decode済みの要求を既存のロック状態へ
接続しています。CUR/ENDは呼び出し側が添付FileHandleから観測した位置・サイズで
解決し、SETは基点0とします。競合はEAGAIN、上限はENOLCK、範囲超過は
EOVERFLOWへ変換します。照会は絶対範囲、または競合なしの応答を返します。
この内部API自体はHandleの認証やアクセスモードの検査を代行しません。

**libcのfcntl、FDのclose時の自動解放、待機処理には未接続です。**
F_GETLK・F_SETLK・F_SETLKWが利用可能になったとは扱いません。

## 所有者とファイルの識別

所有者はサービスが登録したプロセスです。FD番号やopen-file descriptionを
ロックの所有者にしません。UIDが0でも他の所有者のロックを書き換えません。
ファイルはdevice/inodeの組で識別し、別のマウントやinodeと混同しません。
この組自体は権限を与える値ではありません。

`record_lock_request`は、明示的に添付されたFileHandleを検証し、信頼する
filesystem側のfstatから識別と通常ファイルであることを確認します。
読み書き権限は、open時に発行され、IPCでも増幅できないHandleのREAD/WRITEで
判定します。Set(Read)はREAD、Set(Write)はWRITE、それ以外はTRANSFERだけを
要求します。CURのみ追加でSEEKを要求し、位置を変えないネイティブseekで
現在位置を観測します。write-onlyのFileHandleにも読み取り権限を要求しません。
要求内のパス・FD・device/inode・PIDを認証や対象選択に使ってはいけません。
観測はPOSIX状態のmutex外で行い、ネイティブの要求イメージを再検証してから
ロック状態を更新します。失敗時に別providerの権限へ迂回しません。
既存のGET_FLAGS_HANDLEはAPPEND/NONBLOCKのみを返すため、読み書きの
アクセスモードを取得できるAPIとして流用してはいけません。

最初のロック取得時に、IPCで受信した参照一つをサービスの所有へ移します。
要求ごとの複製やDUPLICATE/CONTROLの追加委譲は不要です。同じファイルに
別の所有者がロックを追加しても参照は増やさず、最後の所有者の解放・終了確認後に
破棄します。unlink後のinodeも参照が支えます。ロックの上限を超えるファイル参照を
無制限に保持せず、参照記録と応答領域の確保はロック変更前に済ませます。

## 区間と資源管理

符号付き64ビットoff_tの範囲で半開区間を保持します。SEEK_SET/CUR/ENDの
基点はアダプターが取得する値とし、相対開始位置と負の長さを検査付きで
正規化します。長さ0は現在のファイルサイズでは切らず、後の拡張も含めます。
最大オフセットまでの範囲は長さ0へ正規化できます。

別プロセスのread/readは共有でき、writeとの重なりは競合です。同じ所有者の
既存範囲は必要に応じて分割・モード変更し、同じモードの隣接範囲は結合します。
競合の照会では自分のロックを除外し、競合する所有者・種類・範囲を返します。

管理上限はプロセスごとに128区間、全体で4096区間です。置換は別領域で準備し、
正規化後に上限を確認してから確定します。競合・割り当て失敗・上限超過では
既存状態を保持します。レコードを増やさない解放やファイル全体の解放は
割り当てを必要としません。

## プロセスとFDの寿命

サービスの終了観測とreapへ解放を接続しています。親のない所有者でも
ネイティブ終了イベントの監視対象に入り、終了確認後は親への通知の成否と
切り離して解放します。ネイティブ終了が不明な場合は解放しません。

execのイメージ状態初期化ではプロセス所有のロックを消しません。
[forkでは親のプロセス所有ロックを継承しない規定](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html)
に合わせ、別のプロセスIDへ自動コピーする経路は作りません。
実forkで親のロックが子へ移らず、子の終了で親のロックを消さないことを
ネイティブ試験で確認しています。CLOEXECを含む実execはFD側の接続後に検証します。

同じファイルを指すFDを一つでもcloseした場合、そのプロセスの全範囲を
解放するための`release_file`を用意しています。通常のcloseだけでなく、
dup2による置換・CLOEXECによるcloseにも接続する必要があります。
これはまだFDランタイムから呼ばれていません。

## 残る接続

- fcntlの第3引数がc_intのため、ポインタを切り詰めないABIへ変更します。
- flockのl_pidがshortのため、mochiOSのpid_tを表せるソースABIへ揃えます。
- アプリのflockを検査付きで入出力し、errnoと権限縮小付きFileHandle IPCへ接続します。
- close・dup2・exec時の解放を、並行したロック取得と整合させます。
- F_SETLKWの待機・中断・キャンセル・待機循環の検出を追加します。
- 終了走査で、一つの所有者の観測・通知エラーが後続所有者の解放を遅らせる
  経路を分離します。現状の終了走査は最初のエラーで戻る箇所があります。

ホスト試験は`services/posix/tests/support/record_lock.rs`です。区間の仕様例に
加え、4,000回の操作をバイト単位の独立モデルと照合し、所有者別・全体の
上限と失敗時の状態維持、exec後の保持、終了確認と通知失敗の区別を検証します。
`services/posix/tests/record_lock_wire.rs`でcodecの破損入力・符号付き極値・
不正応答と、短い出力領域での非破壊性も確認します。内部アダプターの試験は
要求のencode/decodeから状態更新と応答のencode/decodeまでを通します。
`record_lock_client.rs`では要求ごとの権限縮小と不正応答の拒否・不要Handleの破棄を、
`support/record_lock_request.rs`では参照の所有移譲・最後の解放・終了時の解放と
メタデータ観測やイメージ検証の失敗時の状態維持を確認します。

`native_record_lock.rs`は実KVM上で、権限縮小したwrite-onlyのFileHandle、
クライアントの事前検査を迂回した不正要求、別openした子プロセスとの競合、
他プロセスの解放不能、終了時の解放、rename/unlink後の保持、CURの非破壊的な
観測と負の長さ、別openからの全解放を確認します。これはFileHandle APIの試験で、
libcのF_GETLK/F_SETLK/F_SETLKWが動作するという証拠にはしません。
