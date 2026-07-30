# mochiOS Networking

この文書は、現行のmochiOS IPv4ネットワーク実装の全体像を説明します。driver固有の
詳細は[virtio-net driver](drivers/virtio-net.md)、service内部は
[network.service](services/network.md)を参照してください。

## 1. 対応範囲

現行実装はQEMUのmodern virtio-net PCI deviceとuser networkingを対象とします。
`network.service`は次を実装しています。

- Ethernet II
- IPv4（optionおよびfragmentは未対応）
- ARP request/reply、32件の期限付きcache、解決待ちpacket
- ICMP Echo Request/Reply
- UDP checksum、port binding、ephemeral port、bounded receive queue
- DHCPv4 client
- DHCPで得たDNS serverを使うA record stub resolverとTTL cache
- outbound専用TCP client（connect、send、receive、close）
- TLS 1.3 client（Web PKI、SNI、X25519、ChaCha20-Poly1305/AES-128-GCM）
- TLS上のHTTP/1.1 client（GET、POST、Content-Length、chunked）
- connected subnetとdefault gatewayのrouting

IPv6、DNS over TCP、DNSSEC、TCP listen/accept、TLS 1.2、HTTP/2、QUIC、Wi-Fi、実機NIC、NAT、
firewall、packet forwarding、promiscuous modeは実装していません。DNSの詳細は
[DNS resolver](network/dns.md)、TCPは[TCP client](network/tcp.md)、TLSは
[TLS 1.3 client](network/tls.md)、HTTPは[HTTP/1.1 client](network/http.md)を参照してください。

## 2. 責務境界

```text
QEMU virtio-net PCI device
        |
        v
virtio-net.driver       PCI、feature、DMA、RX/TX virtqueue、Ethernet frame IPC
        |
        v
network.service         Ethernet、ARP、IPv4、ICMP、UDP、DHCP、DNS、TCP、TLS、HTTP、timer
        |
        v
net                     ping、resolve、TCP/TLS/HTTPS、統計の診断CLI
```

driverはEthernet payloadより上のprotocolを解釈しません。`network.service`はPCI register、
virtqueue、DMA addressを扱いません。

## 3. 起動

`drivers.service`はUSB、PS/2に続いて`/bin/drivers/network`を探索し、manifestが
`bus=pci`、`class=network`に一致するvirtio-net driverを起動します。
`service-manager.service`はdriver discovery完了後に`network.service`を起動します。

`network.service`は有効なDHCPACKを受信した時点でnetwork-readyを通知します。TTYはその前に
起動されるため、NIC不在、link down、DHCP timeoutでもOS全体のbootは停止しません。
network-ready確認後、`service-manager.service`は`update.service`を起動します。同サービスは
`net.http.request`経由でDeveloperCAへ接続し、同期失敗時もbootを停止しません。

## 4. DHCPとrouting

DHCP clientは次の状態を持ちます。

```text
Init -> Selecting -> Requesting -> Bound -> Renewing -> Rebinding
                    |                         |             |
                    +-------------------------+-------------+-> Failed
Failed -- 30秒後 --> Selecting
```

transaction ID、client MAC、magic cookie、message type、option length、server identifier、
offered addressを検証します。ACKは保存済みofferのaddressとserverが一致する場合だけ受理します。
leaseの50%でrenewal、87.5%でrebindingを開始し、期限切れまたは再送上限到達後は一定時間待って
再取得します。

subnet maskが設定済みの場合、同一subnetは宛先IPをARP解決し、それ以外はdefault gatewayを
ARP解決します。設定前の通常IPv4送信は拒否します。DHCPだけは0.0.0.0からbroadcastで送ります。

## 5. QEMUでの起動

通常のrunnerはnetworkを有効にし、固定MACを使用します。

```sh
QEMU_ACCELERATOR=tcg scripts/smoke-test.sh
```

明示設定は次のとおりです。

```sh
QEMU_NETWORK=y \
QEMU_NETWORK_MAC=52:54:00:12:34:56 \
scripts/runner.sh
```

無効化する場合は`QEMU_NETWORK=n`を指定します。packet診断ではpcap出力を指定できます。

