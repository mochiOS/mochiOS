# ファイルの共有状態フラグと直接Handle経路

通常ファイル・ディレクトリ・非端末の特殊ファイルの`F_GETFL`・`F_SETFL`・`FIONBIO`は、保持済みの
Provider Handleから提供元を直接呼びます。POSIX endpointの取得、パス解決、
Handle転送、posix.serviceへの中継は行いません。カーネルへの追加・変更もありません。
FD番号、newlibのフラグ、errnoへの変換はユーザー空間に置きます。

`filesystem-protocol::description_flags`に提供元の契約を定義しています。
`GET`は空入力・4バイトのLE応答です。READ参照では既存の`HandleQuery`を使い、
READのない書き込み専用参照では、既存のCONTROL権限で`HandleControl`を使います。
失敗した問い合わせを別の権限やPOSIX経路で再試行しません。

`UPDATE`は`HandleControl`だけで受け付けます。入力はLEのu32のmaskと値、
合計8バイトです。未知のビット、mask外の値、過不足のある入力、応答容量、
不要な添付Handleは変更前に拒否します。`HandleQuery`の名前空間では変更を
受け付けません。カーネルはコードとフラグの意味を解釈せず、既存のREAD・CONTROL
検査とネイティブ配送の証明を担当します。提供元は通常IPCからの自己申告IDを
受け付けない既存の受信境界の内側で処理します。

共有状態はfilesystem.serviceの同じopen descriptionに置きます。
既存の`update_flags_handle`を再利用し、提供元の排他的な処理区間で一度に変更します。
`F_SETFL`は対応する可変フラグ全体を置き換え、`FIONBIO`はNONBLOCKだけを変更します。
クライアント側の取得・変更・設定による競合はありません。アクセスモードと
CLOEXECなどのdescriptor-localなフラグ、共有カーソルは変更しません。

ランタイムの呼び出しは既存の`io_resources`とFD leaseの内側に置きます。
STATEロックをRPC中に保持せず、ハンドラがFDを閉じて再利用しても元の参照を
回収前まで維持します。不正な応答長・未知の応答ビットはEIOです。
エラー時に古いメタデータを成功値として返しません。

非端末の特殊ファイル（`ObjectSpecial`）も、提供元の同じ契約を使います。
現在の`/dev/null`では、filesystem.serviceが保持するNullDescriptionのフラグを
操作します。新しい非端末デバイスの提供元も、このquery/control契約を実装する
必要があります。未対応を受け取っても、POSIX endpointや別のメタデータ参照へ
自動で切り替えません。端末として分類した参照、ソケット、共有メモリ、
ネイティブストリームは各提供元向けの既存経路を継続します。
全てのメタデータ操作の中継がなくなったという意味ではありません。
163では以前の公開RPCを残していました。180での撤去範囲は後述します。

## 検証（163）

ホスト全体は648ターゲット・3,108件成功、失敗0件です。その後、wireの
mask・値・全切り詰め長・末尾データの試験を追加し、対象4件を再実行して成功しました。
`file_description_flags_runtime.rs`は実アダプタを、POSIX endpointの実装を持たない
fixtureへ組み込み、要求内容・権限別の入口・エラー非再試行・応答検証を確認します。
`fcntl_status_resources.rs`は既存のFD再利用・再入・参照寿命の試験を新経路にも
適用します。`filesystem_handles.rs`は実ファイルシステムのopen descriptionで
両経路の共有状態、独立open、不正な要求と閉じた参照の拒否を確認します。

`native_file_description_flags.c`は実newlibとfilesystem.serviceをKVMで動かします。
別FD・fork後の共有、独立open、append書き込み、カーソルとCLOEXECの維持、
READへ縮小した参照からの取得、CONTROLのない参照からの変更拒否、書き込み専用
ファイルとディレクトリ、終了後のHandle数の復帰の4グループが成功しました。
SDK・Rust stdの再生成は行わず、ランタイムは独立した出力先でビルドしています。

ログは`out/checks/mochios-posix-host-audit-163-final.log`、
`out/checks/mochios-description-flags-codec-163.log`、
`out/checks/mochios-description-flags-163.log`です。途中のコンパイル失敗は
成功の根拠に含めません。POSIX全体の適合、Vim・WebKitの動作を示す結果でもありません。

通常イメージでは診断用のcompositor・service-managerを元へ戻し、新しい
filesystem.serviceを組み込んでBinderのREADYとDock・ステータスバーの表示を確認しました。
画面は実際に取得して確認し、`out/checks/mochios-normal-163.png`と
`out/checks/mochios-normal-163.log`へ保存しています。通常起動は既存のデスクトップ
実行体との統合確認であり、全アプリを新ランタイムで再リンクした試験ではありません。

## 非端末の特殊ファイルへの接続（164）

`ObjectSpecial`の残っていた中継を外しました。`fcntl_status_resources.rs`からも
POSIX Clientの模擬実装を削除し、特殊ファイルを含む再入・FD再利用の試験を
その実装なしで動かしています。`ext4_mount_namespace.rs`ではディレクトリと
実際のNullDescriptionに対して同じ直接query・マスク更新を実行します。

