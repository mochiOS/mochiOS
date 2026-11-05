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

本番のsigned RAM / object image要求は、一回の認証済みsnapshotから同じ表示経路へ進みます。
予約したsource容量と要求元のProcess参照をbrokerが保持し、元のIPC返信を決定まで保留します。
rendererとの別のIPCで待つため、表示中もbrokerは他の要求・終了・入力通知を処理できます。
期限切れ、session / policy / image revisionの変更、表示側や要求元の終了で取消し、所有資源を返します。

表示側は既存secure-ui binaryの専用モードです。service-managerがProcess所有権とPOSIX後始末を
保持します。同じ表示プロセスがViewKitのwindowを破棄して待受けへ戻り、要求ごとに新しい
correlation / window / first-frame状態 / input armを使います。古いviewは元の要求だけを所有し、
後続要求をglobalから取り直しません。プロセス終了や表示失敗時は監督側が後始末と再起動を行います。
新しいサービスやkernel専用interfaceは追加しません。
idle表示側がない間はNotReady（POSIX境界でEAGAIN）を返します。storage確保失敗のENOMEMと区別します。

待受け`MCNW`と操作`MCNC`はversion / 長さ / 予約領域を固定します。操作側が送れるものは
correlation、Presented / Decide / Dismissedだけです。brokerは送信元の測定済みimageを検査し、
compositorの実Presented receipt、現在のsession / focus、独立したInput Authorityのarmと入力履歴を
取得します。rendererが入力serialや権限を申告することはできません。
ViewKitもwindow protocol eventのIPC senderを接続先compositorと照合します。

GUIは標準ViewKit Dialogを独立した保護windowとして表示します。初回frameの実表示とarmの確認が
終わるまで肯定操作を受け付けません。拒否・Escape・window終了は取消しとして処理します。
採用する肯定はAllow Onceだけで、対象imageの実行に必要な権限へ縮小した一回の認可を作ります。
要求元プロセスの権限は変更しません。表示側再起動中、入力未同期、別の要求が表示中の状態は
再試行可能なNotReadyです。明示的な拒否や無効な証拠を自動的に再試行して許可する処理はありません。

ホストでは共通handoff、入力・表示、wire、認可、要求所有、ViewKit sender admissionの9対象107件が
成功しました。待ち状態をENOMEMとして返していた不具合を修正し、追加の回帰試験を含む認可54件も
成功しています。表示側再利用時の古いcorrelation・未armの拒否も加え、認可55件が成功しました。
記録は`out/checks/mochios-consent-369-host.log`、`mochios-consent-372-host.log`、`mochios-consent-375-host.log`です。
capability / secure-ui / service-managerとnative試験のmochiOS release buildも成功しています。

362のKVMでは通常DockからTerminalを起動し、mshからnative_resident_scopeを実行しました。
実brokerへの不明なticketと予約領域が不正なfetch要求は、それぞれENOENT / EINVALで拒否され、
Handleや許可を返さず、mshへ戻りました。成功画面は
`out/checks/mochios-resident-362-result.png`です。これは実IPCの拒否経路の検証です。
有効な要求の表示と肯定判断をKVMで通した試験ではありません。

368のKVMでは通常Terminalからnative試験を実行し、guestのclang、LLD、msignによる自動署名まで
成功しました。370 / 371では署名済みfixtureを使ってGUI判断を切り分け、実Allow Once操作後の
子がnet.connectだけを取得し、capabilities.manage / fs.write.allを取得せず、親にもnet.connectが
追加されないことを確認しました。画面は`mochios-consent-368-probe-4.png`、
`mochios-consent-370-allow-result.png`、`mochios-consent-371-allow.png`です。
この時点では連続する2回目の実行がENOMEMで失敗しました。出力captureなしでも再現しており、
成功した負例試験として数えていません。再起動待ちのエラー分類をNotReadyへ直した後も、
毎回表示プロセスを起動し直す時間で次の要求が待ち切れない状態を確認し、上記の再利用へ変更しました。

375のKVMでは通常Dock / Terminal / mshから署名済みfixtureを起動し、実際のマウスとkeyboardで
Allow Once → Deny → Escape取消し → Allow Once againの4操作が連続して成功しました。
肯定時は子のnet.connectと終了値37を検査し、capabilities.manage / fs.write.allは拒否されます。
否定時は実行前のPermissionDeniedを検査し、毎回親のnet.connect不在も確認しています。
最後に一時directoryを除去してmshへ戻りました。全結果を表示した画面は
`out/checks/mochios-consent-375-result.png`です。CSDのcloseボタンによる終了を追加検証した
という意味ではありません。guest clangによるcompile / link / 自動署名は上記368で別に確認しています。

試験用4ファイルを除去し、診断用loggerを元へ戻して通常イメージを再生成しました。
2,654ファイルと4つのcommand symlinkを読み戻し、6成果物のchecksumとSDKのhash不変を確認しました。
376の通常起動でもBinder / Dock / menubarを確認し、所有VMを停止しています。記録は
`out/checks/mochios-consent-364-{artifacts,readback}.json`、
`mochios-consent-376-normal-desktop.png`、`mochios-consent-376-normal-boot.log`です。
NICなしrunnerの既存network readiness失敗(-6)は残っています。

CLI用の表示形式は共通ですが、mshの保護されたtext rendererへの接続は未完了です。
測定済みmshからの要求はText側へ振り分け、表示側がなければNotReadyで終了します。
任意のstdinのyへフォールバックしません。通常のstdinやrendererが送る肯定だけで新しい権限を
付与する経路もありません。永続許可、process許可、text側の実入力経路は引き続き未完了です。
