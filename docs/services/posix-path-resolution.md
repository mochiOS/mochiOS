# シンボリックリンク先のディレクトリ条件

シンボリックリンクの内容が`file/`だった場合、従来の探索ではパーサが最後の
区切りを落とすため、通常ファイル`file`としてopen・stat・truncateできていました。
リンクの内容が`missing/`なら、O_CREATで通常ファイル`missing`を作る経路もありました。

展開したリンク先の末尾が`/`なら、解決待ちのコンポーネント列へ内部の
`DirectoryEnd`条件を挿入します。その位置で現在のinodeがディレクトリでなければ
ENOTDIRとし、元のpathnameに残る後続成分の前で条件を満たす必要があります。
`dir/`を指すリンクに`/child`を続けた場合、child自身をディレクトリへ制限しません。
存在しないリンク先では未解決の条件が残るので、通常ファイル作成へ進みません。

この条件は追加の`.`検索とは区別します。最後のディレクトリに不要なsearch権限を
要求せず、親inode・名前の対応も消しません。実際の名前探索では従来通りsearch権限、
ルートへの閉じ込め、マウントの仮想親、非公開エントリを検査します。
条件自体を名前探索の回数として数えません。

[POSIXのpathname resolution](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap04.html)
では、末尾の区切りに先行する成分は、リンクを展開した結果としてディレクトリに
解決される必要があります。unlink・renameなど最終リンク自体を操作する経路は、
従来のnofollow指定を保ち、リンク先のinodeを削除・置換する変更は行いません。
カーネル、newlib、wire protocolへの変更もありません。

## 検証（167）

`mount_path_walk.rs`は絶対・相対・連鎖したリンク、ディレクトリ先の後続ファイル、
マウント境界、ルートへの閉じ込め、最終ディレクトリにsearch権限がない場合、
literalなリンクの参照を確認します。
`filesystem_handles.rs`は実ファイルシステムで、ENOTDIRになるopen・statのほか、
O_TRUNCで既存の内容を失わないこと、danglingなディレクトリリンクをO_CREATで
通常ファイルにしないこと、子ファイルの作成、readlink・nofollow・unlinkを確認します。

ホスト全体は651ターゲット・3,117件成功、失敗0件です。
KVMの`native_symlink_trailing.c`は実newlibから4グループを実行し、通常ファイルへの
誤ったopen・truncateの拒否、ディレクトリ経由の子作成、欠けたディレクトリへの
通常ファイル作成の拒否、リンク自体のrename・unlinkとHandle回収が成功しました。
ログは`out/checks/mochios-posix-host-audit-167.log`と
`out/checks/mochios-symlink-trailing-167.log`へ保存しています。
SDK・Rust stdの再生成は行っていません。

通常イメージも更新したfilesystem.serviceで起動し、`Binder.app ready`と
Dock・ステータスバーの描画をログとスクリーンショットで確認しました。
既存デスクトップのバイナリは維持しており、全アプリの再リンク検証ではありません。
通常起動ログにはnetwork.serviceのreadiness timeoutが残っています。
保存先は`out/checks/mochios-normal-167.log`と`mochios-normal-167.png`です。

## 残るパス互換性の作業

167では未定義だった`pathconf`・`fpathconf`は、現在は一部の項目を提供元へ問い合わせます。
対応範囲は[ファイルシステム制限](posix-filesystem-limits.md)と
[端末pathnameの観測](posix-device-observation.md)を参照してください。
既存リンクを宛先としたmkdir・symlink・hard link・socketの末尾`/`は182で整理しています。
openのexclusive createやその他の入口の長さ検査は引き続き確認が必要です。
pathname互換性全体の完了を意味するものではありません。

## 成分数と長さエラー（168）

共有walkerの1,024回という成分探索の上限を削除しました。4,096バイト以内の
正当な入力でも、`./`などを多く含むとELOOPになっていたためです。
名前・`.`・`..`は有限の待ち行列を消費し、待ち行列を補充できるシンボリックリンク
展開は従来通り40回までです。成分ごとのsearch権限検査も維持しています。

walkerへ渡された入力全体がMAX_PATH_LENを超えた場合と、ext4の成分長255バイトを
超えた場合は、EINVALではなくENAMETOOLONGを返します。展開したリンク先の
成分長エラーにも同じ変換を適用します。埋め込みNULなどの不正なwire入力は
従来通りEINVALです。リンク先と未解決suffixの合成長の計算は169で修正しています。

