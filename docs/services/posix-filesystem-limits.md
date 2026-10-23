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

`fpathconf`の先行対応は173で追加しています。`pathconf`のlibc exportと
pathname版は、read権限を不必要に要求するopenへ
置き換えず、認証済みcredentialsとroot/base Handleで対象を解決する経路を作ります。
pipe・terminal・deviceには各提供元の制限と適用可否の処理が必要です。

[POSIXのunistd定義](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/unistd.h.html)
にある各`_PC_*`への対応、上限不定と未対応の区別、errno維持、C文字列長の扱い、
実newlibからのKVM試験まで確認するまでは、pathconf互換性を完了扱いにはしません。

## fpathconfの先行対応（173）

`newlib-runtime::file_configuration`でC ABIの`fpathconf`を追加しました。
FD lookupとlease取得をSTATE内で行い、問い合わせ中はSTATEを保持せず、callback用の
resource scopeでdescriptionを維持します。再openやPOSIX endpointへのRPCは行いません。
READ権限があればHandleQuery、write-onlyなら明示されたHandleControlの観測を使い、
権限エラーが出た後に別経路へfallbackしません。

| newlib selector | 対応 |
| --- | --- |
| `_PC_LINK_MAX` | 提供元のlink count上限 |
| `_PC_NAME_MAX` | 提供元の名前byte上限 |
| `_PC_PATH_MAX` | 提供元とPOSIX wireのpathname byte上限の小さい方にNUL 1バイトを加算 |
| `_PC_PIPE_BUF` | pipe read/write端で、既存のatomic write方針`stream::PIPE_BUF`を直接返す |

現在のext4では順に65,535・255・4,097、pipeでは512です。
SDKヘッダのPATH_MAXに基づくcaller-owned realpath出力バッファの制限は変更していません。
問い合わせ成功時にerrnoを書き換えません。無効FDはEBADF、未知の名前番号はEINVALです。
既知でも未対応の項目や種類はENOTSUPとし、未実装を「上限なし」として返しません。
不正な応答長や取得対象の上限が0である応答はEIOです。

ホストは実ランタイムモジュールを含む6件で、FD lease、selector、応答長・上限、
エラー伝播、errno、query/control選択、pipeで問い合わせを発生させないことを確認しています。
全体654ターゲット・3,154件も成功し、失敗0件でした。

`native_fpathconf.c`を独立ビルドの実newlib-runtimeへリンクし、ELF内の実定義も確認しました。
KVMの4グループは、owner値とアクセスモード・errno、READだけへ権限縮小したHandle、
pipeと未対応・エラー、unlink/dup後の対象維持・cursor・Handle回収をすべて通過しました。
ログは`out/checks/mochios-fpathconf-173-native.log`と
`out/checks/mochios-posix-host-audit-173.log`です。SDK・Rust stdは再生成していません。
この4項目の先行実装であり、pathconf/fpathconf全項目の完了ではありません。

通常イメージには172でビルドしたfilesystem-serviceを配置し、診断用に置き換えた
compositor・service-managerは元へ戻しました。BinderのreadyとDock・ステータスバーの描画を
ログとスクリーンショットで確認しています。既存アプリ全体の再リンク検証ではありません。
保存先は`out/checks/mochios-normal-173.log`と`mochios-normal-173.png`です。
