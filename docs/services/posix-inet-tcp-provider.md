# 権限付きTCP ProviderとPOSIX socket

network.serviceの通常起動とNICなし起動の両方で、TCPループバックをnative Handleから操作できるようにしました。
前段のresource backendを汎用Providerへ接続したもので、mnuのABIは追加・変更していません。
phase274のnative Provider、phase275の共有状態を、phase276でPOSIXの`AF_INET/SOCK_STREAM`とnewlibのFD経路へ接続しました。
phase277で、既存のARP・IPv4・NIC送受信と同じTCP接続表へ接続しました。IPv6は未対応です。
外部通信のnative検証については末尾のphase277の範囲を参照してください。

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


## POSIXのFD経路への接続（phase276）

`inet_stream_native.rs`が、委譲されたfactoryと信頼するnetwork endpointからnative TCP resourceを取得します。
endpointも権限を限定してduplicateし、借用元のHandle番号が再利用されても別endpointへ接続しません。
acceptしたbackendは独立したendpoint参照を持ち、listenerのcloseで利用不能になりません。
不正な応答は拒否し、受信した異なる参照を一度ずつ回収します。

`inet_stream_socket.rs`と既存socket workerが、共有descriptionとconnect通知を管理します。
O_NONBLOCK、SO_ERROR、送受信timeout、shutdownはdescriptionに置き、FD番号とCLOEXECは従来どおりclient側に置きます。
Providerの型を表す内部IDはstatのinodeへ流用せず、TCP用の仮想deviceとdescriptionの連番で表します。
データはclientのHandleRead/WriteからProviderへ入り、networkのnative resourceへ渡します。
FD番号を持つread/write RPCは追加していません。この経路はサービス間の転送を含み、性能測定は未実施です。

libcのsocket()は呼び出し元自身が保持するnet.connect / net.listenの範囲でfactoryを取得し、明示的に添付します。
POSIX workerの権限でクライアントを代理認証しません。
非blocking connectの継続はEINPROGRESS、継続中の再試行はEALREADYとして扱います。
blocking connectの通知は対象descriptionの参照を保持し、最後の通知参照を閉じると未成立の接続だけを中断します。
期限切れと接続成立の競合はnative transportのCANCEL_CONNECTで判定し、成立済みの接続や先に確定した失敗を優先します。
SO_ERRORの消費後も、開始済みconnect通知の結果は失いません。

C側のIPv4アドレス型はuser/libc-port/include/netinet/in.h、byte order変換はnewlib runtimeに追加しました。
利用したSDKとRust std overlayは再生成していません。

`inet_stream_native.rs`のホスト試験は本番adapter・network Provider・TCP transportを通します。
置き換えるのはnative Handle参照、権限検査とIPC配送だけです。
`inet_stream_description.rs`では通知の参照寿命、タイムアウトと成立の競合、キャンセル、偽Provider要求、
accept前の準備失敗、stat、readinessの排出も確認します。

phase276の検証は11 targets / 137 host testsと、POSIX・network・診断managerのmochiOS向けbuildが成功しました。
`native_inet_stream.c`を既存SDKと単体runtimeでリンクし、KVM上のeuid 0 / 501で次を確認しました。

- Cのsocket/bind/listen/connect/accept4と双方向read/write、peek、MSG_WAITALL、半切断。
- O_NONBLOCKの共有、descriptorごとのCLOEXEC、SO_TYPE/SO_ACCEPTCONN/SO_ERROR、fstatの型と所有者。
- backlogが満杯の間のEINPROGRESS/EALREADY、accept後の再送とpoll起床。
- blocking connectの期限切れとエラーの一度だけの消費、接続待ちに入った後のpthreadキャンセル。
- dup後の参照寿命、forkでの共有、execを越えるHandle継承とCLOEXEC参照の破棄。

同じKVM起動中に既存libc UDP、factory/worker、同一portの64回の最終close・再open試験も成功しました。
証跡は`out/checks/mochios-tcp-socket-276-host-final.log`、`-targets-final.log`、`-final.log`です。
診断用managerのbinary・manifest・verificationは試験前の内容へ戻し、仮の実行ファイルを削除しました。
network.serviceとposix.serviceは試験済みの内容を通常イメージへ残しました。
通常イメージでもinput/Binderの起動、FilesのHome表示と閉じる操作を画面で確認しました。
試験したKVMは停止し、一時停止の記録もすべて再開済みです。

