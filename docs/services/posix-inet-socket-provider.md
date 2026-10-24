# IPv4 UDP socket Provider（段階213）

段階211のUDP descriptionをsocket workerへ登録し、段階212で渡した信頼するnetwork endpointへ
接続しました。libcのsocket()からのfactory取得・FD登録はまだです。

## 作成と権限

posix-clientにopen_inet_datagramを追加しました。通常のOP_SOCKET_CREATE payloadに
IPv4 / Datagram / protocol 0または17を載せ、factoryをCONTROL | TRANSFERへ縮小して添付します。
送信元のfactoryは借用し、成功したsocket Handleだけを呼び出し元へ返します。
異常応答・通信失敗では受信した別の参照を一度だけcloseします。
PID・FD・network endpoint・object IDをクライアントが指定するwireにはしていません。

POSIX dispatcherはnative IPCのsenderと現在のprocess imageを確認し、factoryをworkerへ転送します。
内部のprocess IDは、同じPOSIX processに属するdispatcherからのIPCだけで受理します。
workerは起動時に登録されたnetwork endpointを借用し、受け取ったfactory参照を所有します。

network authority protocolのVALIDATE（opcode 3）により、添付factoryがそのnetwork endpoint自身の
発行した生存中のfactoryか照会します。照会先は信頼したnetwork endpointです。
任意の添付Provider自身を呼んで検証する構造ではありません。
network側はkernelのidentify_provider_objectで確認し、自己申告のidentityやambient Capabilityには
置き換えません。保持済みfactoryは元のnet.connectをdropした後も利用できます。
foreign Provider、packet以外の偽factory、失効したIDは拒否します。

Registryは一つのDatagramを一つのProvider descriptionとして保持します。
IDは(3 << 59)と単調増加counterの組み合わせで、UNIX socket、transfer/route/receive/connect jobと
重ならず、ProviderObjectIdentifyの符号付き成功範囲内に収まります。IDは再利用しません。
既存routeの識別もjob_idによる厳密な検証へ揃え、複合タグをrouteとして拾わないようにしました。
作成・初期signal・返信の失敗ではdescriptionを回収します。最後のnative releaseでbindingと
factoryをcloseします。FDのdupは同じProviderを参照するため、状態と寿命が共有されます。

## 操作と待機

公開するrightsはREAD / WRITE / CONTROL / WAIT / DUPLICATE / TRANSFERです。
既存connect_socketの内部bridgeは、nativeに識別したUDP descriptionのconnectも処理します。
Providerはoptions、timeout、kind、accepting、bind、disconnect、local/peer address、readinessを
扱います。接続後のnative Read / Writeとoffset 0のReadAt peekは実際のpacket backendへ接続します。
通常受信が短いbufferで成功した場合は、残りのrecordを破棄します。

socket workerのwait setへ下位packet Handleを追加し、readinessを上位Providerへ中継します。
すでに中継したREADABLE / WRITABLEは再度待機対象にしないため、levelが立ったままの間に
workerが空回りしません。受信後などにlevelが落ちれば再armします。
wait setの上限を超える場合だけ、短い期限で全bindingを再観測します。
ancillary cycle GCの期限とも同じwaitを共有し、UNIX socketのGCを止めません。

network側はrecordを排出した後のsignalを応答前に更新します。上位workerが古いREADABLEを
保持したままsleepしないための順序です。signal失敗を成功として記録することもありません。

## 検証

関連11 targets、63 testsが成功しました。実装のRegistry、Datagram、NetworkBackend、wire、
loopback queueを使い、namespace、権限、reply cleanup、peer、データ、readinessの再arm、
signal失敗時の再試行、受信応答前のlevel更新を検証しています。

KVMの診断用managerは、POSIXへのnetwork endpoint引き渡し後にtests/native_inet_socket.rsを
実行します。診断manifestだけにnet.connectを追加し、試験中にdropします。
本番managerへnetwork Capabilityを追加する変更ではありません。
実IPCで全3 groupsと同一portの64回再利用が成功しました。foreign factory拒否、factoryを
呼び出し元でcloseした後のbind/connect、複製Handleの寿命、受信待機、繰り返すpeek、record切り捨て、
接続後の送信元addressを確認しています。libcのFD APIを検証したものとは区別します。

通常managerとmanifest / verificationへ復元後、input READY、Binder、network endpoint配布、
デスクトップとDockを確認しました。診断用net.connectは除去済みです。
NIC不在のnetwork Failed(-6)と既存USB driver errno 22は残っているため、全サービス正常とは扱いません。
VMは停止済みです。
SDK / Rust std overlay / kernelは再生成していません。
記録はout/checks/mochios-inet-provider-213-{final-host,build,network-build,probe-build,native,normal}.log、
-normal.png、-artifacts.json、-evidence.jsonです。

署名付き実装コミットはuser bc6c034、services 451fa52です。

## 残る実装

一般processへのnetwork endpoint配布・factory取得、libc socket()でのfactory添付とFD登録、
sendto/recvfrom/sendmsg/recvmsgのrecord transport接続が必要です。
特にnativeの長さ0のRead/Writeはmechanism上のno-opなので、空recordや長さ0のrecvを
POSIXとして扱うにはrecord transportの接続が必要です。
shutdown、SO_ERROR、buffer容量、socket stat、SCM_RIGHTSとfork/execを含むFD経路の実統合も未完了です。
未対応のProvider controlは明示的にエラーを返します。
TCP、IPv6、fragmentation、ICMP error、multicastも未完了で、POSIX全体のgoalは継続中です。
