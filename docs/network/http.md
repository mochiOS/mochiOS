# HTTP/1.1 client

## 対応範囲

`mochios-http-client`はTLS上で動くclient専用の共有`no_std` crateです。GETとPOSTを生成し、
HTTP/1.1 responseをbounded bufferで解析します。network.serviceのv1は接続ごとに
`Connection: close`を使用し、connection reuseは行いません。

requestには次を必ず設定します。

```text
METHOD path HTTP/1.1
Host: hostname[:port]
User-Agent: mochiOS/0.1
Accept: */*
Connection: close
```

POSTは`Content-Type`と実body長から生成した`Content-Length`を追加します。header名のtoken構文、
値の制御文字、CR/LF、Host一致、明示Content-Lengthとbody長を検証します。GETにbodyは付けません。

## URL

標準APIは`https://hostname[:port]/path?query`だけを受理します。userinfo、fragment、IPv4 literal、
wildcard、空label、制御文字、空白、port 0、過大URL/pathを拒否します。schemeはcase-sensitiveな
`https`固定です。

共有crateには最大3回、loop検出、別hostnameの再parse、HTTPSからHTTPへのdowngrade拒否を行う
redirect policyがあります。ただし現行network.serviceは301、302、303、307、308を追跡せず、
すべて`RedirectRejected`として返します。したがってredirect先へ証明書検証なしで移動する経路は
ありません。

## response framing

次を解析します。

- `HTTP/1.1` status line、100から599のstatus code、reason phrase
- case-insensitiveなheader名
- Content-Length
- `Transfer-Encoding: chunked`、chunk extension、zero chunk、trailer
- framing headerがない場合のpeer close

異なる複数Content-Length、Transfer-EncodingとContent-Lengthの併用、chunked以外の
Transfer-Encoding、trailing data、不正chunk size/CRLF、途中EOFを拒否します。同じ値の重複
Content-Lengthだけは許可します。Content-Lengthがbody上限を超える場合はbody完了前に拒否します。

## 上限

| 対象 | 上限 |
| --- | ---: |
| URL | 2048 bytes |
| hostname | 253 bytes |
| path + query | 1536 bytes |
| status line | 1024 bytes |
| header line | 4096 bytes |
| header count | 64 |
| header合計 | 16 KiB |
| trailer合計 | 8 KiB |
| response body | 1 MiB |
| 1 chunk | 256 KiB |
| redirect policy | 3回（serviceでは追跡無効） |
| 保存済みHTTP response | 8 |
| HTTP IPC body/read | 4096 bytes/message |

現行serviceはresponseを上限内で一度保持しますが、IPCはheader/bodyを`HttpRead`で4096 bytesずつ
読み出すstream境界です。bodyを単一の無制限IPC messageへ格納しません。

## IPCとCapability

`HttpRequest`はmethod、timeout、URL、Content-Type、bounded bodyを渡します。結果はstatus code、
response handle、header/body長、Content-Typeです。`HttpRead`でheaderまたはbodyを分割取得し、
`HttpClose`で保存済みresponseを破棄します。wireの共通headerと検証規則は
[TLS client](tls.md)と同じです。response handleもIPC sender所有で、別senderは操作できません。

HTTP操作には`net.http.request`が必要です。`net.connect`または`net.tls.connect`だけではHTTP IPCを
実行できません。内部TLS接続は同じsenderをownerとして作成します。

## 診断と検証

```text
/ $ net https-get https://accounts.mochios.org/health
Status: 200
Content-Type: application/json
Content-Length: 36
Body:
{"service":"accounts","status":"ok"}
```

CLIのbody表示は受信body上限内であり、TLS/HTTP本文をservice logへ出しません。POSTの共有APIとIPCは
実装済みですが、現行`net`診断CLIはGETだけを公開します。

`make tls-http-smoke-test`はContent-Lengthとchunkedの正常系に加え、header超過、不正
Content-Length、不正chunk、HTTP downgrade redirect、body上限超過を確認します。

