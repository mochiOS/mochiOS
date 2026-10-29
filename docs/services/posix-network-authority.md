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

このHandleを添付したUDP資源の作成、datagram Provider、受信signalは
[段階210](posix-datagram-provider.md)で追加しました。POSIX AF_INETの
create / bind / connect / sendto / recvfromへの接続、endpointのbootstrap配布は引き続き必要です。
実NICでの通信やInternet socket全体の完成は、今回の試験からは主張しません。
POSIX互換環境全体のgoalは継続中です。

## 異常なnetwork statusの符号（phase279）

phase277のfailure_status変更に誤りがあり、未表現の値のfallbackが+5になっていました。
syscall::EIOはu64へ符号拡張した-5なので、i32へのcast後に再度negateしてはいけません。
fallbackを-5へ戻し、回帰試験も同じ定数式に依存せず、wire上の-5を直接確認します。
network_error_statusとnetwork_routing_policyの27 host testsが成功しました。
証跡はout/checks/mochios-dns-records-279-status.logです。kernel・SDK・IPC ABIは変更していません。


## 数値アドレスの変換（phase278）

`posix-client::internet`とnewlib runtimeへinet_pton / inet_ntopを追加しました。
IPv4のstrict dotted-decimal、IPv6の省略・大小文字・IPv4埋め込み表現を扱います。
変換は値だけを扱う処理で、network endpoint、Capabilityの取得、POSIXサービスRPCは使いません。
文字列の解釈はclient層で行い、kernelへaddress familyやInternet固有の意味論を追加していません。

C入口はapplicationの文字列・binary addressを既存MemoryCopyでsnapshotします。
source/destinationの生ポインタからRust参照を作らず、NULを越えて別のvirtual pageへ読みに行きません。
不正な表現は0、未対応familyはEAFNOSUPPORT、短い出力はENOSPCを区別します。
成功や表現不正の戻り値ではerrnoを変更しません。出力サイズを確認してからNUL込みの結果を書き込みます。

arpa/inet.h、IPv6のin6_addr / sockaddr_in6、in6addr_any / in6addr_loopback、IN6_IS_ADDR_*を追加しました。
C ABIはin6_addrが16 bytes・alignment 4、sockaddr_in6が28 bytesです。C++からも利用できることを確認しています。
これらの型と文字列表現への対応は、IPv6 socketやIPv6 packet送受信の実装を意味しません。

internet_address.rsでは、ホストCライブラリの別実装を使ってIPv4/IPv6の構文・表現を比較します。
2048組のIPv4/IPv6 addressとzero runの位置・長さ、表現不正、family、buffer境界を確認しました。
関連する5 targets / 33 host tests、単体runtime、診断manager、native Cのbuildが成功しています。
native_internet_address.cはKVMのeuid 0 / 501でC ABI、NUL境界、保護page、NULL・overflow・kernel virtual addressの拒否、
短い出力、定数symbol、並行変換とthread別errnoを確認しました。診断用のnetwork権限は追加していません。

SDKとRust std overlayは再生成せず、既存SDKを入力として単体runtimeをbuildしました。
試験後はmanager・manifest・verificationを元のhashへ戻し、仮の実行ファイルを削除しました。
通常イメージの内容を復元し、checksumも検証しました。今回、復元後のGUI起動の再試験は行っていません。
証跡はout/checks/mochios-internet-address-278-{host-final,runtime,c-build,cxx-header,native,image-normal}.logです。

getaddrinfo / getnameinfoと旧来のinet_addr / inet_ntoa、DNSの複数record・AAAA・PTRへの接続は残っています。
外部TCPのnative試験も、runnerの仮想NIC切替の返答待ちのままです。ホスト設定は変更していません。
POSIX互換環境全体のgoalは継続中です。

## DNSの複数recordと別名（phase279）