ホスト全体は648ターゲット・3,109件成功、失敗0件です。NullDescriptionへの
マスク更新の確認を追加した後、対象2ターゲット・87件も再実行して成功しました。
`native_special_description_flags.c`は前節の通常ファイル4グループに加え、
`/dev/null`の読み取り専用・書き込み専用・読み書きの3グループをKVMで確認します。
各モードでアクセスモード・CLOEXEC、独立open、FD・fork間の共有、マスク更新、
EOF・破棄書き込み、Handle回収を確認しました。読み取り可能なモードでは、
READだけへ縮小したネイティブ参照のquery成功とCONTROL拒否も確認しています。

ログは`out/checks/mochios-posix-host-audit-164.log`、
`out/checks/mochios-special-flags-focused-164-final.log`、
`out/checks/mochios-special-flags-164.log`です。新ランタイムをリンクした診断実行体での
確認であり、配布用SDK・Rust std・既存デスクトップ実行体は再生成していません。
診断終了後は元の通常起動用compositor・service-managerをバイト比較付きで復元しました。
通常起動に使う実行体は163で確認したものと同じなので、今回はその起動試験を
繰り返して新ランタイムの確認とみなすことはしません。

## 旧POSIX中継RPCの撤去（180）

通常ファイル・ディレクトリ・非端末特殊ファイルの実装を再検索し、newlibは既に直接
Handle経路を使い、旧POSIX ClientのフラグRPCを呼ぶproduction利用者がいないことを
確認しました。`OP_FGETFL`・`OP_FSETFL`・`OP_FUPDATEFL`の定義、Clientの
get/set/update_status_flags、PosixServiceのdispatchとFilesystemBackendの対応メソッド、
native filesystem中継と専用応答ヘルパーを削除しています。互換aliasは設けていません。
filesystem自身の提供元API、共有メモリ・socketのローカル状態処理は対象ではありません。

旧RPCだけを試すClient fixtureは撤去し、残るファイル操作の権限・応答参照回収・
STATEロック・image置換の検証行列は維持しています。共有メモリの提供元が引き続き
使用するmask/value codecと、その不正な入力を拒否する試験は残しました。
実runtimeをPOSIX endpointなしで組み込む既存試験も、そのまま直接経路を検証します。

添付Handle付き未知要求が既存の添付検査でEINVALになっていたため、assigned requestの
認識をprotocol側へ追加しました。PosixServiceは未知opcodeを添付の種類の判定より前に
ENOSYSで拒否し、filesystemへ転送しません。OP_STATUSも要求としては拒否します。
既知opcodeで余分なHandleがある場合は、従来通り操作を実行せず拒否します。
公開された全request定数と認識範囲の一致を検証し、未割り当て値を公開定数や互換操作として
残しません。ネイティブIPCで受信した参照の破棄は既存のadapterの責務のままです。

公開カーネルABIも確認し、SyscallNumberの124項目には旧POSIX syscall名がありませんでした。
これはenum名の確認であり、全引数契約・カーネル内部がPOSIX非依存であることの最終証明では
ありません。この変更でカーネルを変更したわけでも、POSIX全体を完成したわけでもありません。

ホスト全体は656ターゲット・3,182件成功、失敗0件です。旧RPC専用のClient試験3件を
削除し、従来の中継動作の試験1件を、未知要求の拒否・backend未到達・公開opcodeの照合の
3件へ置き換えたため、179より件数は1件少なくなっています。
最初の対象試験で添付付き未知要求のEINVALを検出し、上記の拒否順序を修正しました。
失敗ログは`out/checks/mochios-flag-relay-removal-180-unknown-handle-rejection-before.log`、
成功ログは`out/checks/mochios-posix-host-audit-180-final.log`に分けて保存しています。

KVMでは新runtimeをリンクした`native_special_description_flags.c`を、新しいposix.serviceと
組み合わせて実行しました。通常ファイルのalias・独立open・mask更新・append・
READだけのqueryとCONTROL拒否・書き込み専用・directory・fork・Handle回収の4グループと、
`/dev/null`の3アクセスモードの計7グループが成功しました。
ログは`out/checks/mochios-flag-relay-removal-180-native.log`です。
未知opcodeの拒否はホスト上の実dispatcherで検証しており、このCプローブが未知要求を
ネイティブIPCで送信した結果としては数えません。SDK・Rust stdは再生成していません。

診断用compositor・service-managerを元のバイナリへ戻し、新しいposix.serviceを通常イメージへ
配置しました。filesystemは179のバイナリを維持しています。同じ通常起動でBinderのREADY、
Dockとステータスバーを確認しました。ログと視覚確認した画像は
`out/checks/mochios-normal-180.log`・`out/checks/mochios-normal-180.png`です。
既存アプリ全体の新runtimeでの再リンクや、POSIX全体の適合を確認したものではありません。
