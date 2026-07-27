# Outbound TCP client

## 対応範囲

現行実装はIPv4上のoutbound clientだけを提供します。`connect、send、receive、close`に対応し、
listen、accept、server、IPv6、SACK、window scaling、advanced congestion control、fast retransmit、
Nagle、keepaliveは未対応です。TLSとHTTPはこのclientの上位層として
[TLS 1.3 client](tls.md)と[HTTP/1.1 client](http.md)に実装します。

## 状態と識別

connectionは`local address、local port、remote address、remote port`の4-tupleで識別し、次の状態を
持ちます。

```text
Closed -> SynSent -> Established -> FinWait1 -> FinWait2 -> TimeWait -> Closed
                         |
                         +-> CloseWait -> LastAck -> Closed
任意の通信状態 --RST/timeout/interface loss--> Reset
```

SYNとFINはsequenceを1消費します。ACK範囲はwraparoundを考慮して検証します。duplicate ACKとsegmentを
識別し、out-of-order payloadはapplicationへ渡さず現在のACKを返して再送を促します。reassembly queueは
ありません。TIME_WAITは30秒保持し、その間4-tupleとephemeral portを再利用しません。
peer FIN後の`CloseWait`でも送信halfを維持し、TLS close_notifyなどの残りdataを送信してACKされた後に
local FINを送ります。

local portは49152から65535で、乱数seedから探索して使用中portとの衝突を避けます。initial sequenceと
64-bit handleもplatform tokenをseedにします。handle衝突時はboundedな別候補探索を行います。

## Header、MSS、Window

送受信ともIPv4 pseudo-headerを含むsoftware checksumを使用し、offloadしません。20-byte header、
data offset、flags、urgent pointer、option length、payload lengthを検証します。SYN、ACK、FIN、RST、PSHを
処理し、URGは未対応として拒否します。未知optionはlengthを検証してskipします。

SYNには`MTU - IPv4 header - TCP header`を上限とするMSSを付けます。MTU 1500では1460です。送信時は
local MSS、peer MSS、16-bit peer advertised windowの最小値でsegment化します。window 0では送信せず、
operation timeoutまで待ちます。receive buffer解放時は更新したlocal windowをACKで通知します。

## Bufferと再送

connectionは最大16件です。connectionごとのsend/receive bufferは各16 KiB、1 IPC requestは4096 bytes
以下です。receive overflowはpayloadをdropし、send overflowはrequestを拒否して統計へ記録します。

初期RTOは500 ms、最大5 retriesの指数backoffです。SYN、data、FINを同じbounded outstanding queueで
保持し、ACKで解放します。高度なRTT推定とfast retransmitはありません。retry上限、明示timeout、RST、
driver IPC失敗はconnection failureとして上位へ返します。timerはconnection数により上限があります。

## 所有権とAPI

IPC sender thread IDがconnection ownerです。`TcpSend、TcpReceive、TcpClose`はhandleとsenderの一致を
毎回検証し、別process/threadによる操作を`EACCES`で拒否します。endpointやhandleをCapabilityとして
譲渡するAPIはありません。利用者は`net.connect`を必要とします。

```text
tcp_connect(host-or-ip, remote_port, timeout) -> handle
tcp_send(handle, bytes, timeout) -> acknowledged length
tcp_receive(handle, buffer, timeout) -> (length, peer_closed)
tcp_close(handle, timeout)
```

## 診断とQEMU検証

```text
/ $ net tcp-connect 10.0.2.2 8080
/ $ net tcp-send 10.0.2.2 8080 mochios-tcp-smoke
```

`tcp-send`は受信payloadが送信payloadと完全一致しなければ失敗します。正式なSmoke Testはrunnerが
127.0.0.1に限定したecho serverを起動し、guestからQEMU gateway `10.0.2.2`へ接続します。SYN、
SYN+ACK、Established、payload ACK、同一payload受信、FINとserver側の17-byte echoを検査します。

`net stats`はconnection attempt/established/failure、segment send/receive、retransmission、checksum、
RST、timeout、send/receive dropを表示します。raw TCPでsecretや認証情報を送信せず、上位TLS APIを
使用してください。
