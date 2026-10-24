# Internetソケットに向けたUDP bindingの基盤

## 今回実装した範囲

network-stackのUDP tableを、ポートだけの識別から、ローカルIPv4 address・portと
内部のUdpSocketIdによる管理へ拡張しました。
同じportでも異なる具体的IPへのbindingは別queueになり、宛先IPに一致するqueueへ配送します。
wildcardと具体的IP、同じIP同士の重複は拒否します。
SO_REUSEADDR / SO_REUSEPORTによる共有はまだ行いません。

UdpSocketIdはtable内の管理用tokenで、権限やクライアントFDではありません。
同じtableでは解放後もIDを再利用せず、portを再bindしても古いIDから新しいqueueを
receive・peek・query・closeできません。別tableや別service世代にまたがるIDではありません。
将来のIPC adapterは、カーネルが認証したHandleの所有objectからこのIDを選ぶ必要があります。
クライアントが自己申告する番号だけでアクセスを許可するAPIは追加していません。

live queueが空の場合と、解放済みIDは区別して返します。
peekはqueueを消費せず、空datagramもqueue件数として数えます。
queue件数・payload長・socket数を制限し、bindingとpayloadのallocation失敗は
公開・enqueueの前に処理します。巨大なqueue予約でもpanicせず失敗します。

## 実際のstackへの接続

DHCPはwildcard bindingのIDを保持し、そのIDで受信します。
DNSは設定済みローカルIPにbindingを作り、そのIDで問い合わせ・受信・解放します。
時計取得や送信などで失敗した場合も、問い合わせ自身のbindingだけを解放します。
ポート番号だけで別のbindingを閉じる経路には依存しません。

旧port helperはstack内部のwildcard用に限定し、具体的addressのbindingは選択しません。
services workspaceのnetwork-stack依存をローカルsourceへ揃え、診断用overrideを追加しなくても
今回の型と実装を解決するようにしました。生成されたCargo.lockの変更も保存します。

bindのaddress availability・アクセス権検査と衝突エラーの区分は
[GNU libcのbind](https://www.gnu.org/s/libc/manual/html_node/Setting-Address.html)を参照しています。
実インターフェースに存在するaddressか、POSIX credentialsやCapabilityで許可されるかの
判定は、このpacket tableではなく、今後接続するserviceのpolicy側で行います。

## 検証

services/posix/tests/udp_endpoints.rsでは、IPごとの配送、wildcardの競合、port再利用、
古いIDの拒否、peek・空record・容量超過、ephemeral portの一周、allocation失敗を確認します。
既存UDP codecのchecksum・長さ・queue上限のテストも、実装sourceから同じ場所で実行します。
network_routing_policy.rsは実際のNetworkStackに対してIPv4/UDP packetを渡し、
宛先IP別の配送とDNSの時計取得・送信失敗後のbinding解放を検証します。

関連4 targetsの25件と依存解決2件が成功しました。
mochiOS targetのnetwork.serviceもbuild済みです。
今回の変更はpacket tableとservice内の管理処理で、native IPC・Handle ABIは変えていません。
VMは起動せず、rootfsへの配置や実NICによる通信確認も行っていません。
SDKとRust std overlayは再生成していません。

記録はout/checks/mochios-udp-endpoints-207-verified.log、-dependencies.log、
-production-build.log、-evidence.jsonです。

## 続ける範囲

POSIX AF_INETのcreate / bind / connect / sendto / recvfrom、Handleを返すnetwork Provider、
受信待ちEvent・close後の寿命管理との接続はまだ必要です。
peer filter、IPv6、再利用option、broadcast / multicast policyも残っています。
今回のpacket管理の追加をInternet domainソケット全体の完成とは扱いません。
POSIX互換環境全体のgoalは継続中です。
