# 端末の読み取り専用観測

`tcgetattr`・`isatty`の端末判定・`tcgetwinsize`は、読み取り可能な端末Handleでは
既存のネイティブ`HandleQuery`を使います。属性の取得だけにCONTROL権限を要求しません。
読み取り権限のない書き込み専用の端末では、既に持つCONTROL権限と取得専用の
controlコードを使います。queryが失敗した後にcontrolへ切り替える処理はありません。

新しいユーザー空間のコードは`PTY_QUERY_ATTRIBUTES`・`PTY_QUERY_WINDOW_SIZE`です。
どちらも空入力で、既存のフィールド単位のwire形式を返します。カーネルには
端末の知識・queryコード・POSIX専用syscallを追加していません。ネイティブの
READ検査、Provider配送の認証、明示的に保持したHandleの参照をそのまま使います。
サービス名の検索や他プロセスのFD番号による取得も行いません。

提供元では`&self`の観測処理へ切り出し、既存の取得controlと同じ処理から応答を
作ります。queryの名前空間から設定変更、キューの消費、flush、flow操作へは
進みません。設定変更のCONTROL検査とSIGTTOU等のjob control処理は維持します。
観測自体ではjob control停止やWINCHなどの通知を生成しません。
未知のコード、不正な入力、短い容量、解放済みの参照はそれぞれエラーです。

ランタイムでは既存のFD leaseと`io_resources`の内側で呼び出します。
短い応答や不正なwire形式をEIOにし、提供元や権限のエラーを成功に変えません。
通常ファイルは端末として問い合わせずENOTTYにし、特殊ファイルの提供元が
端末操作を実装しない場合も従来と同じENOTTYへ変換します。

## O_NOCTTYの受理

既存のPTY回帰試験を実行したところ、`open(..., O_NOCTTY)`が観測処理へ到達する前に
EINVALになりました。SDKのO_NOCTTY（0x8000）がnewlibの許可フラグから漏れていたためです。
フラグを受理し、その他の不明なビットや不正なアクセスモードの拒否は維持します。
`newlib_noctty.rs`で実変換関数をコンパイルし、対応済みフラグとの組み合わせを確認します。

mochiOSは制御端末を明示的なユーザー空間の取得操作で割り当て、openでは自動取得しません。
したがって新しいカーネル・filesystemフラグへ転送する必要はありません。
[POSIXの制御端末の規定](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap11.html)
では、O_NOCTTYなしで開いたsession leaderへの自動割り当ては実装定義で、
O_NOCTTYありでは制御端末を取得しないことを要求します。

## 検証（165）

`terminal_observation.rs`は実PTY状態で、master/slaveの両方、controlとの値の一致、
観測による属性・readiness・通知キューの不変性、queryから変更controlへ到達できないこと、
不正な要求と解放済み参照の拒否を確認します。
`terminal_special_runtime.rs`は実ランタイムをコンパイルして、READのみとCONTROLのみの
両経路、特殊ファイルの端末判定、応答長とerrnoを確認します。queryがエラーになる場合も
呼び出しが一度であることを追加で確認しています。

O_NOCTTY修正後のホスト全体は650ターゲット・3,113件成功、失敗0件です。
非再試行の確認を追加した対象2件も成功しています。
ログは`out/checks/mochios-posix-host-audit-165-final.log`と
`out/checks/mochios-terminal-observe-no-retry-165.log`です。
この変更は端末観測の権限分離であり、POSIX全体の適合やVim・WebKitの動作の証拠ではありません。

`native_terminal_observation.c`は既存PTY master回帰試験に加え、読み取り専用・
書き込み専用・読み書きの3アクセスモードで実newlibの`isatty`・`tcgetattr`・
`tcgetwinsize`を確認しました。READだけに縮小したネイティブ参照から属性とサイズを
取得し、変更controlとquery経由の変更要求が拒否されること、参照が回収されることも
確認しています。別のsession leaderがO_NOCTTYで開いても制御端末を取得しない試験も
成功しました。最終ログは`out/checks/mochios-terminal-observe-165.log`です。

最初のKVM実行では、既存PTY試験がO_NOCTTY未対応で止まり、新しい観測試験へ
到達していません。そのログは`out/checks/mochios-terminal-observe-165-noctty-failure.log`
として区別し、成功の根拠には含めません。SDKとRust stdは再生成せず、独立した
出力先で新ランタイムをビルドして診断実行体とPOSIXサービスへリンクしました。

通常起動では診断用compositor・service-managerを元へ戻し、新しいPOSIXサービスと
既存のデスクトップ実行体を組み合わせてBinderのREADY、Dockとステータスバーの表示を
確認しました。画面も取得して確認し、`out/checks/mochios-normal-165.log`・
`out/checks/mochios-normal-165.png`へ保存しています。全アプリの新ランタイムでの
再リンクや、未撤去の公開フラグRPCの整理までを確認したものではありません。
