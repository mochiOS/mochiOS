# mochiOSのパッケージ

この文書はパッケージ機構の概念だけを説明します。形式や検証規則はここでは定義しません。

mochiOSでは、アプリケーション、サービス、ドライバー、CLI、ライブラリをパッケージ単位で配布・管理します。

パッケージは、実行ファイルだけでなく、次の情報を1つの配布単位へまとめます。

- Package ID、名前、バージョンなどの識別情報
- インストール対象ファイル
- アプリケーション、サービス、ドライバーなどの起動定義
- Capability要求
- manifest署名とDeveloper Certificate

配布形式は`.mpkg`です。インストール後のmanifestは`/system/packages/<package>/manifest.toml`へ配置し、実行ファイルやアプリケーションbundleとは分離して管理します。

主な責務は次のように分かれます。

| コンポーネント | 責務 |
| --- | --- |
| `package.service` | MPKGの受け取り、検証依頼、payloadの配置 |
| `signature.service` | manifest署名とpayload整合性の検証 |
| `capability.service` | manifestとPolicyに基づくCapability解決 |
| カーネル | 許可された実行ファイルのロードとプロセス生成 |

`.mpkg`のコンテナ形式、manifest schema、署名対象、パス規則、インストール手順の正本は[mochiOS Package Format](mpkg.md)です。

Root Certificate、Developer Certificate、失効、`/libraries/system/execution.allowlist`との境界は[mochiOSの証明書と署名検証](certificates.md)を参照してください。
