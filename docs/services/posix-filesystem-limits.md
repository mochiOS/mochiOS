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

`fpathconf`の先行対応は173、`pathconf`の3項目への接続は174で追加しています。
対象ごとの全項目対応は未完了です。
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

## pathconfのパス解決（174）

`pathconf`のLINK_MAX・NAME_MAX・PATH_MAXを接続しました。callerの文字列は既存の
`path_io` resource scopeでsnapshotし、POSIXの`OP_PATH_LIMITS_AT`へ渡します。
新しい番号はユーザー空間のwire protocolであり、mnuのsyscallは追加していません。

POSIX側は既存の読み取り専用`path_query`/`path_request`経路を利用します。
認証されたprocessの保存済みeffective credentialsとroot/cwdを取得・保持し、状態ロックを
解放してからfilesystemへ問い合わせます。明示的なbaseは添付Handleでしか指定できず、
要求内の自己申告identity・FD・生ポインタは使いません。NOFOLLOWなどの余分なflagsは拒否します。

filesystemの`OP_OBJECT_LIMITS_AT`は、認証済みcontrol endpointとroot/base添付を使って
一度だけ対象を解決します。親のsearch権限を検査し、最終対象をopenしたりデータをreadしたりせず、
同じnamespace処理内で解決先ボリュームの制限を返します。新しいFileHandleやFDは作りません。
提供元の値とnewlib selectorの変換はfpathconfと共有します。

ホストでは、read権限のない対象・search権限のない親、rootからの脱出拒否、flagsとHandle数、
保存済みidentity、1KiB/4KiBのmount先、wireの過不足を確認しています。新しい要求も
読み取り専用操作の既存テスト行列へ加え、状態ロック外の問い合わせ、anchor回収、
要求中のimage置換後に応答を渡さない処理まで検証しています。
対象565件と全体654ターゲット・3,158件が成功し、失敗0件でした。

実newlibの`native_pathconf.c`をKVMで実行し、5グループすべて成功しました。
提供元の上限・symlinkと末尾区切り、callerポインタとselectorのエラー、UID変更後の
read不可対象への問い合わせと親searchの拒否、chrootと絶対リンクの閉じ込め、
128回の問い合わせ後のHandle回収を確認しています。通常のfilesystemとposixの
両バイナリを独立ビルドし、SDK・Rust stdは入力としてのみ使用しています。
ログは`out/checks/mochios-pathconf-174-native.log`と
`out/checks/mochios-posix-host-audit-174.log`です。

診断用compositor・service-managerを元へ戻し、新しいfilesystem・posix両サービスを
通常イメージへ残しました。BinderのreadyとDock・ステータスバーの描画も確認済みです。
既存の全アプリを再リンクしたという検証ではありません。通常起動の証跡は
`out/checks/mochios-normal-174.log`と`mochios-normal-174.png`へ保存しています。

pathconfはこの3項目の先行対応です。pipe・terminal・deviceの適用可否や未実装項目を
上限不定に偽装する処理は追加していません。
