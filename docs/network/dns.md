# DNS stub resolver

## 対応範囲

現行の`network.service`はIPv4のA recordだけを問い合わせるstub resolverを持ちます。問い合わせ先は
DHCPACKの最初のDNS serverです。未設定または`0.0.0.0`の場合はpacketを送らず`ENXIO`を返します。
IPv4 literalはDNSを使わずそのまま返します。DNS over TCP、IPv6、DNSSEC、mDNS、DoH、DoT、
negative cache、CNAME追跡は未対応です。CNAMEだけの応答は`ENOTSUP`になります。

## 問い合わせフロー

1. hostnameをASCII lowercaseへ正規化し、labelとhyphenを検証する
2. TTL cacheを検索する
3. ephemeral UDP portをbindし、ランダムなtransaction IDでRD付きA/IN queryをport 53へ送る
4. DHCP DNS addressかつsource port 53からの応答だけを受け取る
5. transaction ID、QR、Opcode、RCODE、question、QTYPE、QCLASS、answerと長さを検証する
6. 最初の有効なA recordを返し、TTLが0でなければcacheへ保存する

`ServerFailure`、transaction ID不一致、timeoutは再試行可能です。初期再送間隔は500 ms、最大3回で
指数backoffします。request timeoutが先に到達した場合はそこで終了します。`NameError`は
`ENOENT`です。FormatError、NotImplemented、Refusedなどは失敗として区別してdecodeします。

## Cacheと上限

cacheは最大32 entriesで、`hostname、IPv4 address、expires_at`だけを保持します。同名insertは更新、
満杯時は最古entryを除去します。TTLはmillisecondsへ飽和変換し、TTL 0は保存しません。複数A record
のうち現行APIが返し保存するのは最初の有効な1件です。negative resultは保存しません。

DNS messageは512 bytes以下、hostnameは253 bytes以下、labelは63 bytes以下です。空名、空label、
非ASCII、英数字とhyphen以外、先頭・末尾hyphenを拒否します。

## Compression安全対策

通常labelとcompression pointerの混在を処理します。pointer先はpacket内に限定し、参照済みoffsetを
記録してloopを検出します。参照は最大16回、展開名は253 bytesまでです。範囲外pointer、予約済み
label形式、truncated label、異常な深さはpacket全体を拒否し、外部byte列でpanicしません。

## APIと診断

IPCの`ResolveIpv4`は`hostname、timeout_ms、request_id`を受け取り、status、IPv4 address、cache hitを
返します。wireは`mochios-net-device-protocol`のlittle-endian v1で、declared lengthとreservedを
検証し余分なbyteを拒否します。

```text
/ $ net resolve localhost
localhost -> 127.0.0.1
```

`net stats`の`dns_queries、dns_cache_hits、dns_cache_misses、dns_timeouts、dns_failures`で診断できます。
正式なSmoke TestはQEMU DHCPが配布する`10.0.2.3`を使い、外部networkに依存せず`localhost`を解決します。
