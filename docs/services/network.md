# network.service

## 1. 責務

`network.service`はdriver非依存のIPv4 stackです。virtio-net driverとは
`mochios-net-device-protocol`だけで通信し、次を所有します。

- interface設定とpacket統計
- Ethernet demultiplex
- ARP cache、retry、解決待ちpacket
- IPv4 checksumとrouting
- ICMP Echo
- UDP socket table
- DHCP clientとlease timer
- DNS resolverとTTL cache
- outbound TCP connection table、buffer、再送timer
- application向けPing/DNS/TCP/Statistics IPC

PCI、MMIO、DMA、virtqueueは扱いません。

## 2. 受信処理

driverから1回に最大1 frameを取得し、1 loopで最大32 framesを処理します。処理順は次です。

1. Ethernet header長、source MAC、destination MAC、EtherTypeを検証
2. 自分宛unicastまたはbroadcast以外を破棄
3. ARPまたはIPv4へdemultiplex
4. IPv4 version/IHL、total length、fragment、TTL、header checksumを検証
5. ICMP、UDPまたはTCPへdemultiplex

IPv4 option、fragment、未知protocolはfail closedで破棄します。UDP checksumが0の場合はIPv4の
規則に従って省略として受理し、非0の場合はpseudo headerを含めて検証します。

## 3. 上限

現行値は次のとおりです。

| 対象 | 上限 |
| --- | ---: |
| ARP cache | 32 entries |
| ARP cache TTL | 60 seconds |
| ARP retry | 5回、1秒間隔 |
| ARP解決待ちIPv4 packet | 8 packets |
| UDP bindings | 8 ports |
| UDP receive queue | portごとに8 datagrams |
| UDP payload | 1472 bytes |
| DNS message | 512 bytes |
| DNS cache | 32 entries |
| DNS retry | 3 attempts、初期500 msの指数backoff |
| TCP connection | 16 connections |
| TCP send buffer | connectionごとに16 KiB |
| TCP receive buffer | connectionごとに16 KiB |
| TCP IPC transfer | 4096 bytes/request |
| TCP retransmit | 初期500 ms、5 retries、指数backoff |
| TCP TIME_WAIT | 30 seconds |
| 1 loopのRX処理 | 32 frames |

UDP port 0をbindすると49152から65535の範囲で未使用ephemeral portを割り当てます。同じportの
重複bind、socket上限、queue上限、payload上限はerrorです。DHCP clientはport 68を明示bindします。

## 4. ARPとrouting

ARPはEthernet/IPv4、address length、operation、sender MACを検証します。Ethernet sourceと
ARP sender MACが一致し、target IPが自分のaddressであるpacketだけをcacheへ登録します。

未解決next hopへのIPv4 packetはbounded queueへ保持し、ARP Reply受信後に該当next hop分だけ
送信します。retry上限到達時は該当packetをdropします。

同一subnet宛はdestinationをnext hopとし、それ以外はdefault gatewayを使用します。
address、mask、gatewayが未設定なら通常packetを送信しません。

## 5. DHCP

起動時にランダムtoken由来のtransaction IDを生成し、DISCOVERをbroadcastします。OFFER受信後は
REQUESTを送り、保存したaddress/serverと一致するACKだけを受理します。取得する値はaddress、
subnet mask、default gateway、最初のDNS server、lease time、server identifierです。

再送はbusy loopではなくtickと指数backoffで制御します。leaseの50%でRenewing、87.5%で
Rebindingへ移行し、失敗時は30秒後にSelectingから再開します。valid ACK受信時だけ
network-readyを通知します。`status=0`はDHCP boundを表し、将来の通信成功を保証するものでは
ありません。

## 6. service IPC

同じ共有wire v1で次を提供します。

- `Ping`: IPv4 address、request ID。結果はstatusとRTT milliseconds。
- `ResolveIpv4`: hostname、timeout、request ID。結果はIPv4 addressとcache hit flag。
- `TcpConnect`: hostnameまたはIPv4、port、timeout。結果はowner固有connection handle。
- `TcpSend`: handle、bounded payload、timeout。結果はACK済みbyte数。
- `TcpReceive`: handle、最大長、timeout。結果はpayloadとpeer close flag。
- `TcpClose`: handle、timeout。FIN handshake開始と完了。
- `GetStackStatistics`: RX/TX、ARP、IPv4 checksum、ICMP、DHCP統計。

request IDはreplyへそのまま返します。magic、version、opcode、declared length、reserved、固定長を
検証し、余分なbyteは拒否します。connectionはIPC sender threadが所有し、別senderのhandle操作は
`EACCES`です。`net` CLIのmanifestは`net.connect`と`ipc.client`を要求します。shellからの
exec chainも親Capabilityの部分集合規則に従って`net.connect`を委譲します。

DNSはDHCP Bound後、TCPはIPv4設定後に利用できます。driver IPCが失敗した場合はinterface設定と
保留packetを破棄し、全TCP connectionを失敗させます。

## 7. 起動とdebug

service-managerはdriver discovery後に`network.service`を起動します。driver processが見つからない
場合は5秒でready failureを返して常駐し、OS bootを停止させません。DHCP成功時のinterface設定、
gateway ARP、最初のEcho Replyは`/system/logs/services/network.log`に記録されます。

```text
/ $ net ping 10.0.2.2
/ $ net resolve localhost
/ $ net tcp-connect 10.0.2.2 8080
/ $ net tcp-send 10.0.2.2 8080 test
/ $ net stats
```

QEMU設定、pcap取得、未対応protocolは[mochiOS Networking](../networking.md)を参照してください。
