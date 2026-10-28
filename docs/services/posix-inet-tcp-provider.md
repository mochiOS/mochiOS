# 権限付きTCP Provider（phase274）

network.serviceの通常起動とNICなし起動の両方で、TCPループバックをnative Handleから操作できるようにしました。
前段のresource backendを汎用Providerへ接続したもので、mnuのABIは追加・変更していません。
POSIXの`AF_INET/SOCK_STREAM`、newlib、Rust stdのFD経路への接続は引き続き未完了です。
また、このProviderのroutingはIPv4 loopback（127/8）までで、物理interfaceへの接続は残っています。

## 権限の取得と委譲

`user/crates/net-device-protocol/src/stream.rs`にversion 1のwire形式を置きました。
要求は`NTCP`、応答は`NTCR`を識別子とし、未知opcodeは`-38`を返します。
version、長さ、予約領域、scope、応答statusを検査します。PID、FD、内部resource IDは要求に載せません。

- `ACQUIRE`はkernelが保証するIPC senderについて、要求したscopeに対応する既存の`net.connect`と`net.listen`を別々に確認します。
- factoryはCONTROL / DUPLICATE / TRANSFERだけを持ち、`OPEN`へCONTROLだけを添付してresourceを作ります。
- `DERIVE`は添付元factoryのscopeの部分集合だけを持つ別factoryを作ります。保持済み権限はCapabilityのdrop後も利用できます。
- `VALIDATE`、`OPEN`、`DERIVE`、`ACCEPT`は、信頼するnetwork endpointで添付Handleをidentifyします。任意のProviderを呼び出して信頼性を調べる方式にはしません。
- 待ち受けはLISTEN scope、接続開始はCONNECT scopeを必要とします。明示的なbind先portは1024以上に制限し、POSIX uid 0でもscopeやport制限を迂回しません。

factoryから作ったresourceは作成時のscopeを保持し、factoryの参照破棄後も独立して使えます。
acceptにはlistenerのCONTROL参照を添付し、受理した接続を別のnative Handleとして返します。
factoryとresourceの内部IDは型のnamespaceを分けて単調増加させ、解放済みIDを再利用しません。
通常IPCでProviderの要求を模倣しても、native-object delivery metadataがなければ操作・releaseを拒否します。

## 操作と寿命

resourceはREAD / WRITE / CONTROL / WAIT / DUPLICATE / TRANSFERを持ちます。
データはnative HandleRead / HandleWriteから扱い、HandleReadAtのoffset 0はpeekとして扱います。
非blockingな部分write、接続開始、半切断、close、待ち受け・接続・受信のreadinessはresource backendの状態を使います。
local/peer/statusの観察はREAD権限のHandleQueryで行い、エラーを消費する操作はCONTROLに限定します。
memory backingは持ちません。

複製された参照は同じresourceを指し、最後の参照のnative releaseでbackendを閉じます。
listenerのcloseでaccept済み接続は閉じません。FIN交換と放置された接続の回収はbackendの期限で処理します。
受信bufferの排出や状態変化の後は、応答前にreadinessのlevelを更新します。
native Handleの作成失敗をreadyなacceptの取り出しより先に処理し、応答送信失敗でも発行したresourceを回収します。

## 検証と残り

`services/posix/tests/network_stream_provider.rs`は本番のfactory・Provider・TCP backendを使い、nativeのIPC/Handle機構だけを試験用に置き換えます。
scopeの分離と縮小、異種/不正Handleの拒否、偽release、失敗時の回収、acceptの保持、データ・peek・readiness・半切断、wire形式を確認します。
TCP・UDPの既存試験を合わせて9 targets / 113 testsと、network.service・診断managerのmochiOS向けbuildが成功しました。

`native_tcp_provider.rs`はKVMで実際のIPC添付、Handleの複製・権限縮小、Capabilityのdrop、接続・accept・双方向通信・peek・readiness・半切断を検査します。
euid 0 / 501の両方で成功し、foreign Providerの拒否と、native deliveryを偽装できないことも確認しました。
同じ診断managerで既存UDPのfactory・Provider・newlib socket経路も成功しました。
診断には通常のmanifestとmsignの検証記録の生成経路を使い、診断用の権限・managerは除去済みです。
通常設定で再起動し、input READY、Binder、network endpointの引き継ぎ、FilesのHome表示とcloseを確認しました。
全VMは停止し、hostのrust-analyzer一時停止記録も再開済みです。SDKは再生成していません。

証跡は`out/checks/mochios-tcp-provider-274-host-final.log`、`-network.log`、`-manager-combined.log`、
`-native-combined.log`、`-normal.log`、`-artifacts.json`です。

POSIX側のsocket descriptionのworker/FD接続、factoryの添付、非blocking connect、bind/listen/accept、SO_ERROR、shutdown、
dup/fork/execとFD flagの処理、readinessの中継、物理NICでのTCP Providerを接続する作業が残っています。
完全なPOSIX互換環境のgoalは継続中です。

## 共有descriptionの状態処理（phase275）

`services/posix/src/inet_stream.rs`へ、TCPの共有descriptionとして使う状態処理を追加しました。
接続待ち・成立・失敗・待ち受け、nonblocking flagとtimeout、半切断、readiness、所有する下位参照のcloseを扱います。
CLOEXECとFD番号は保持しません。acceptしたdescriptionのnonblocking flagは要求で決め、listenerのflagをコピーしません。

SO_ERRORは共有pending errorを一度だけ消費し、開始済みのconnectの結果は別に保持します。
この分離は[SO_ERRORの取得・クリア](https://man7.org/linux/man-pages/man7/socket.7.html)と、
[非blocking接続の完了確認](https://man7.org/linux/man-pages/man2/connect.2.html)を踏まえたものです。
下位の失敗通知を消費した後に不正な応答が返っても、その接続を成功へ昇格させません。
peerやlocal addressが成立した接続として不正な場合も、失敗状態を保持します。

`inet_stream_description.rs`の13件は、本番のTCPループバックtransportをadapter越しに使い、
pending connect、SO_ERRORとIOによる消費、peek・空read、半切断、accept後の独立した寿命、
不正応答の拒否、仮の参照の回収を確認します。既存TCP・UDPと合わせて10 targets / 126 testsと
POSIX serviceのmochiOS向けbuildが成功しました。

この状態処理はまだsocket workerから呼び出しておらず、新しいFD経路を提供した段階ではありません。
次はNative backend、workerでのfactory受理・description管理、client/newlib側の接続を進めます。
payloadはnativeのデータ経路を使い、共有するPOSIX状態とFDごとの状態を混同しない構成にします。
この段階ではIPC wire、kernel ABI、SDK、OSイメージを変更せず、KVMも起動していません。
証跡は`out/checks/mochios-tcp-description-275-host-final.log`、`-posix-final.log`、`-artifacts.json`です。
