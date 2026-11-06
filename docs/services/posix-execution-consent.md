# 共通の実行許可表示

`capability-protocol::consent::prompt`はCLIとGUIで共有する、brokerから表示側へ渡す形式です。
ticket / challenge / display、期限、選択肢、元ファイル全体のSHA-256、package ID、
要求Capabilityを明示的にserializeします。最大1,391 bytes、Capabilityは最大128個・
終端込み1,024 bytesです。version、予約領域、長さ、個数、重複と順序を検査します。
package IDとCapabilityは表示用のASCII識別子に限定し、改行、端末escape、bidi制御を拒否します。

`execution_approval::Pending::write_prompt`は保持中の認証済みSelectionだけから内容を作ります。
元ファイルを開き直さず、呼出側が送ったアプリ名や追加権限も採用しません。
要求一覧をsort / deduplicateした結果を渡します。launch継続の肯定の選択肢は
Allow OnceとAlways Allowです。process単位の許可は提供しません。

`Registry::write_prompt`は、その要求へ結び付いた表示Instanceだけに内容を返します。
`MCNF`要求にはcorrelationしかなく、serverがIPC senderから測定済みInstanceを取得します。
別の送信者、変更されたimage revision、期限切れ、取消済みticketを拒否します。
ticketを知っているだけで他の表示側の内容を取得することはできません。

CLI用の`write_text`とsecure-uiの`execution_prompt::Presentation`は同じ内容と選択肢を使います。
GUIは標準ViewKitのDialog / Text / Stack / Buttonで構成し、保護表示へ渡すbindingも保持します。
Buttonから出るのはcorrelationと選択候補です。入力証拠や実行許可を生成する処理はありません。
文字表示は`[n] Don't Allow / [y] Allow Once / [a] Always Allow`です。
`choice_for_shortcut`はbrokerが提示した選択肢だけを復号します。通常のstdin、paste、改行だけを
物理入力の証拠として扱う関数ではありません。

Alwaysはユーザーのeffective UID、認証済みpackage / developer / signing key / provenance、
元ファイル全体のSHA-256、要求Capability集合をすべて結び付けて保存します。UIDは保持中の
native Processと照合したPOSIX credentialsから読み、呼出側が指定するUIDは採用しません。
保存済みの許可があっても、実行ごとのimmutable source検証とmanifest policy選択は省略しません。
別ユーザー、変更されたファイル・署名主体・要求権限には引き継がず、新しい確認が必要です。
一致時も子のimage-installだけを認可し、要求元や他の実行へ権限を追加しません。

保存先はbroker専用の`/var/lib/security/execution-consent.db`（root所有、0600）です。
既存の永続policy用fsync / rename / 復旧処理を共有し、最大4,096件・426,032 bytesに制限します。
version、予約領域、件数、厳密な長さ、並び順・重複、全体checksumを検査します。
checksumは破損検出用であり、改変を防ぐ権限境界はファイルへの書込権限です。
物理入力を伴う承認を消費してから永続化し、保存に失敗すれば実行許可も返しません。
Allow Onceは保存しません。取消し・一覧編集用UIはまだありません。

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
Allow Once / Alwaysのどちらも、対象imageの実行に必要な権限へ縮小した一回の認可を作ります。
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

Always追加後のhost回帰試験は11対象124件（認可63件を含む）が成功しています。
別ユーザー・変更されたfile / signing identity / Capability集合への許可再利用を拒否し、
保存失敗、破損、切詰め、重複、最大件数超過でも既存の許可を広げないことを検査しました。
記録は`out/checks/mochios-consent-377-regression.log`と`mochios-consent-377-host-final.log`です。
377のKVMでは通常Terminalから署名済みfixtureを実行し、Alwaysを一度選んだ後の2回の
実行が追加入力なしで成功しました。子のnet.connect、capabilities.manage / fs.write.all不在、
親のnet.connect不在も検査しています。`mochios-consent-377-result.png`と、root所有0600の
保存ファイルを読み戻した`mochios-consent-377-store.json`が記録です。
378では同じKVM / Data overlayを保ったreset後、通常Dock / Terminal / mshから
`--expect-remembered`で2回実行し、確認画面への追加入力なしで成功しました。
新しいbrokerでも同じ範囲の子だけの許可を使い、親の権限は増えません。
記録は`mochios-consent-378-reset.json`と`mochios-consent-378-reboot-result.png`です。
試験用4ファイルを除去し、最終release buildで通常イメージを再生成しました。
2,654ファイルと4つのcommand symlinkを読み戻し、6成果物のchecksum、SDK / kernelの不変、
System / Data seedに試験用の許可がないことを確認しています。379の通常起動でも
Binder / Dock / menubarを確認し、所有VMを停止しました。記録は
`mochios-consent-377-readback.json`と`mochios-consent-379-normal-desktop.png`です。

