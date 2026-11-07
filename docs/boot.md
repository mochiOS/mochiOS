# Bootとサービス起動

ブートモジュールv5では、プラットフォームが作った初期スタックを`MIMG`形式でELFと共に渡します。
`pack-boot-module.pl`がCランタイム向けのELF補助情報を用意し、カーネルは既存の
stack / ELF relocationだけを適用します。補助情報のキー、サービス名、パスをカーネルで決めません。
初期スタックは省略可能で、その場合は空のnative entry pageを使います。Cランタイムを使う
通常の生成物では省略しません。初期プロセスへのHandle更新や追加mappingは受け付けません。
v4以前のモジュールは再生成が必要です。初期スタックがあってもbootstrapは直ちに開始し、
通常の`ProcessCreate`で作る停止中の子プロセスとは区別します。

`core.service`は`logger.service`、`capability.service`を起動します。Capability policy適用後、
`capability.service`が`signature.service`、`package.service`、`service-manager.service`を起動します。

`service-manager.service`の固定起動順は次です。

```text
filesystem.service / Ready
posix.service / Ready / endpoint handoff
drivers.service
Driver HELLO
mboot-agent.service
input.service
display.driver
display Ready
input Ready
compositor.service
Driver discovery (asynchronous)
network.service
update.service
user.service / Ready
keychain.service / Ready
secure-ui.service / login
workspace.service
linux.service
Binder.app / first frame
Desktop notification
Driver discovery complete
network Ready
Session authority publication
Permission presenter preparation (asynchronous)
resident / Execution presenter preparation (asynchronous)
Permission preparation attempt finished / IME dictionary warmup (background)
```

初回インストールではloginを省略し、Binderの初回描画後にInstallerを開きます。
`update.service`はnetworkの起動後、Readyの待機より前に起動します。起動または同期の失敗はbootを止めず、保存済み
Developer PKI databaseを使用します。`signature.service`はnetwork Readyより前に起動し、ローカルDBを
読み込みます。新しいDBがない場合もserverとして常駐しますが、新規MPKG installはfail closedです。

正式な`make smoke-test`はサービス起動順、重複起動、Ready順、Driver探索、network lifecycle、
`update.service`起動、panic/exception不在をログ行番号と出現回数で検証します。xHCIがbuild設定で無効な
場合はUSB bundleを要求せず、PS/2とvirtio-net以降の順序を引き続き検証します。

起動時間はQEMUプロセス開始からBinderの初回描画通知までを計測します。自動ログインの開発設定では
KVM・CPU host・2 vCPU・1 GiB RAM・VirGL有効で10秒以内を目標とします。ビルド時間と、人による
ログイン操作の待ち時間はこの値に含めません。描画通知に加え、ポインター移動やキーボード操作が実際の
画面に反映されることも確認します。表示方式とホスト負荷を記録し、計測用VNCを併用したGTK表示と
通常のGTKだけの起動を混同しません。

基本入力は辞書の準備と独立してReadyになります。日本語辞書は初回描画とセッションの公開後に背景で読み込み、
それ以前でも日本語モードへの切り替えや変換要求があれば準備を開始します。辞書の準備完了は別に記録し、
準備中もマウスとキー入力の受付を続けます。この起動順はユーザー空間で決め、カーネルにはサービス名や
起動パスを追加しません。

許可画面の事前準備は認証済みセッションの公開後に行います。権限確認用と実行許可用の
presenterはそれぞれ別のワーカースレッドで起動し、そのスレッド専用のIPC受信先でReadyを待ちます。
サービス管理ループは完了した結果だけを受け取り、事前準備の間も通常の要求を受信します。
Readyにはtokenと保持中のProcessHandleによる送信元確認を適用します。失敗時や結果の受け取り先を
破棄した場合も、作成したプロセスの参照を終了・POSIX登録の後始末まで保持します。
再起動の確認は1秒間隔とし、後始末が残っていれば新しい起動を延期します。

