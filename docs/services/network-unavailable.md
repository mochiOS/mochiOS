# ネットワークの機器不在時の応答

## 原因と修正

static PIEの通常起動確認で見つかったnetwork.serviceのREADY timeoutを調査しました。
使用したKVM runnerは`network = "none"`で、NICを付けない設定です。
従来のvirtio-net.driverは初期化に失敗するとIPCを受け付けずyieldを続けていました。
network.serviceは存在するdriverを発見できたため、最初のinterface問い合わせで待ち続けていました。

PCI探索は「該当deviceなし」とtransport初期化失敗を分けます。
driverは機器不在をwire status `-6`で返し、初期化やqueueの失敗後もIPC要求に応答します。
interface情報と統計の失敗もstatus-only responseで返し、clientはopcode・request ID・
負のstatusを検証します。queueが失敗してもdeviceとDMA所有権を維持します。

raw driverの要求は、認証済みIPC senderの`net.raw`で判定します。
network.serviceも失敗状態でTCP / TLS / HTTPそれぞれのCapabilityを確認し、
許可された要求には各operationのresponse形式で失敗を返します。
READYには失敗statusを通知し、成功を偽装しません。

one-way requestにはcallのreply権限がありません。
network.serviceはnative `AccessDenied`の場合に限り、認証済みsenderへのsendでcompletionを返します。
旧errno `-13`との比較、PeerClosedなど別のエラーからの再送は行いません。
この修正にカーネルABIの追加やSDKの再生成はありません。

## 検証

`services/posix/tests`内のnetwork_driver_service、network_driver_startup、network_reply、
network_error_statusで、失敗応答・不正frame・request ID・DMAの維持・権限拒否・
one-way completionの条件を確認します。

native_network_unavailableはNICを付けないKVMで実際のIPCを使います。
raw driverの権限拒否、TCPの通常call、TCPのone-way completion、TLS / HTTPの型付きエラー、
Capabilityをdropした後の拒否の5 groupsが成功しました。
診断用compositorの終了後のCompositorNotFoundは試験構成によるもので、
このログだけを通常デスクトップの起動成功とは扱いません。

ホスト全体回帰は682 targets、3,310件成功・失敗0件・8 ignoredでした。
試験用のcompositor・service-manager・manifest・verificationを元へ戻した通常起動で、
input READY、BinderとDockの表示を確認しました。
network.serviceの通知は従来のTimedOutから機器不在を表す`Failed(-6)`に変わっています。
最初の通常起動では全体テストと並行してinputのIME初期化がREADY期限を超えました。
そのログもnormal-initial.logとして残し、全体テスト終了後の同一イメージの再起動で
input READYを確認しています。負荷が原因だと断定したものではありません。
全サービスの成功扱いにはせず、NIC不在と既存USB driver起動失敗もログに残しています。

この試験はネットワーク機器不在時の完了と権限境界の検証です。
実NICでのDHCP・パケット送受信や、POSIX sockets全体の完成を示すものではありません。
NICを動作中に再接続する処理は含まず、再初期化にはサービスの再起動が必要です。

記録は`out/checks/mochios-network-unavailable-205-`で始まるbuild、host、native、normalログです。
POSIX互換環境全体のgoalは継続中です。
