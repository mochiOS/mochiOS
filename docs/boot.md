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
Permission presenter prewarm
IME dictionary warmup (background)
resident
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

許可画面の事前準備は認証済みセッションの公開後に行います。セッションの公開に失敗した場合は
許可画面の準備と辞書の自動読み込みを開始せず、公開エラーを記録します。