```sh
QEMU_NETWORK_PCAP=/tmp/mochios-net.pcap \
QEMU_ACCELERATOR=tcg \
scripts/smoke-test.sh
```

runnerは`virtio-net-pci,disable-legacy=on`とQEMU user networkingを使用します。

通常の`make run`では、QEMU networkingが有効ならhost loopbackのport 20000にTCP echo serverも起動します。
runnerが次のようにguestから接続するaddressとportを表示します。

```text
[run] TCP echo server guest=10.0.2.2:20000 mode=persistent
```

mochiOSのshellから、表示されたportを指定して複数回確認できます。

```text
/ $ net tcp-send 10.0.2.2 20000 mochios-test
```

別のportを使用する場合は`QEMU_TCP_ECHO_PORT=23456 make run`、serverを無効にする場合は
`QEMU_TCP_ECHO_SERVER=n make run`を使用します。Smoke Testでも同じserverを複数接続可能な状態で
起動し、接続直後のcloseとpayload echoを別々の接続で検証します。

## 6. 診断

shellからgatewayまたは任意のIPv4 addressへEcho Requestを送信できます。

```text
/ $ net ping 10.0.2.2
reply from 10.0.2.2: time=0ms

/ $ net resolve localhost
localhost -> 127.0.0.1

/ $ net tcp-connect 10.0.2.2 8080
Connected to 10.0.2.2:8080 (10.0.2.2)

/ $ net tcp-send 10.0.2.2 8080 mochios-tcp-smoke
sent=17 received=17 data=mochios-tcp-smoke

/ $ net tls-connect accounts.mochios.org 443
Connected to accounts.mochios.org:443 (...)
TLS version: TLS 1.3

/ $ net https-get https://accounts.mochios.org/health
Status: 200
Content-Type: application/json
Body:
{"service":"accounts","status":"ok"}

/ $ net stats
```

`net stats`はRX/TX、drop/error、ARP、IPv4 checksum、ICMP、DHCP、DNS、TCP、TLS、HTTPを
表示します。service logは`/system/logs/services/network.log`に保存されます。通常動作では
packet単位のlogは出さず、interface情報と主要な接続状態だけを記録します。

正式な`make smoke-test`はQEMU DHCP DNSで`localhost`を解決し、runnerがloopbackへ起動した
bounded echo serverへguestから`10.0.2.2`経由で接続します。SYN、SYN+ACK、Established、17 bytesの
送信、ACK、同一payloadの受信、FIN完了を実状態のlogとserver側byte数で検査します。外部HTTP
serverはCLI検査の成功条件に使用しません。boot後は`update.service`も並行してHTTPSを使用するため、
共有TCP統計にはDeveloperCA接続が追加される場合があります。

TLS/HTTP専用の決定的検証は`make tls-http-smoke-test`です。test Rootを本番bundleから分離し、
正常なContent-Length/chunkedと証明書・record・HTTP framingの失敗系を確認します。公開endpointの
追加検証は`make accounts-https-smoke-test`です。

## 7. セキュリティ上の制限

- `drivers.service`は委譲のため、virtio-net driverはdevice操作のために
  `device.net`と`dma.allocate`を持ちます。
- `network.service`は`net.raw`を持ち、applicationは直接driver IPCを使用しません。
- frame、各header、checksum、DHCP option、descriptor index、DMA範囲を検証します。
- Ethernet sourceとARP senderが一致し、自分宛のARPだけをcacheへ登録します。
- RX/TX buffer、受信frame、UDP queue、ARP cache、ARP解決待ちpacketは固定上限です。
- DNS message/cache/retryとTCP connection/send/receive queue/timerも固定上限です。
- checksum offload、GSO、multiple queueを受理しないため、software checksumを使用します。

raw TCPの`net.connect`、TLSの`net.tls.connect`、HTTPの`net.http.request`は別Capabilityです。
Web PKIはDeveloper Certificate用Rootと分離され、UTCまたはCSPRNGが利用不能ならTLSを拒否します。
raw TCPへsecret、credential、認証tokenを平文で送ってはいけません。

現行のQEMU user networkingは開発・検証用です。実機NIC、外部公開service、firewallは未実装であり、
この構成を境界networkへ直接接続することは想定していません。
