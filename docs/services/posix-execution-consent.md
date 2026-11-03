# 共通の実行許可表示

`capability-protocol::consent::prompt`はCLIとGUIで共有する、brokerから表示側へ渡す形式です。
ticket / challenge / display、期限、選択肢、元ファイル全体のSHA-256、package ID、
要求Capabilityを明示的にserializeします。最大1,391 bytes、Capabilityは最大128個・
終端込み1,024 bytesです。version、予約領域、長さ、個数、重複と順序を検査します。
package IDとCapabilityは表示用のASCII識別子に限定し、改行、端末escape、bidi制御を拒否します。

`execution_approval::Pending::write_prompt`は保持中の認証済みSelectionだけから内容を作ります。
元ファイルを開き直さず、呼出側が送ったアプリ名や追加権限も採用しません。
要求一覧をsort / deduplicateした結果を渡します。現在のlaunch継続が提供する肯定の選択肢は
Allow Onceだけです。永続許可やprocess許可を暗黙に表示しません。

`Registry::write_prompt`は、その要求へ結び付いた表示Instanceだけに内容を返します。
`MCNF`要求にはcorrelationしかなく、serverがIPC senderから測定済みInstanceを取得します。
別の送信者、変更されたimage revision、期限切れ、取消済みticketを拒否します。
ticketを知っているだけで他の表示側の内容を取得することはできません。

CLI用の`write_text`とsecure-uiの`execution_prompt::Presentation`は同じ内容と選択肢を使います。
GUIは標準ViewKitのDialog / Text / Stack / Buttonで構成し、保護表示へ渡すbindingも保持します。
Buttonから出るのはcorrelationと選択候補です。入力証拠や実行許可を生成する処理はありません。

ホストではpromptの7件、既存wireの5件、controllerの23件、署名認可・要求所有の45件が成功しました。
secure-ui / capabilityのhost check、protocolのallocなしno_std check、両サービスのmochiOS向け
release buildも成功しています。記録は`out/checks/mochios-consent-360-*`です。

362のKVMでは通常DockからTerminalを起動し、mshからnative_resident_scopeを実行しました。
実brokerへの不明なticketと予約領域が不正なfetch要求は、それぞれENOENT / EINVALで拒否され、
Handleや許可を返さず、mshへ戻りました。成功画面は
`out/checks/mochios-resident-362-result.png`です。これは実IPCの拒否経路の検証です。
有効な要求の表示と肯定判断をKVMで通した試験ではありません。

本番launchから要求をRegistryへ登録する処理、表示側の起動・登録、保護された入力による
決定と最終認可への接続は未完了です。表示部品をビルドできたことを、通常のmsh／GUIで
許可画面が使用可能になった証明にはしていません。stdinの`y`や通常のUIクリックだけで
新しいCapabilityを付与する経路も追加していません。