network-stackのDNS decoder/cacheをRRset単位へ変更しました。A / AAAA / PTRのquery生成と応答解析、
IPv4 / IPv6の逆引き名生成を提供します。DNSのnameはbinary labelとrootを扱い、
アプリケーション向けのhostname検査は別に適用します。末尾dot付きhostnameも正規化します。
形式は[RFC 1035](https://www.rfc-editor.org/rfc/rfc1035.html)、
nameとTTLの規則は[RFC 2181](https://www.rfc-editor.org/rfc/rfc2181)、
AAAA / IP6.ARPAは[RFC 3596](https://www.rfc-editor.org/rfc/rfc3596)を参照しています。

answer sectionから質問のname/class/typeへ対応する全recordを選び、重複を除きます。
同じ応答内のCNAMEはrecordの並び順に依存せず追跡し、canonical nameを保持します。
循環・深すぎる別名・異なるCNAME targetの併存・CNAMEと通常dataの併存は失敗とします。
TTLは経由するCNAMEとRRset全体の最小値に揃え、high bit付きTTLを0として扱います。
positive cacheはname/type単位で置換し、旧応答のmemberとのmergeや個別の延命を行いません。
空の応答を成功したnegative cacheとして保存することもありません。

全sectionのrecord境界と既知のRDATAを確認してから結果を返します。
authority / additionalや無関係なownerのrecordからanswerやcacheを作りません。
未知のRDATAは不透明なbytesとして境界のみ確認します。DNSSECの検証やAD flagによる信頼付与は行いません。
圧縮nameはRDLENGTH、展開後255 bytes、pointerの向きと循環を確認します。
wire decoderはTCP最大長65535 bytesを扱い、TC付きの応答は部分成功にせず専用errorへ分けます。

既存network.serviceのIPv4 resolverをこのdecoder/cacheへ接続しました。
戻り値は従来通り先頭のIPv4ですが、同じ応答内の別名を解決し、cacheには全候補とcanonical nameを保持します。
transaction/questionが一致しない応答を無視し、TCは再試行可能な失敗として返します。
権限の確認・IPC wire・kernel ABIは変更していません。

dns_recordsの32件（既存decoder/retry試験9件を含む）、実NetworkStackのnetwork_routing_policy 23件、
network_error_status 7件の合計62 host testsと、mochiOS向けnetwork.service buildが成功しました。
実サービスのUDP bindingへIPv4 packetを投入する試験で、複数候補・別名TTL・cache hit、
不一致応答の除外、truncated/malformed応答の拒否を確認しています。
今回の変更は新しいIPCやC APIを追加しないためKVMは起動していません。imageへのstageも行っていません。
SDKとRust std overlayは再生成せず、SDKの既存4ファイルのhashも一致しました。
証跡はout/checks/mochios-dns-records-279-{host-final,network-final}.logと-counts.jsonです。

AAAA/PTRのサービス経由の照会、複数messageにまたがるCNAME追跡、DNSのTCP fallback、
SOAを使うnegative cache、非同期resolver job、getaddrinfo / getnameinfoへの接続は引き続き必要です。
外部通信のnative試験はrunnerのNIC切替の返答待ちです。ホスト設定は変更していません。
POSIX互換環境全体のgoalは継続中です。

## 非同期DNSとTCP再試行（phase280）

network.serviceへ問い合わせ単位のresolver jobを追加しました。各jobは期限、質問・ID、
UDP bindingまたはTCP connection、送受信の進捗、別名と結果を所有します。
開始・pump・結果の取得・取消を分け、一回のpumpで行うI/Oをjobごとに4段階へ制限します。
全体は16 jobsで、取得されていない結果もquotaへ含め、job IDを再利用しません。
完了・失敗・取消・network contextの変更時に専有する通信資源を解放します。
他のUDP/TCP利用者と同じ既存のpacket table / transportを使い、kernelへDNSやPOSIXの意味論を加えていません。

UDP応答のTC、または再送上限からTCPへ切り替え、2 bytesのlengthと本文を一つのqueryとして送ります。
部分送信・分割されたlength/body・EAGAIN・EOFを区別し、応答全体の検証後に結果を返します。
TCP上でも不一致のID/questionはframe単位で除外し、TCや不正なlengthを部分成功にしません。
UDPの送信待ちは未送信の再送回数として数えません。source portとquery IDはnative RandomFill由来とし、
乱数取得に失敗した場合は推測可能な代用値を使わず失敗します。
応答のsource/destination address、port、ID、question/type/classを照合します。
根拠は[RFC 7766](https://www.rfc-editor.org/rfc/rfc7766.html)と
[RFC 5452](https://www.rfc-editor.org/rfc/rfc5452.html)です。

複数messageにまたがるCNAMEも追跡します。以前の別名のTTLを次の照会中の経過時間だけ減らし、
元のquestion、全alias、canonical nameと全候補を保持します。
messageをまたぐ循環・合計16段を超える別名も拒否します。
DNS server、local address、gateway/subnetのcontextが変わった場合、cacheを破棄し、
進行中の照会を別networkへ黙って付け替えず失敗させます。

既存ResolveIpv4 RPCは処理loopへjobを登録し、結果ができた時点で元のnative IPC呼び出しへ返信します。
認証はIPCが保証したsenderに対するnet.connectの検査です。wireへ自己申告PIDを追加していません。
返信待ちは全体16件・senderごと2件、期限は最大30秒です。
旧HTTP/TLSの同期処理も同じresolverを駆動し、DNSの別実装と呼び出し元指定のquery乱数を削除しました。
HTTP/TLS要求全体を非同期化したという意味ではありません。

4 targets / 81 host testsと、通常network・診断probe・managerのmochiOS向けbuildが成功しました。
dns_jobsでは並行する照会、quota、再送とbackpressure、分割I/O、取消、context喪失、別名TTLと循環を確認します。
実NetworkStackを使う試験ではUDPのTCから実Ethernet/IP/TCPへ切り替え、1 byteだけ先に届くlengthと、
MTUを超える82候補の応答を復元し、TCPとUDPの資源が解放されることを確認しました。

native_dns_repliesはproductionの返信controllerを使い、実IPCで待機中の問い合わせを残したまま
別の同期呼び出しへ先に返信し、その後元の呼び出しへ戻ることをKVMのeuid 0 / 501で確認しました。
net.connectの破棄後の新規要求は両方のidentityで拒否されました。
このnative試験のDNS dataは固定fixtureです。外部DNS serverとの実通信を検証したとは扱いません。
診断用manager・manifest・verificationは元のhashへ復元し、仮のprobeも削除しました。
証跡はout/checks/mochios-dns-jobs-280-{host-final,network-final,native-build,manager,native}.logと-counts.jsonです。

通常managerと更新したproduction networkでimageを再生成し、checksumを確認しました。
通常KVMでinput READY、Binder ready、desktop / DockとFilesの起動・閉じる操作を画面で確認しています。
閉じる操作の最初のclickがwindowのアクティブ化になり、次のclickで閉じる既存挙動は残っています。
NICなしのnetwork Failed(-6)も残るため、全service成功とは扱いません。
両VMは自身のPIDを照合して停止し、host analyzerの一時停止記録も再開済みです。
通常起動の証跡は-normal.log、-desktop.png、-files.png、-closed.png、-image-normal.logです。

native Handleによるresolver jobの公開、posix-clientからのAAAA/PTR照会、getaddrinfo / getnameinfo、
SOAを使うnegative cacheは引き続き必要です。IPv6 transportと外部通信のnative確認も残っています。
SDKとRust std overlayは再生成していません。ホストのNIC設定も変更していません。
POSIX互換環境全体のgoalは継続中です。

## DNS authorityとquery Handle（phase281）

既存のnetwork endpointへ、DNS専用factoryと問い合わせ結果のProviderを追加しました。
新しいserviceやkernel entry pointは追加していません。NDNSのversion 1 wireはnameのbytes、
A / AAAA / PTRの種類、期限だけを受け取り、PID・FD・provider IDをクライアントから指定しません。
factory取得時にはnative IPC senderのnet.connectを確認します。
明示添付されたfactoryはcreating endpointのProviderObjectIdentifyで検証し、任意objectへのcontrol呼び出しで
正当性を推測することはしません。deriveは種類のsubsetと期限の短縮だけを認めます。
既存factoryの利用はCapDrop後も可能で、新規取得の権限とは別に扱います。

query HandleはREAD / WAIT / CONTROL / DUPLICATE / TRANSFERを持ち、WRITEは与えません。
READのmetadata queryからpending/ready、種類、結果statusと長さを取得できます。
結果はquestion / canonical name / aliases / typed valuesを含む不変のsnapshotで、ReadAtで読みます。
CONTROLによるcancelは、すでに確定した結果を上書きしません。
最後のnative参照が閉じられたreleaseだけが進行中の問い合わせを解放し、factoryのcloseは発行済みjobを無効化しません。
provider IDは既存native ABIの非負領域に収め、再利用しません。

factoryは64、queryは16へ制限します。snapshotは最大2 MiB、aliasは16段で、wireの長さ・reserved bits・
種類・name境界・value countを検査します。失敗用bufferをbackend開始前に確保しているため、
結果の追加allocationに失敗しても、既存bufferでerror snapshotを返せます。
発行・signal・backend・Handle付き返信の失敗では資源を解放し、可能なら添付なしのerror返信を返します。
NICなしでもfactory endpointは利用でき、問い合わせは偽のaddressを作らずnetwork-unavailableの結果を返します。

posix-client::resolverはfactoryの取得・検証・権限縮小、query開始、metadata照会、cancelとsnapshotの読み取りを提供します。
借用したsourceをcloseせず、異常な返信の重複Handleを一度ずつ解放します。
snapshotはcallerのbufferへ最大4096 bytesずつ読み、短い結果・過大な返却件数・question/type/statusの不一致を拒否します。
取得済みHandleへの操作は既存native APIを使い、posix.serviceやサービス名検索を介しません。

7 targets / 105 host testsと、network・診断managerのmochiOS向けbuildが成功しました。
権限縮小、native origin、raw releaseの拒否、取得済み結果の不変性、cancelと完了の競合、quota、
発行・signal・返信・backendの失敗時の解放、複数chunkのsnapshotとclient側の異常返信処理を確認しています。
証跡はout/checks/mochios-dns-handles-281-{host-final,network-final,manager-final}.logと-counts.jsonです。

最初のnative試験では、ReadAtの前にkernelが発行する汎用OP_MEMORY_OBJECTへの応答が欠けており、
metadata照会後の読み取りが-95になりました。DNS snapshotはpager identityを提供しないため、
その照会へ0を返す実装と回帰試験を追加しました。kernel側は変更していません。
初回の失敗ログは-native-first.logへ保持しています。

続くnative試験ではfixture側のIPC APIの扱いを2点修正しました。
受信用endpointのHandleをblocking waitへ渡し、IPC callの返り値はsender情報を含むpacked値から
下位32 bitsの返信長を取り出します。途中の失敗ログは-native-second.logと-native-third.logへ保持しています。
修正後はeuid 0 / 501の両方でfactory取得・derive・種類と期限の制限・native originの検査・
WRITE拒否・読み取り専用HandleのIPC委譲・最後の参照までの結果保持・CapDrop後の挙動とquota回復が成功しました。
NICなしの明示的な失敗結果を使う試験であり、外部DNS通信の成功ではありません。
成功ログはout/checks/mochios-dns-handles-281-native.logです。

診断manager・manifest・verificationを元のhashへ戻し、production networkを組み込んだ通常imageの
checksumを確認しました。通常KVMでinput READY・Binder ready、desktop / Dock、Filesの起動・
フォルダ表示・終了を画面で確認しています。最初のclose clickでwindowがアクティブになり、
次のclickで閉じる既存挙動と、NICなしのnetwork Failed(-6)は残っています。
証跡は-normal.log、-desktop.png、-files.png、-close-first.png、-closed.png、-image-normal.logです。
診断probeは追加していません。各KVMは自身のPIDを照合して停止し、analyzerの一時停止記録はすべて再開済みです。
今回の各pause時点で対象processは0件でした。SDKとRust std overlayは再生成していません。

getaddrinfo / getnameinfo、SOAを使うnegative cache、IPv6 transport、外部DNS serverとのnative実通信は残っています。
ホストNIC設定の変更は返答待ちのため行っていません。POSIX互換環境全体のgoalは継続中です。

## 名前・サービス変換の共通処理（phase282）

posix-client::netdbへ、getaddrinfo / getnameinfoのユーザー空間での共通処理を追加しました。
Backendは名前サービスのdataとinterface設定を供給し、共通処理がaddress family、TCP/UDPとprotocol、
AI_PASSIVE / CANONNAME / NUMERICHOST / NUMERICSERV / V4MAPPED / ALL / ADDRCONFIGを解釈します。
数値hostとportだけの変換ではDNS・hosts/services検索・endpoint取得を行いません。
AI_ADDRCONFIGを明示した場合は供給された非loopbackの設定を使い、推測したfamily maskは使いません。
IPv6の数値scopeとinterface名、getaddrinfo用のinet_addr形式の数値IPv4も扱います。
inet_ptonの厳密な構文は変更していません。

候補数を先頭の1件へ絞らず、全addressと適合するtransportをcallerの出力先へ渡します。
canonical nameは最初の候補だけへ付けます。family単位で結果を出した後の失敗や、
出力先のallocation失敗を成功へ変換しません。callerは全体が成功した場合だけ結果を公開する契約です。
この処理はallocation、FD table、global cache、共有credential stateを持ちません。

逆引きはhost/service出力を個別に省略でき、NI_NUMERICHOST / NUMERICSERV / NAMEREQD / DGRAMを扱います。
NI_NOFQDNはBackendがlocalと判定した名前だけへ適用します。
名前なしの結果は必要に応じ数値へ戻し、通信失敗や権限拒否とは区別します。
参照する仕様は[RFC 3493 §6](https://www.rfc-editor.org/rfc/rfc3493.html#section-6)です。

hosts/servicesの借用record parserも追加しました。callerが権限に従って読み込んだdataから
複数address、canonical name、alias、portとTCP/UDPを取り出し、parser自身はfilesystemへアクセスしません。
DNS境界ではlabelのbytes、escaped dot/backslash、decimal escape、root、255-byte nameを保持し、
IPv4/IPv6の逆引きquestionを生成します。phase281の不変snapshotを全A/AAAA候補またはPTR名へ変換し、
名前なし・再試行・メモリ不足・system errorを区別します。

5 targets / 39 host testsが成功しました。新規21件に、既存のaddress変換とresolver client/wireの18件を加えています。
GNU libcとの独立比較では数値forward/reverseを72通り、legacy IPv4表記も照合しました。
この比較で見つかった、mapped IPv6 literalをAF_INETとして扱う互換動作の不足を修正しています。
複数千候補、部分失敗、configによるfamily選択、mapped fallback、scope、逆引きとbuffer境界、
hosts/servicesの不正行、binary DNS labelと1024件のsnapshot変換も確認しました。
mochiOS向けposix-clientのsyscall-endpoint構成もbuild成功です。
証跡はout/checks/mochios-netdb-282-{host-final,client-final}.logと-host-results.txtです。
今回の失敗・修正ログは-host-second.logへ保持しています。

これはC公開関数の完成ではありません。netdb.h、getaddrinfo/freeaddrinfo/getnameinfo/gai_strerrorのC ABI、
結果listの所有権・faultableな入出力、実際のDNS Handleを待機して使うBackend、
hosts/services設定の読み込みと名前サービス方針への接続は引き続き必要です。
pureな処理の追加のため今回KVMは起動していません。kernel・SDK・Rust std overlayは変更していません。
POSIX互換環境全体のgoalは継続中です。

## C resolverの結果と入出力の所有権（phase283）

newlib-runtimeのnetdb_memory / netdb_abiへ、具体的なBackendを受け取るC resolver transactionを追加しました。
Backend・入力snapshot・作成中の結果listを既存io_resourcesへまとめて登録し、
失敗したprefixとBackend資源を解放してからEAI_SYSTEMのerrnoを公開します。
errnoのnamespaceは既存のnewlib変換を使い、POSIX wire statusをCへ直接返しません。

addrinfoはLinux互換の48-byte配置です。各nodeは自身のsockaddrとcanonical名を同じallocationに持ち、
IPv4/IPv6のfamily・port・flowinfo・scopeとpaddingを初期化します。
listの任意の有効なsublistを個別に解放でき、canonical名はBackendのbufferを参照し続けません。
allocationの追加・listへの連結・公開とcleanup所有権の解除にはnative deliveryのguardを使います。
公開前のlistはprivateなstorageとして扱い、C入力のpointerをRust参照へ変換しません。

hintsと文字列、sockaddrはMemoryCopyでsnapshotし、getnameinfoの出力もprivate bufferからコピーします。
hintsのC paddingは未初期化でも許容し、使用しないfieldにはPOSIXのzero/null契約を適用します。
host/serviceの出力を個別に省略でき、両方の容量を検査してから最初の書き込みを行います。
lookup中にresultのmappingがread-onlyになった場合もEAI_SYSTEM / EFAULTを返してlistを回収します。
freeに相当する内部処理の入力は、返却済みの有効なlist/sublistであることが前提です。

4 targets / 52 host testsが成功しました。新規12件では実際のio_resourcesとcallback_resources、
process_vm_readvによる自己プロセスのmemory検証を使い、2048 nodes、allocationの各失敗位置、
late Backend failure、出力先のread-only化、部分listの解放、unaligned/invalid pointer、
処理frameの放棄による一度だけの解放と、cleanup後のerrnoを確認しました。
既存のpolicy/data 21件とcallback resource 19件も成功です。
sandbox内の自己memory検証はEPERMになったため、同じhost試験をsandbox外で実行しました。
証跡はout/checks/mochios-netdb-memory-283-{host-native-copy,runtime}.logと-host-results.txtです。
standalone runtime archiveはbuildしましたが、SDKの6 filesのhashは変化していません。
kernel・Rust std overlay・通常imageは変更せず、今回KVMは起動していません。

この段階では公開Cシンボルとnetdb.h、実DNS HandleのBackendはまだ接続していません。
AI_ADDRCONFIGとinterface名の解決に必要なnetwork設定情報の照会も、既存endpointには不足しています。
その情報経路、名前サービス設定、実Handleの待機・取消を揃えてからCの公開入口へ接続します。
全体のPOSIX goalは継続中です。

## Interface設定の観測（phase284）

network endpointへNIFS version 1のread-only snapshotを追加しました。
interfaceのindex・名前・flags・MTUと、addressの種類・prefix・scope・usable状態を一つの返信へserializeします。
これはnativeなnetwork dataで、POSIXのFDや自己申告PID、pointerをwireへ載せません。
serviceはIPCが保証するendpoint経由のdeliveryと添付Handleなしを要求します。
委譲済みendpointのSENDがこの観測の権限です。legacyなPID宛て呼び出しやnative Provider要求を通して
同じ観測へ入ることはできません。socket/DNS factoryのnet.connect検査は維持しています。

現在のserviceが扱うIPv4 loopbackをlo/index 1、実deviceをeth0/index 2として表します。
deviceなしではloopbackだけを返し、DHCP設定前やlease喪失後にはphysical addressを作りません。
MTUは実deviceのinfoと、現在のlocal datagramの転送上限から取ります。
IPv6 addressのwireとclient解析にも対応しますが、実IPv6 transportを追加したという意味ではありません。

posix-client::interfacesは委譲されたendpointを呼び、返信全体のversion・長さ・reserved field、
index/nameの重複、address/prefix/scopeを検査します。bufferは4096 bytesを送信前に確保する契約です。
返信の上限超過はpartialな一覧へ切り詰めず失敗し、大きな返信が受理されない場合は小さなerror headerを試みます。
要求していない添付Handleは重複を避けてcloseします。snapshotからinterface名のindexを引き、
AI_ADDRCONFIG用にはloopback、unspecified addressとusableでないaddressを除外してfamilyを求めます。
このPOSIXの判断はclient側に置き、kernelへinterfaceやPOSIXのentry pointを加えていません。

5 targets / 63 host testsが成功しました。新規11件はproductionのwire・client・serviceを使い、
実状態のencode、lease喪失、mask/prefix、IPv6 scope、endpoint deliveryによる認可、
添付Handleの拒否・回収、version/境界/重複と返信失敗を確認します。
既存policy/data・routing・network errorの52件も成功しています。
networkと診断managerのmochiOS向けbuildも成功しました。
証跡はout/checks/mochios-interfaces-284-{host-final-verified,network-verified,manager-verified}.logと-host-results.txtです。

最終版のKVM試験はeuid 0 / 501で成功しました。通常managerのmanifestからnet.connectを追加せず、
64回ずつの観測、lo/MTUとfamilyの解釈、SENDだけの複製endpointでの観測、SENDなしの拒否、
同じendpointからのDNS factory取得の拒否を確認しています。
証跡はout/checks/mochios-interfaces-284-native.logです。NICなしの環境であり、外部通信の成功とは扱いません。

診断managerを元のhashへ戻し、manifest/verificationのhashも一致することを確認しました。
更新したproduction networkで通常imageを生成し、checksum、input READY、Binder readyと、
desktop / Dock、Filesの起動・フォルダ表示・終了を画面で確認しています。
closeの最初のclickでアクティブになり、次で閉じる既存挙動と、NICなしのnetwork Failed(-6)は残っています。
証跡は-normal.log、-desktop.png、-files.png、-close-first.png、-closed.png、-image-normal.logです。
3回のKVMは自分のPIDを照合して停止し、各analyzerの記録も再開済みです。pause時点の対象は各0件でした。
SDKの6 filesのhashは変わっておらず、kernelとRust std overlayも変更していません。

C resolverの公開関数、実DNS Handle Backend、hosts/services等の名前サービス設定への接続は残っています。
POSIX互換環境全体のgoalは継続中です。