残りは、このProviderの物理NICへの接続、IPv6、bufferのサイズ変更や追加socket option、
広いTCP適合試験、Rust stdを直接使ったTCP試験、スループットと遅延の測定です。
この接続だけでPOSIX全体や外部ネットワーク対応が完了したとは扱いません。


## IPv4 interfaceとの接続（phase277）

`stream::Transport`の接続表をNetworkStackのTCP接続表として共有します。
Providerはfactoryとresourceの参照を管理し、物理interfaceありの場合はstackのtransportを借用します。
NICなしの場合は独立した同じtransportを使い、loopbackを維持します。
legacy HTTP/TLS経路とnative resourceが別々のportを予約してしまう構成を解消しました。
TCBのowner labelも異なる領域へ分け、legacy APIからnative resourceを操作・破棄できないようにしています。
これらの番号はユーザー空間内の識別子で、kernel ABIやFD番号ではありません。

native TCPの送信待ちは既存のARP・IPv4 routingへ渡し、受信は既存のEthernet/IP/TCP検証から共通の接続表へ入ります。
物理向けSYN、SYN+ACK、データ、ACK、FINに別の疑似接続経路を設けていません。
送信の一時的なEAGAINは未公開パケットを保持し、接続成功や失敗へ置き換えません。
MSSはinterfaceのMTUを超えない値を使い、外部向けに127/8やmulticast/broadcastを送出しません。

interfaceを失ってもnative resourceの参照は有効なまま、確定した失敗とSO_ERRORの一度だけの消費を保ちます。
legacyのclosed TCB回収からnative resourceが参照しているTCBを除外し、loopbackはinterface喪失から独立させています。
factoryのQUERY_LIMITSには利用可能なrouteのbitを追加しました。これはinterfaceの可用性の情報で、権限ではありません。
CONNECT/LISTENのscope、保護portへのbind制限、信頼するendpointでの添付Handle検証は維持します。

同じnext hopに対する並行TCP接続はARP問い合わせを共有します。
有効なARP requestから学習した場合や、別のpeerの解決も待っている場合でも、そのnext hopの待機パケットを排出します。

`support/routed_tcp_cases.rs`は本番NetworkStackとresource transport、相手側の本番TCP state machineを使います。
ホストfixtureが置き換えるのはdeviceの入出力と時刻・entropyだけです。
実際のEthernet/IPv4/TCPのencode/decodeを通して、subnet外の接続、受理後のlistener終了、双方向データ、半切断、
ARP待ち、複数next hop、MTU 600、送信backpressure、interface喪失、timeout後の参照保持、owner/port分離を確認します。
既存TCP/UDPと合わせて11 targets / 149 testsとnetwork.serviceのmochiOS向けbuildが成功しました。

`native_inet_stream_routed.c`と`support/routed_tcp_echo.py`は、仮想NICを有効にしたKVMで公開C APIから外部通信を確認するための試験です。
ホスト相手はlocalhostだけで待ち受け、euid 0 / 501それぞれについて8192 bytesの内容、write半切断、echo、EOFを確認します。
現在の専用runner設定はnetwork=noneのため、この外部native試験はまだ実行していません。
SDKとRust std overlayは再生成せず、既存SDKと前段で検証した単体runtimeを消費しています。

変更後のnetwork.serviceでも、NICなしKVMでeuid 0 / 501の公開C APIによるTCP・cancel・fork/execと、
既存UDP・64回の同一port再open試験が成功しました。通常イメージへ戻した後のinput/Binder起動、FilesのHome表示と終了も確認しました。
診断managerのbinary・manifest・verificationは元へ戻し、仮の実行ファイルは削除しました。
KVMはPIDを照合して停止し、一時停止の記録はすべて再開済みです。
ホストのrunner設定は変更しておらず、echoサーバーもまだ起動していません。
証跡は`out/checks/mochios-tcp-routed-277-host.log`、`-network.log`、`-loopback.log`、`-normal.log`です。