Terminal内のmshには、通常のVT / PTYと分離した保護表示を接続しました。表示側は測定済みのTerminalです。
元ファイル、権限選択、実表示・入力の検証、Alwaysの保存はGUIと同じbrokerが所有します。
新しいサービス、kernel専用interface、別の許可databaseは追加しません。

TerminalはPTY masterをCONTROL / TRANSFERへ縮小して添付します。既存FD exportの読み取りmetadata契約で
取得した参照は、Terminal内ですぐ縮小して閉じます。brokerはmasterと新しいnative Process参照をPOSIXへ
明示的に添付し、同じobject、controlling terminal、session、foreground process groupを照合します。
PID / FD / terminal IDの自己申告で関係を作れません。TTYに属するbackground処理もGUIへ迂回させません。

`MCNP` / `MCNX`は16 bytesのpoll / releaseです。pollを待機要求として保留せず、idle時は8 bytesのstatus、
該当時は共通promptを返します。masterと表示Instanceが一致する質問だけを取得でき、登録数は最大16です。
最終判断でもforeground関係を再確認します。表示側がない状態はNotReadyです。

ViewKitは既存windowでも、質問を含むframeの保護bindingをcommit前に登録します。compositorは認証済みの
Window登録に結び付く間だけ、対象surfaceをshell panelより上・secure overlayより下へ配置します。
CPU / GPUの合成、backdrop、入力判定、保護表示の重なり検査が同じ順序を使います。取消し、focus / owner /
sessionの失効で昇格を解除し、通常のsurface作成では取得できません。昇格だけでは入力をarmしません。
内容generationが新しいframeの実表示も引き続き必要です。失敗理由はReleaseでもloggerへ記録します。

`y` / `a`を選択した後、新しいEnterで確定し、`n` / Escapeで拒否します。brokerは独立したInput Authorityの
physical shortcut → 全キー解放 → Enterを検査します。選択と確定の種類も一致が必要です。TextInput、IME、
paste、stdin、PTY buffer、repeat、修飾キーのchordを肯定の証拠にしません。判断キーの解放まで通常入力を
遮断し、同じ押下から出る文字をPTYへ流しません。ViewKitはcodepointがない文字キーのreleaseも復元します。
表示は標準ContentArea / Stack / Text / Scrollで構成し、長い一覧をscrollしても選択案内を下部へ残します。
focus喪失、tab変更、表示領域外の操作、window終了は取消しです。stdinだけの端末やSSH向けの測定済み表示
アダプタは未実装で、任意のstdinのyへフォールバックしません。

関連host試験は698件成功しました。compositorの51件成功・1件ignoreを含みます。記録は
`mochios-text-consent-380-{host-final,input-regression}.log`、`mochios-text-consent-381-controller-host.log`、
`mochios-text-consent-384-{lifecycle-host,compositor-harness-host}.log`、`mochios-text-consent-385-key-release-host.log`です。
harnessなし実行体の誤指定は停止し、正しいtest targetの結果だけを数えています。

381のKVMではguest clangに`-fintegrated-cc1`を指定し、LLDによるリンクとmsignの自動署名まで成功しました。
通常指定のclangは途中で中断しており、この試行の完走は主張しません。同じguest生成ELFを384で実行し、
質問 → y / Enter → 子のnet.connectだけの取得、再度の質問、nによる拒否を確認しました。
`mochios-text-consent-381-integrated5.png`と`mochios-text-consent-384-{question,once,question-again,deny}.png`が記録です。
通常echoのpipelineは質問前の別経路で拒否されたため、入力保護の成功例には数えません。
385のnative試験では、stdinへ先にy / aを投入しても入力待ちを続け、物理nで拒否できることを確認しました。
`mochios-text-consent-385-{prefed-waits,result-final}.png`が記録です。nの文字もPTYへ流れません。
この試行のAlways段階は時間切れになったため、成功として数えていません。
新しい試行ではaを選んで新しいEnterで確定し、その後の二回が追加入力なしで成功しました。子の終了値37、
net.connectだけの取得、親の権限不変も検査しています。`mochios-text-consent-385-complete.png`が記録です。
root所有0600の保存ファイルを読み戻し、同じData overlayを保った再起動後にも、新しいbrokerで二回の
無入力実行が成功しました。`mochios-text-consent-385-{store,reset}.json`と`mochios-text-consent-385-reboot-pass.png`に記録しています。

試験用6ファイルを除去し、通常イメージの2,654ファイルと4つのcommand symlinkを読み戻して照合しました。
6成果物のchecksum、SDKの6入力とkernelの不変を確認し、System / Data seedの許可記録・一時保存・旧版が
すべて存在しないことも検査しました。`mochios-text-consent-380-{artifacts,readback}.json`と
`mochios-text-consent-386-{normal-image,readback}.log`が記録です。
386の通常起動でもBinder / Dock / 上部バーを確認しました。
`mochios-text-consent-386-normal-desktop.png`に記録し、所有VMを停止して検証用の一時停止処理も解除済みです。
