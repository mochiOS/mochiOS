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

`pathconf`・`fpathconf`は現在のnewlibランタイムとSDK libcに定義がありません。
提供元の実際の制約を問い合わせる設計が必要で、固定値で実装済みとしてはいません。
また、元のpathname自身が`/`で終わる場合の各操作のnofollow方針、展開長の計算、
walker外の入口で行う長さ検査には別の確認が必要です。167の修正は
リンク先の内容にある末尾区切りの扱いであり、pathname互換性全体の完了ではありません。

## 成分数と長さエラー（168）

共有walkerの1,024回という成分探索の上限を削除しました。4,096バイト以内の
正当な入力でも、`./`などを多く含むとELOOPになっていたためです。
名前・`.`・`..`は有限の待ち行列を消費し、待ち行列を補充できるシンボリックリンク
展開は従来通り40回までです。成分ごとのsearch権限検査も維持しています。

walkerへ渡された入力全体がMAX_PATH_LENを超えた場合と、ext4の成分長255バイトを
超えた場合は、EINVALではなくENAMETOOLONGを返します。展開したリンク先の
成分長エラーにも同じ変換を適用します。埋め込みNULなどの不正なwire入力は
従来通りEINVALです。リンク先と未解決suffixの合成長の計算自体は別の残作業です。

`path_walk_limits.rs`で長い通常成分列・`.`・`..`・リンク展開、255/256バイト境界、
MAX_PATH_LEN超過、nofollow、リンク循環の上限を検証します。
`filesystem_handles.rs`でも実ext4のopen/create・statを通し、1,500個の`.`に続く
255バイト名での作成成功と、過長パスのENAMETOOLONGを確認します。
この変更はホスト上で再現できる探索処理のみなので、VM起動・通常イメージの更新、
SDK・Rust stdの再生成は行いません。

全体のホスト回帰テストは652ターゲット・3,124件成功、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-168.log`へ保存しています。
