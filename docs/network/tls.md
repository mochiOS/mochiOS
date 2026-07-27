# TLS 1.3 client

## 対応範囲

mochiOSのTLS clientは`mochios-tls-client`共有`no_std` crateと`network.service`の
接続管理で構成します。暗号処理とTLS state machineにはrustls 0.23、暗号primitiveには
RustCryptoを使用する`rustls-rustcrypto`を利用し、独自暗号実装はありません。

対応する設定は固定です。

| 項目 | 現行値 |
| --- | --- |
| protocol | TLS 1.3のみ |
| cipher suite | `TLS_CHACHA20_POLY1305_SHA256`、`TLS_AES_128_GCM_SHA256` |
| key exchange | X25519のみ |
| SNI | DNS名を小文字化し、末尾の`.`を除いて送信 |
| client authentication | 未対応 |
| session resumption / 0-RTT | 無効 |
| ALPN | 未指定 |

IPv4 literalとwildcardを接続先名として受理しません。HTTPS v1はDNS名を必須とし、証明書の
SANをrustls/webpkiで検証します。Common Nameだけへのfallbackは行いません。

## 証明書検証

Web PKI trust store、UTC、接続先DNS名を使い、次をfail closedで検証します。

- DER構文、証明書署名、issuer chain、信頼済みRoot
- `notBefore`と`notAfter`
- Basic Constraints、CA flag、path length
- Key UsageのdigitalSignature、Extended Key UsageのserverAuth
- SAN dNSName、ASCII case-insensitive一致、左端1 labelだけのwildcard
- 不明critical extension
- chain depth、証明書単体size、chain合計size
- CertificateVerifyとFinished

SHA-1用signature algorithmはproviderの証明書検証一覧とTLS signature scheme一覧に含めません。
失効確認（OCSP、CRL）は未対応です。
Web HTTPS用trust storeはMPKG署名用mochiOS Developer PKIとは別の信頼境界です。詳細は
[Web PKI trust store](trust-store.md)を参照してください。

## 状態とclose

公開wrapperは次の状態を持ち、handshake内部のServerHello、EncryptedExtensions、Certificate、
CertificateVerify、Finishedの遷移はrustlsのstate machineが管理します。

```text
Handshaking -> Established -> Closing -> Closed
                  |              ^
                  +-> PeerClosed-+
任意の処理失敗 -> Failed
```

確立前のApplication Dataは上位へ返しません。複数recordに分割されたhandshakeと、1回の受信に
複数recordが含まれる場合を処理します。AEAD認証失敗、fatal alert、不正message、過大recordは
接続失敗です。終了時は`close_notify`を送信してTCP FINを開始します。peerが先にTCP FINを送った
`CloseWait`でもclose_notifyを返してからlocal FINを送れます。

## 上限

| 対象 | 上限 |
| --- | ---: |
| TLS connection | 16 |
| hostname | 253 bytes |
| plaintext | 16 KiB/record operation |
| TLS record buffer | 18,437 bytes |
| handshake input buffer | 64 KiB |
| 証明書単体 | 64 KiB |
| 証明書chain合計 | 64 KiB |
| 証明書chain depth | 8 |
| subject / issuer表示 | 各512 bytes |
| TLS IPC data | 4096 bytes/message |

connection handleはkernel CSPRNGで生成し、IPC sender threadをownerとして保存します。別senderの
操作は`EACCES`です。buffer上限、接続上限、乱数未初期化、UTC不明はいずれも成功扱いしません。

## IPCとCapability

`mochios-net-device-protocol` v1の`TlsConnect`、`TlsSend`、`TlsReceive`、`TlsClose`を使用します。
wire headerはmagic `MNET`、version、opcode、message length、reserved、request IDを持つ24 bytesの
little-endian形式です。余分なbyte、短いmessage、未知opcode/version、非0 reservedを拒否します。

TLS操作には`net.tls.connect`が必要です。raw TCPの`net.connect`だけではTLS IPCを実行できません。
乱数とUTCを取得する`network.service`だけが`system.random.read`と`system.time.read`を持ちます。

## 診断

```text
/ $ net tls-connect accounts.mochios.org 443
Connected to accounts.mochios.org:443 (...)
TLS version: TLS 1.3
Cipher suite: TLS_CHACHA20_POLY1305_SHA256
Server hostname: accounts.mochios.org
Certificate subject: ...
Certificate issuer: ...
Certificate validity: ..... ... (Unix UTC)
```

秘密鍵、ephemeral secret、traffic secret、session key、乱数は出力しません。`net stats`は接続、
handshake、証明書、hostname、record、AEAD復号失敗の集計だけを表示します。

## 検証

`make tls-http-smoke-test`は本番trust storeへ混ぜないtest RootとTLS 1.3 serverをホスト側に起動し、
QEMUから正常handshake、Content-Length、chunked、close_notifyを確認します。未信頼CA、hostname不一致、
期限切れ、不正CertificateVerify、暗号record改ざん、応答しないpeerのtimeoutも実際の失敗結果と統計で
確認します。

外部E2Eは`make accounts-https-smoke-test`です。Mozilla由来Root、実UTC、DNS、SNI、hostnameを使い、
`https://accounts.mochios.org/health`へ接続します。
