# Web PKI trust store

## 信頼境界

HTTPS用Web PKIと、MPKG署名用mochiOS Developer PKIは別の用途、別のRoot集合です。

| trust store | 用途 | 実装 |
| --- | --- | --- |
| Web PKI | HTTPS server証明書 | `mochios-tls-client`内のread-only組み込みbundle |
| mochiOS Developer PKI | MPKG/実行binary署名 | PackageIndexとDeveloper Certificateの既存経路 |

Web PKI Rootでpackage署名を許可せず、mochiOS Rootで一般Web serverを信頼しません。

## 現行bundle

本番bundleの識別情報は次です。

| 項目 | 値 |
| --- | --- |
| source | Mozilla Included CA Certificate Reportを元にした`webpki-roots` |
| crate version | `webpki-roots 1.0.9` |
| anchor count | 121 |
| Cargo source `src/lib.rs` SHA-256 | `581232b6fb8d5b8df315d34c798099ee759cb4930ffed77c331e7b38fed82b15` |
| runtime source | `webpki_roots::TLS_SERVER_ROOTS` |

各Rootのlabel、issuer、subject、serial、SHA-256 fingerprintはこの固定versionの
`webpki-roots/src/lib.rs`にRootごとに記録されています。このversion、件数、source hashの組を
現行組み込みRoot一覧の識別子とします。`user/Cargo.lock`が解決versionを固定し、
`WEB_PKI_ROOTS_VERSION`をTLS crateが公開します。単一leaf pinningは使用しません。

bundleはbinaryへread-only dataとして組み込み、実行時にfilesystem、network、Developer PKIから
追加しません。自動更新と署名付きbundle更新は未実装です。Root変更は依存version更新、Root一覧差分、
証明書fixtureとAccounts E2Eを含む通常のcode review対象です。

## test Root

決定的Smoke TestのRootは`test-web-pki` featureでのみ
`user/crates/tls-client/test-fixtures/test-root.cert.pem`から読み込みます。本番network.serviceの
featureには含めません。build scriptは`MOCHIOS_NETWORK_TEST_WEB_PKI=1`が明示されたtest imageだけで
有効化します。test private keyはfixture専用であり、本番trust storeや製品credentialではありません。

## 検証範囲と制限

chain、署名、Root、時刻、SAN hostname、Basic Constraints、Key Usage、Extended Key Usage、
critical extension、path lengthをrustls/webpkiで検証し、mochiOS側でchain depthとsizeを追加制限します。
SHA-1 signature algorithmは無効で、SHA-1署名leafの拒否fixtureを単体テストだけで使用します。OCSP、
CRL、certificate transparency、動的trust store更新は現時点で未対応です。失効確認が必要な
高リスク用途では、この制限を前提に別途設計が必要です。
