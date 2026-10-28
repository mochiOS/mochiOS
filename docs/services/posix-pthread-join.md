# pthread_joinの結果と回収権（phase270）

joinの回収権を、既存callback_resources / io_resourcesで追跡するprocess-privateなClaimへ
移しました。通常returnでの破棄に加え、ThreadExit、キャンセル、siglongjmpなどでC callbackが
待機フレームを放棄した場合にも、アプリのcleanupを呼ぶ前にjoin_waiterと回収権を戻します。
単なるRustのDropに依存せず、既存の非return遷移のabandon処理へ接続しています。

結果ポインタは直接dereferenceせず、既存MemoryCopyを使うuser_copyへ渡します。
結果の書き込みが成功するまで対象スレッドを回収しません。書き込み失敗ではエラーを返し、
対象の終了結果を保持して回収権を戻すため、別の有効な出力先で再度joinできます。
NULL出力は結果を破棄してjoinし、非整列出力も検査付きcopyで扱います。
結果の書き込みと回収の確定中はnative callbackを遅延しますが、待機中は遅延しません。

## forkと待機

Claimはnative Handleを所有せず、親processのfork epochと対象管理領域だけを保持します。
そのため、安定した待機中のClaimはforkを妨げません。対象への観測・更新はcallbackを
遅延した区間で行い、kernel status観測の一時的な参照は従来の専用guardで保護します。

ハンドラがforkして子へ戻った場合、元のjoin対象は生存していません。コピーされたClaimは
epochの変化を確認してESRCHで終了し、再利用された子側の管理領域を変更しません。
forkの子側処理では、呼び出しpthreadに残った別スレッドのjoin claimも取り除きます。
detach済みの状態は変えません。

forkの子で既存の呼び出しpthreadの通知wordを更新します。待機条件を捕捉した後、native waitへ
入る前のcallbackでforkした場合も、コピーされた古いexpected値では待機し続けません。
親の通知wordや回収権は変更しません。

kernel ABI、POSIX service protocol、Capabilityは変更していません。
待機は引き続きnative WaitOnAddressを使い、POSIXのpthread状態やfork epochはruntime内で
扱います。SDKも再生成していません。

## 検証

ホストでは本番join、callback資源追跡、検査付きcopyを実行しました。回収前の出力失敗、
待機のエラー、生成完了待ち、通常・非同期のframe放棄、copy後の回収、fork後のepoch不一致を
確認しています。forkが待機の直前に起きる順序と、子側で別のjoin claimが存在する順序も
再現し、古いClaimが子側の状態を変更しないことを確認しました。

`services/posix/tests/native_pthread_join.c`を既存診断managerからfork/exec/waitで実行し、
UID 0と501の両方が終了status 0でした。次を実際のnewlib APIで確認しました。

- 無効・PROT_NONE・読み取り専用・ページ境界の出力失敗と、同じ対象への再join。
- 非整列出力と、繰り返すthread生成・joinでのTLS初期値。
- 通常・非同期キャンセル中のjoin放棄と、cleanupからまだ生存中の対象への再join。
- join中のsiglongjmpから戻った後の再join。
- ハンドラ内fork後の子側ESRCH、子での新しいpthread生成、親側joinの継続と子のwait。

関連7 targets / 33 host testsとstandalone runtime・C probe・診断/通常managerのbuildが
成功しました。診断用の配置を除去した通常イメージでinput READY、Binder、デスクトップ、
DockからのFiles起動、Home表示とウィンドウを閉じる操作を確認しました。
閉じる操作にはフォーカス後の再クリックが必要で、既存のUSB driver errno 22と
NIC不在のnetwork Failed(-6)も残ります。UI全体・全サービス正常を証明したものではありません。
実行した2回のKVMは自分のPIDを照合して停止しました。一時停止対象は両方とも0件で、
再開記録を保存しています。

証跡は`out/checks/mochios-pthread-join-270-`をprefixとするhost、build、native/normalのログと
`artifacts.json`です。完全なPOSIX環境のgoalは継続中です。
