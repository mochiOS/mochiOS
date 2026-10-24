# ネットワーク権限を明示的なHandleとして渡す境界

## 実装した範囲

network.serviceに、認証済みsenderがnet.connectを持つ場合だけ限定したProvider Handleを
発行する処理を追加しました。通信機器の初期化とは分離し、NIC不在の失敗応答loopでも
同じ発行・問い合わせ・解放経路を処理します。

別アプリのendpointを示して、そのアプリのCapabilitySetを借りるAPIは作っていません。
要求にPID・FD・object IDは含めず、発行後の権限はIPC添付のHandleそのもので渡します。
発行元を偽装するための自己申告identityを受け付けません。

HandleのrightsはCONTROL | DUPLICATE | TRANSFERだけです。
READ / WRITE / SEEK / MAPや、プロセス全体への権限はありません。
今回はQUERY_LIMITSによるIPv4 backendの範囲照会とnativeの複製・転送・最終解放に対応します。
UDP socketの作成やpacket送受信に接続した完成済みfactoryではありません。
今後は、この明示的な権限を添付してPOSIX層からbackend資源を取得する経路へ接続します。

## 認証と寿命

acquire時はkernelが保証したIPC senderに対してnet.connectを検査します。
通常のendpoint参照とprocess-local endpoint Handleのdeliveryに対応します。
Providerの操作はIPC_DELIVERY_NATIVE_OBJECTだけを受け付けます。
raw IPCでProvider形式のbytesと正しいobject IDを送っても、照会やreleaseを許可しません。

保持済みの権限Handleを操作するとき、保持者のambient net.connectを再び借りることはありません。
権限を落とすと新規発行は拒否されますが、明示的に保持・委譲したHandleは寿命の間利用できます。
複製は同じobjectを参照し、最後のnative参照が閉じられたときにregistryから退役します。
IDは再利用せず、返信失敗後に届いた遅いreleaseも新しいobjectを消せません。
発行数は全体64件に制限し、登録・Handle作成・返信の各失敗で未公開の資源を残しません。

network.serviceの受信をHandle添付対応へ変更しました。
通常の既存要求は空のattachmentで処理し、受け取った不要・不正な添付も共通処理でcloseします。
kernel ABIやkernel内のservice固有policyは追加していません。

## Protocol

net-device-protocol/datagram_authorityに、version付きacquire、status response、固定長limitsを
定義しました。Handle値はpayloadにserializeせず、native IPCの添付領域だけを使います。
未知のacquire opcodeは-38、不正な長さ・添付は-22、権限不足は-13です。
limitsは現在minimum port 1024、maximum payload 1472、IPv4 backend tagを返します。
これらは将来の限定した資源発行の範囲であり、POSIX uid判定をkernelへ移したものではありません。

## 検証

services/posix/tests/network_datagram_authority.rsは実際のserver実装を使って、
認証、rights、偽造query/release、quota、発行・返信失敗、stale IDとwireを検証します。
関連7 targets、37件が成功しました。network.serviceのtarget buildと、通常のworkspaceが
ローカルprotocolへ解決されることも確認しています。生成されたlockは保存します。

KVMのnative_network_unavailableでは、実際のHandle転送・複製、READ権限の拒否、
raw Provider要求の拒否、128回の発行とclose、CapDrop後の新規拒否と保持済みHandleの照会が
成功しました。既存no-NIC試験の5 groupsと、権限Handle試験の2 groupsが成功しています。
試験のendpoint探索は診断用で、POSIX向けの実アプリのbootstrap配布はまだ実装が必要です。

記録はout/checks/mochios-network-authority-209-final-host.log、-build.log、
-probe-build.log、-native.log、-normal.logと.png、-evidence.jsonです。
通常構成へ戻した後もinput READY、Binder、デスクトップとDockを確認し、VMは停止済みです。
NIC不在のnetwork Failed(-6)と既存USB driver起動失敗は残っており、全サービス成功とは扱いません。
SDKとRust std overlayは再生成していません。

## 残る実装

このHandleを添付してUDP資源を作る処理、datagram Provider、受信Event、POSIX AF_INETの
create / bind / connect / sendto / recvfromへの接続、endpointのbootstrap配布が必要です。
実NICでの通信やInternet socket全体の完成は、今回の試験からは主張しません。
POSIX互換環境全体のgoalは継続中です。
