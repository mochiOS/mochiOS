# IPv4 UDPのlibc record転送（段階214）

`socket(AF_INET, SOCK_DGRAM, 0/IPPROTO_UDP)`から、既存のUDP Providerへ接続しました。
FDはruntime内で予約・登録し、カーネルやnetwork.serviceへは渡しません。

## 作成時の権限

登録済みのPOSIX processはOP_NETWORK_ENDPOINT_GETでSEND | TRANSFERだけのendpointを
受け取ります。これは通信先の参照であり、net.connectを付与する操作ではありません。
呼び出し元自身がnetwork endpointへfactoryを要求し、network.serviceがnative senderの
Capabilityを確認します。POSIXサービスがクライアントの代わりにambient権限を使う経路はありません。
一時endpointとfactoryは所有者が成功・失敗の両方でcloseします。

既存のfactoryを明示的に委譲されたクライアントは、その参照に基づいてsocketを作れます。
ambient Capabilityをdropした後の新規factory取得は拒否されます。
カーネルの変更、サービス名検索、自己申告PIDの認証は追加していません。

## recordデータ経路

既存のsocket-worker endpointとRAM添付によるrecord転送へIPv4を接続しました。
SendToのwireには数値IPv4 addressを追加し、socketのWRITEとRAMのMAP | READだけを
添付します。受信はsocketのREADとRAMのMAP | WRITEです。生ポインタはIPCに載せません。
workerはnativeに識別したProviderからdescriptionを選び、添付RAMの種類・範囲を確認します。

送信は一つのdatagramをcommitします。受信はheadをpeekし、応答とRAMへのコピーを
成功させてから消費します。コピー失敗ではqueueを消費しません。元の長さと送信元を返し、
MSG_PEEK、MSG_TRUNC、空datagram、長さ0の受信を区別します。
UDP送信のEAGAINはnative WRITABLEを待ちます。UNIX record送信では既存の長さ別待機を維持します。

## 検証

関連ホスト試験は9 targets、44件成功、失敗0件でした。mockだけでなく、実際の
UDP description / network adapter / loopback backendを通してrecordの意味論を確認します。

KVM上ではstandalone newlib runtimeのC ABIからsocket、bind、getsockname、sendto、recvfrom、
connect、send、poll、dup、closeを実行しました。送信元、peek、MSG_TRUNC、空packetと
長さ0の消費、dup後の参照寿命を確認しました。既存のnative Provider試験3 groupsも成功しました。
診断manifestのnet.connectを除去し、通常managerで再起動した後、input READYと
デスクトップ・Dockを画像で確認しています。VMは停止済みです。

初回の試験では診断リンカが別の古いrelease runtimeを選んでいました。
実際に使用するrelease archiveを再ビルドして再リンクした結果を最終証拠にしています。
さらにC入口のsendtoに残っていたUNIX専用address判定をIPv4対応へ修正しました。

記録はout/checks/mochios-inet-record-214-{final-host,native,normal}.log、normal.png、
artifacts.jsonとout/checks/inet-record-214.mkです。配布SDKとRust std overlayは再生成していません。

## 残る範囲

IPv4のrecvmsgとancillary data、追加のsocket options、TCP、IPv6は未完了です。
NICのないrunnerではnetwork readinessのFailed(-6)が残り、USB driverのerrno 22も別件です。
この検証は外部ネットワーク、LLVMの実行、POSIX全体の完成を示しません。
