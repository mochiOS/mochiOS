# pthread生成と回収の順序（phase269）

native ThreadCreateは親のsyscall復帰前に子を実行できます。従来は子が終了してdetach済みの
管理領域を別スレッドが回収すると、その後に復帰した親が同じ領域へnative IDを書き込む
余地がありました。また、thread IDの出力をnative生成後に直接dereferenceしていました。

生成を準備するruntimeは管理領域の`creation_pending`を立て、ID出力を既存MemoryCopyで
完了してからnative ThreadCreateを呼びます。出力失敗では子を作らず、確保済みのstack、
reent、TLS、signal stackを既存の回収経路へ戻します。native生成が成功した場合、
native IDを反映した後の最後の管理領域更新として生成中の印を外します。
生成前にIDを出力するため、その後のnative生成失敗時の出力値は使用できません。
失敗時に出力値が未規定となるpthread_createの契約に従います。
[Oracleのpthread_create仕様](https://docs.oracle.com/cd/E19455-01/806-0630/6j9vkb8fv/index.html)。

joinとdetach済みスレッドの回収は、生成中の管理領域を解放しません。detach回収は、
回収権の取得後にも終了状態と生成中の印を再確認します。最初の観測と回収権の取得の間に
別スレッドが領域を回収・再利用しても、新しい生成途中の領域を解放しないためです。
native Thread Handleで実際の終了を確認してからmappingを回収する条件も維持します。
子自身によるdetachを親の完了処理で上書きしません。

追加した状態はprocess-localなpthread runtimeのものです。kernelはpthread、join、detach、
POSIXのthread IDを解釈せず、既存のThreadCreate、Thread Handle、MemoryCopyを提供します。
kernel ABI、POSIX service protocol、Capability、SDKは変更していません。

## 検証

ホスト試験では、本番の生成末尾とdetach回収を実行し、次を確認しました。

- 出力失敗でnative ThreadCreateを呼ばず、準備済み資源を一度だけ回収すること。
- 子がnative ThreadCreateの復帰前に終了しても、親が使う管理領域を回収しないこと。
- 回収権を取る直前の別世代への再利用でも、取得後の再確認で早期回収を防ぐこと。
- joinが生成完了を待ち、その待機中のキャンセルでjoin claimを戻すこと。

`services/posix/tests/native_pthread_publication.c`をKVMで実行し、UID 0と501の両方で
無効・PROT_NONE・読み取り専用・ページ境界の出力先、PROT_NONEの属性入力、非整列出力、
4 creatorによるdetach済み/自己detachの子の並行生成、管理領域の反復再利用、
TLS初期値、fork後のpthread生成とjoinが成功しました。
既存の診断managerからfork/exec/waitで呼び出し、両方とも終了status 0でした。
関連8 targets / 29 host testsと、standalone runtime・C probe・診断/通常managerのbuildも
成功しました。

診断用バイナリを除去した通常イメージでinput READY、Binder、デスクトップとDock、
FilesのHome表示と閉じるボタンによる終了を画面で確認しました。閉じる操作には
フォーカス後の再クリックが必要でした。起動時のnetwork診断文字の一時的な描画、
NIC不在のnetwork Failed(-6)、既存USB driver errno 22は今回修正していません。
全サービス正常やUI全体の正常性を証明したものとは扱いません。
試験に使った3回のKVMはすべて自分のPIDを照合して停止しました。
一時停止手順の対象は3回とも0件で、再開記録も保存しました。

試験ELFは既存SDKとstandalone runtimeを使い、PT_TLSを出力する既存
`user/runtime/linker.ld`でリンクします。最初にSDK同梱の古いscriptを使った試験は
PT_TLSを欠きTLS検査で失敗しました。試験側のリンク指定を直し、build scriptへ
PT_TLSの確認を追加した後の実行結果と区別して保存しています。SDKは再生成していません。

証跡は`out/checks/mochios-pthread-publication-269-`をprefixとするhost、build、
native/normalのログと`artifacts.json`です。完全なPOSIX環境のgoalは継続中です。
