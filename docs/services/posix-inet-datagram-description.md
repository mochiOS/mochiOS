# IPv4 UDP共有description（段階211）

段階210のpacket Handleに、POSIX socket用のdescription policyとnativeアダプタを追加しました。
実装はservices/posix/src/inet_datagram.rsとinet_datagram_native.rsです。

**libcのAF_INET socket経路への登録・配線はまだです。** 既存のsocket workerは引き続き
AF_UNIXの実装を使います。この段階ではUDP description自体と実際のpacket wireを使う
アダプタを実装し、同じコードをホストで実行して検証しています。

## 所有権と認証境界

一つのDatagram instanceを一つのProvider descriptionとして所有する構造です。
FDごとに複製する構造ではなく、options、timeouts、local binding、peerが共有されます。
network endpointは継承したHandleを借用し、factoryは明示的に委譲された参照を所有します。
アダプタには名前検索や、service自身のCapabilityで元のクライアントを代行する処理はありません。

factoryにはCONTROLとTRANSFERが必要です。packet作成時の添付rightsはCONTROLだけに縮小します。
これはnetwork.service側のidentify_provider_object認証で検査されます。
将来のsocket作成要求では、元の呼び出し元からworkerまでfactoryを転送するために
CONTROL | TRANSFERを明示的に委譲する必要があります。

成功応答はversion/opcode/status、添付数、rights、reserved、Handle値を検証します。
異常応答では受信した別の参照だけをcloseし、借用中のfactoryはcloseしません。
重複した受信ラベルも一度だけcloseします。
descriptionのDropはpacket bindingをcloseし、その後backendのDropがfactoryをcloseします。

## POSIX policy

- IPv4 / Datagram / protocol 0または17を受け付け、未対応domain/kind/protocolは明示的に拒否。
- 作成時は未bind。明示bindまたは最初の送受信・connectでbindingを取得。
- bindの再実行はEINVAL。失敗・不正なlocal応答では仮bindingをcloseし、再試行可能。
- 宛先なしの未接続sendはEDESTADDRREQ。異なるaddress familyやpayload超過も送信前に拒否。
- connectはrouteとpeer更新が成功してからpeerをpublish。失敗時は前のpeerを維持。
- disconnectはpeer/filterを解除し、local port、options、timeout、受信済みrecordは維持。
- 接続済みでもsendtoの明示宛先による上書きを許可。default peerは変更しない。
- peekはrecordを保持。短いbufferは元の長さを別に返し、通常受信では残りを破棄。
- payload buffer長0でもrecordを受信・消費でき、空recordと未着EAGAINを区別。
- 操作は一度で返る。blocking、再試行、timeout待機はclient側の処理として接続する。

packet protocolにSOURCE（NUDPSRC1）を追加しました。入力は宛先endpoint 8 bytes、
出力は送信元endpoint 8 bytesです。bindingやpeerは変更しません。
wildcard bindingの接続後getsocknameでは、保持したportとrouteに対応する送信元を返します。
NICなしの外部routeは失敗し、0.0.0.0を接続済みの送信元として成功扱いにしません。
返信の送信元port・addressがbindingと矛盾する場合はEIOとします。

## 検証

関連11 targets、60 testsが成功しました。inet_datagram_descriptionは実装のDatagram、
NetworkBackendとpacket wireを実行し、native Handle/IPC境界だけをfixtureで置換しています。
受信機構には実装のLoopback / UdpSocketTableを使います。
network_datagram_authorityは実際のProvider dispatcherでSOURCEの長さ・route・認証・非変更性を
検証します。64回の同一port作成・破棄、失敗時の参照回収、peer更新のrollbackも確認しています。

posix.serviceとnetwork.serviceのmochiOS向けtarget buildが成功しました。
今回の変更はuserspace policyと同じHandleControlのpayload追加で、kernel ABI変更はありません。
新たなKVM起動・image差し替えは行っていません。前段210のnative境界試験とは区別します。
SDK / Rust std overlayは再生成していません。

記録はout/checks/mochios-inet-description-211-{final-host,build,network-build}.log、
-artifacts.json、-evidence.jsonです。

署名付き実装コミットはuser efd7fd7、services d17a9f6です。

## 残る配線

認証済みsocket作成要求のfactory添付、worker内のdescription登録、POSIX Provider controlと
record transportへの接続、native readinessの中継、libcの送受信経路、endpoint bootstrap配布、
fork / exec / dup / SCM_RIGHTSでの実統合試験が必要です。
TCP、IPv6、fragmentation、ICMP error、multicast、socket options全体も引き続き未完了です。
POSIX全体のgoalは継続中です。

信頼するnetwork endpointのPOSIX層への起動時配布は[段階212](posix-network-bootstrap.md)で追加しました。
factory転送とsocket workerへの登録は引き続き必要です。