辞書の自動読み込みは、初回の権限確認用presenterの準備試行が終わってから開始します。
これは準備失敗でも開始しますが、Readyとみなしたり許可を与えたりするものではありません。
明示的な日本語入力の要求はこの順序を待ちません。セッションの公開に失敗した場合は
許可画面の準備と辞書の自動読み込みを開始せず、公開エラーを記録します。
実際の許可要求への応答は従来通りモーダルで待ちます。

起動中のReady通知、Driver探索完了、通常のIPC要求は同じ受信キューへ届きます。
別の起動待ちが先に受け取った要求は保存し、探索完了待ちでは保存済みの要求も確認します。
送信元は保持中のProcessHandleとカーネルが返す送信endpointの所有プロセスを照合し、
payload内のPIDでは判定しません。他プロセスからの要求とendpointの公開は保持したまま、
既存のrequest ID検証、子プロセスの終了監視、元の待機期限を適用します。
これにより、受信済みの探索完了通知を再度待って期限切れになる遅延を防ぎます。

2026-11-07のKVM検証では、初回描画からセッション公開までの待ちが約10秒から
約0.02秒になり、探索完了の受信・検証成功もシリアルで確認しました。2 vCPU・1 GiB・
VirGL有効で、ホストの並行ビルドを許可に基づき一時停止した回の初回描画は9.38秒でした。
ホスト負荷で起動全体の時間は変動するため、10秒以内を常に達成したとは扱いません。

透過パネルの操作領域は描画されたピクセルだけに依存させず、Dockの表示中の操作範囲、
非表示中の再表示用の下端、開いているランチャーの範囲を明示します。KVMでDockの再表示、
Filesの起動、ランチャーのクリックによるEditの起動、英字入力、日本語変換候補の表示を
確認しました。記録は`out/checks/startup-interaction-20261107/final-dock-reveal/`です。
別の負荷の高い起動でも、IME辞書の準備完了前にApp Storeの検索欄へ英字を入力し、
入力内容と検索結果が画面へ反映されることを確認しました。記録は
`out/checks/startup-interaction-20261107/final-warm-text/`です。

事前準備の非同期化とランチャーのフォーカス修正後のKVM検証では、初回描画7.90秒、
通常要求の受信開始7.92秒でした。権限確認用presenterのReadyは9.16秒、実行許可用は
10.49秒で、受信開始が両方の準備完了より先になりました。辞書の読み込み開始は9.16秒、
完了は21.67秒です。DockからのFiles起動、ランチャーの文字検索とEnterからのEdit起動、
英字入力、日本語変換候補の表示を確認しました。
記録は`out/checks/launcher-focus-20261107/keyboard-restoration/`です。
この回の英字入力試験は辞書の準備完了後なので、辞書準備中の入力確認とは扱いません。
初回描画の通知だけで全操作の準備が完了したとは扱いません。

ランチャーはViewKitの`EventContext::set_keyboard_capture`で、表示中だけパネルへ
キーボード入力を取得します。閉じると元のウィンドウへ戻し、フォーカスを別の画面に
奪われた場合は取得を破棄します。コンポジターはパネルの所有権と`FOCUS_CONTROL`を確認し、
保護された許可画面・ロック画面・システムモーダルの表示中は取得とシェルのショートカットを
拒否します。Dockとメニューの通常操作は取得しません。カーネルの変更はありません。
KVMでFilesとEditが前面の場合に検索できること、検索中の文字が本文に入らないこと、
EscapeとAlt+Spaceで閉じた後にクリックせずEditへ入力を続けられることを画面で確認しました。

前回の関連ホスト試験は71件、今回の非同期準備・起動順・所有権・Ready検証は54件の実行が
成功しました。パネルのキーボード取得・既存入力領域・ViewKit経由の拒否通知・ランチャーの
終了処理の関連試験は25件が成功しました。既存の`ipc_empty_native`にはソース形式と
返却タプルに追従していないfixtureがあり、2件は未通過です。全体の試験成功とは扱いません。
検証結果と未解決点の一覧は`out/checks/startup-interaction-20261107/evidence.json`に記録しています。
非同期化の結果は`out/checks/presenter-async-20261107/evidence.json`に記録しています。
フォーカス修正の結果は`out/checks/launcher-focus-20261107/evidence.json`に記録しています。

