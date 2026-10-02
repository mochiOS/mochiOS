# mochiOSのパッケージ

この文書はパッケージ機構の概念だけを説明します。形式や検証規則はここでは定義しません。

mochiOSでは、アプリケーション、サービス、ドライバー、CLI、ライブラリをパッケージ単位で配布・管理します。

パッケージは、実行ファイルだけでなく、次の情報を1つの配布単位へまとめます。

- Package ID、名前、バージョンなどの識別情報
- インストール対象ファイル
- アプリケーション、サービス、ドライバーなどの起動定義
- Capability要求
- manifest署名とDeveloper Certificate

配布形式は`.mpkg`です。image同梱のbuilt-in manifestは`/system/packages/<package>/manifest.toml`、後からインストールしたmanifestは`/var/lib/packages/<package>/manifest.toml`へ配置し、実行ファイルやアプリケーションbundleとは分離して管理します。

主な責務は次のように分かれます。

| コンポーネント | 責務 |
| --- | --- |
| `package.service` | MPKGの受け取り、検証依頼、payloadの配置、atomic更新・削除、インストール済み一覧 |
| `signature.service` | manifest署名とpayload整合性の検証 |
| `capability.service` | manifestとPolicyに基づくCapability解決 |
| カーネル | プロセス生成、実行ファイルのロード、Capabilityの矯正 |

`.mpkg`のコンテナ形式、manifest schema、署名対象、パス規則、インストール手順の正本は[mochiOS Package Format](mpkg.md)です。

Root Certificate、Developer Certificate、失効との境界は[mochiOSの証明書と署名検証](certificates.md)を参照してください。

Storeなどの管理クライアントは共有`mochios-package-protocol`を使い、要求ID付きで
install、update、remove、listを呼び出します。それぞれ`package.install`、
`package.update`、`package.remove`、`package.inspect` Capabilityが必要です。
クライアントはファイルを直接配置せず、署名と所有者継続性の最終判断は常に
`package.service`が行います。