`path_walk_limits.rs`で長い通常成分列・`.`・`..`・リンク展開、255/256バイト境界、
MAX_PATH_LEN超過、nofollow、リンク循環の上限を検証します。
`filesystem_handles.rs`でも実ext4のopen/create・statを通し、1,500個の`.`に続く
255バイト名での作成成功と、過長パスのENAMETOOLONGを確認します。
この変更はホスト上で再現できる探索処理のみなので、VM起動・通常イメージの更新、
SDK・Rust stdの再生成は行いません。

全体のホスト回帰テストは652ターゲット・3,124件成功、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-168.log`へ保存しています。

## リンク展開時の長さ（169）

従来はリンク先の長さに元の入力全体の長さを加算していました。このため、すでに
探索した長いprefixを再度数えて正当な入力を拒否し、逆に複数のリンク展開で残った
suffixが増えた場合にはその増加を数えず、実際の長さ超過を見逃していました。

成分ごとに後続する区切り文字のバイト数を保持し、展開時はリンク先・リンク直後の
区切り・未解決の全suffixだけを加算します。複数の区切りを正規化して短く数えることは
せず、リンク展開を重ねても元の区切りの長さを維持します。合成後にMAX_PATH_LENを
超える場合はENAMETOOLONGです。末尾のディレクトリ条件とnofollow指定は維持します。
空のリンク先をproviderが返した場合も、親や後続成分へ解決せずENOENTを返します。

追加した回帰テストのうち5件が修正前に失敗することを確認しました。walkerのテストは
解決済みprefixの除外、4,096バイトちょうどのリンク先、区切りを含むsuffixの境界、
繰り返し展開で増えるsuffix、nofollow、空のリンク先を扱います。実ext4でも長いprefixと
長いリンク先によるopen・statの成功、および1バイト超過の拒否を確認しています。
ホストで再現できる処理のため、この変更でもVM・通常イメージ・SDK・Rust stdは更新しません。

対象79件と全体652ターゲット・3,131件が成功し、失敗は0件でした。
ログは`out/checks/mochios-path-expansion-169-focused.log`と
`out/checks/mochios-posix-host-audit-169.log`へ保存しています。

## 参照パス自身の末尾区切り（170）

[POSIXのpathname resolution](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap04.html)
では、最終リンク自体を返す条件に、pathnameに末尾の区切りがないことが含まれます。
従来はNOFOLLOW指定だけで最終リンクを残したため、`lstat("dir-link/")`相当の参照や
`O_NOFOLLOW`付きopenが、ディレクトリへのリンクでもENOTDIRになっていました。

共有walkerの厳密なnofollow指定を変更する代わりに、対象を参照する入口で
`lookup_symlinks`を使います。stat・readlink・access・chown・set_times、および
exclusive create以外のopenで、末尾`/`がある場合はディレクトリへ解決します。
末尾`/`のないNOFOLLOWは従来通りリンク自体を対象にします。
readlinkの`dir-link/`は解決先がリンクではないためEINVAL、ファイルへのリンクは
ENOTDIR、欠けたリンク先はENOENTです。対象の取得と操作は従来通り同じprovider処理内で行います。

unlink・renameは最終リンク自体を保持する指定を維持します。これらへ参照用の
追従処理を適用して、リンク先のディレクトリを削除・移動することはありません。
mkdir・symlink作成・socket作成・hard link・exclusive createの細かなエラー方針は、
この変更の完了範囲には含めていません。

`support/trailing_lookup.rs`の実ext4テストで、stat/readlink/openの末尾区切り、
参照先のaccess判定、最終ディレクトリに不要なsearchを要求しないこと、chown・時刻更新が
リンク自体の属性を変えないこと、unlink・renameがリンク先を変更しないことを確認します。
試験用ルートをsearch可能にし、アクセス試験はlink自体のF_OKと参照先のR_OKを区別します。
カーネル・IPC・SDKへの変更はなく、ホストで検証できるためVMと通常イメージは更新しません。

対象84件と全体652ターゲット・3,136件が成功し、失敗は0件でした。
ログは`out/checks/mochios-trailing-lookup-170-focused.log`と
`out/checks/mochios-posix-host-audit-170.log`へ保存しています。

## 末尾区切りを落とした誤作成の防止（171）

名前探索は、mkdirのために末尾`/`のある未存在エントリもMissingとして返します。
この結果を非ディレクトリ作成やrenameで無条件に使うと、symlink・socket・hard linkが
`new/`への要求から`new`を作成し、renameも本来失敗すべき宛先へ移動していました。

各操作の同じprovider処理で、Missingとなった宛先の末尾区切りを調べ、inode作成・
link countや時刻の更新前に拒否します。symlinkとsocketはENOENT、hard linkは
ENOTDIRです。renameは非ディレクトリの移動ならENOTDIR、ディレクトリならENOENTとし、
いずれも元のエントリを保持します。事前statや別RPCによる再探索は追加しません。
mkdirの末尾区切りによる作成と、既存ディレクトリ宛先へのrenameは維持します。

[POSIX rename](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html)は、
末尾区切り付きの宛先が既存ディレクトリを指さない場合、失敗することを要求します。
またhard linkのsourceは参照用の末尾区切り解決を使い、ディレクトリリンクをたどった場合も
ディレクトリのhard link作成は禁止します。区切りなしのsourceリンク自体へのhard linkは維持します。

`support/trailing_creation.rs`の6件中5件が修正前に失敗することを確認しました。
実ext4で拒否後の宛先不在、元のinodeとlink countの維持、socketのretained・非retained両経路、
mkdirと既存ディレクトリへのrename、sourceリンク自体へのhard linkを検証します。
この変更もホストで検証可能な処理のみで、カーネル・IPC・SDK・通常イメージは変更しません。

対象90件と全体652ターゲット・3,142件が成功し、失敗は0件でした。
ログは`out/checks/mochios-trailing-creation-171-focused.log`と
`out/checks/mochios-posix-host-audit-171.log`へ保存しています。

## 既存の作成先と末尾区切り（182）

mkdir・symlink・hard link・socket作成で、既存の最終シンボリックリンクに`/`を付けると、
既存エントリとしての拒否より先にwalkerのディレクトリ条件が働き、ENOTDIRになっていました。
[POSIX mkdir](https://pubs.opengroup.org/onlinepubs/9799919799/functions/mkdir.html)は、
作成先がシンボリックリンクを指す場合にEEXISTで失敗することを規定しています。
この作成系の最終エントリは、リンク先へ変換せずに既存と判定する必要があります。

作成先の探索だけで、入力末尾の`/`を取り除いた借用sliceを使います。元のpathnameは保持し、
未存在なら171で定めた非ディレクトリ作成の末尾区切り拒否をそのまま適用します。
mkdirの新規ディレクトリ作成は維持します。親の`.`・`..`・symlinkは通常通り探索し、
親ディレクトリのsearch/write権限も維持します。これは提供元の同じ排他的な操作内で行い、
別のstat要求や再探索を追加しません。

元の長さを検査してからsliceを作るので、大量の末尾`/`でMAX_PATH_LENを超えた入力が
短くなって許可されることはありません。symlinkとhard linkの入口に残っていた
過長パスのEINVALもENAMETOOLONGへ揃えています。unlink・renameや通常の参照用walkerは
変更していません。openのO_CREAT|O_EXCLへの適用も、この変更には含めません。

`support/trailing_creation.rs`へ追加した6件のうち、修正前は5件が失敗しました。
既存ディレクトリ・通常ファイル・dir/file/danglingリンクへの作成失敗、inodeとlink countの
維持、dangling参照先の未作成、retained socketでのopen ID未消費を確認します。
また親の検索拒否、親リンクを通した正当なmkdir、MAX_PATH_LENの境界と過長入力も確認します。
既存の未存在宛先・rename・hard link sourceの試験と合わせた対象12件は成功しています。

この変更は提供元内のpathnameとinodeの状態処理だけです。IPC・native ABIは変更せず、
ホスト上の実ext4試験とfilesystem.serviceのネイティブビルドで検証します。
VM起動、通常イメージへの配置、SDK・Rust stdの再生成は行っていません。

全体656ターゲット・3,188件が成功し、失敗0件でした。
修正前と途中の長さ検査の失敗は成功ログと区別しています。
保存先は`out/checks/mochios-creation-destination-182-before.log`、
`out/checks/mochios-creation-destination-182-intermediate-length-failure.log`、
`out/checks/mochios-creation-destination-182-focused-final.log`、
`out/checks/mochios-posix-host-audit-182-final.log`です。
filesystem.serviceのビルドも成功し、`out/checks/mochios-creation-destination-182-native-build.log`
へ保存しています。通常イメージ上のfilesystem・POSIXバイナリは180の証跡と同じことを
ハッシュで確認しており、この変更の起動試験を済ませたものとして扱いません。
