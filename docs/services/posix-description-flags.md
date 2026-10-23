# ファイルの共有状態フラグと直接Handle経路

通常ファイルとディレクトリの`F_GETFL`・`F_SETFL`・`FIONBIO`は、保持済みの
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

外部デバイスは異なる提供元の契約を持つため、今回の直接経路の対象ではありません。
`ObjectSpecial`は既存のメタデータ委譲経路を使います。ソケット、共有メモリ、
PTY、ネイティブストリームは各提供元向けの既存経路を継続します。
これらを含む全操作の中継がなくなったという意味ではありません。

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
