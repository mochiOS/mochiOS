# filesystem提供元の制限問い合わせ

`pathconf`・`fpathconf`のために、全FDへext4の固定値を返す実装は追加しません。
まず、保持しているファイルの提供元から制限を取得する経路を追加しています。
libcの`_PC_*`番号やPOSIXの意味論はカーネルへ持ち込みません。

## 実装済みの提供元経路（172）

`filesystem-protocol::object_limits`のv1 selector `MFS1LIMT`へ、空の入力で
native HandleQueryを送ります。write-onlyのdescription向けには、同じ観測を
HandleControlでも明示的に提供します。必要なREAD/CONTROL権限は既存のnative機構が
検査し、providerへ届くobject IDは認証された配送から取得します。
追加のHandle添付、引数、入力データ、異なる出力長は拒否します。
任意PID・FD・パスから他プロセスのHandleを取得するAPIはありません。

応答は厳密に16バイト、4個のlittle-endian u32です。

| フィールド | 現在のext4提供元の根拠 |
| --- | --- |
| `name_bytes_max` | `DirEntryName::MAX_LEN`。パーサが検査する名前長上限 |
| `path_bytes_max` | 共有walkerが検査する`MAX_PATH_LEN` |
| `link_count_max` | 実際のlink count型と、追加前の上限検査に対応するu16最大値 |
| `filesystem_block_bytes` | 保持したdescriptionの実ボリュームの`block_size_bytes()` |

byte数はC文字列の終端NULを含みません。ブロックサイズも全オブジェクトの
最小割当量・direct I/O対応を保証する値ではありません。POSIX値への変換は
後続のPOSIX層で扱います。

問い合わせはデータの読み取り、cursorの移動、flags・時刻の変更、pathnameの再探索を
行いません。unlink後も保持しているdescriptionのボリュームから値を返します。
不明・close済みIDはEBADF、device descriptionはENOTSUPであり、ext4の値へfallbackしません。
カーネルABI・カーネル側のPOSIX処理は追加していません。

## 検証

`services/posix/tests/filesystem_limits_wire.rs`でバイト順・往復変換・過不足のある
応答長を確認します。`support/object_limits.rs`では、実ext4を使ってquery/control両方の
観測、cursor・flags・内容の維持、unlink後の参照とclose後の拒否、不正入力、デバイスの
拒否を確認します。1KiBと4KiBのボリュームを同じnamespaceへmountし、各descriptionが
自分のボリュームのサイズを返すことも確認しています。

対象のホストテスト79件が成功しました。mochiOS向けfilesystem-serviceも、既存SDKと
Rust std overlayを入力としてビルド済みです。SDK・Rust std・通常イメージは更新しておらず、
今回の問い合わせをKVM上で実行したという主張はしていません。

全体のホスト回帰テストも653ターゲット・3,148件が成功し、失敗0件でした。
検証ログは`out/checks/mochios-object-limits-172-focused.log`、
`mochios-posix-host-audit-172.log`、`mochios-object-limits-172-native-build.log`です。

## 残る実装

`pathconf`・`fpathconf`のlibc exportと名前番号の変換はまだ未実装です。
次に保持FDのleaseからこの経路へ接続し、pathname版はread権限を不必要に要求するopenへ
置き換えず、認証済みcredentialsとroot/base Handleで対象を解決する経路を作ります。
pipe・terminal・deviceには各提供元の制限と適用可否の処理が必要です。

[POSIXのunistd定義](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/unistd.h.html)
にある各`_PC_*`への対応、上限不定と未対応の区別、errno維持、C文字列長の扱い、
実newlibからのKVM試験まで確認するまでは、pathconf互換性を完了扱いにはしません。
