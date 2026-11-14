# アプリの資源アクセスとOS確認

アプリが選んだパスやUIDを権限として扱わず、認証した実行主体と、OSが発行した
Handleを基準にファイル・専用保存領域・設定変更を許可します。
アプリ向けAPIの使い方は[AppCoreガイド](../../tools/devkit/crates/appcore/docs/usage.md)を参照してください。

## 責任の境界

mnuは汎用Handleの所有権、権限の縮小、転送とIPCを提供します。
POSIXは実UID・実効UID、グループ、プロセスのセッション情報を管理し、
filesystem.serviceは実際のパス解決と名前空間の認可を一つの操作として行います。
アプリの識別や保存先の配置、利用者への確認はユーザー空間の各brokerが担当します。

## 選択ファイル

既存のFiles.appのpickerを使い、Workspaceが選択対象を開いて、必要な権限に縮小した
ファイルHandleをアプリへ渡します。表示用パスや、アプリから再送されたパスを使って
後から権限を増やしません。読み取りと編集を区別し、読み取り用の返答にはWRITEを含めません。

保存先は、Workspaceが親ディレクトリと一つの名前を保持する別のネイティブオブジェクトです。
アプリのbegin・commit・abort要求は、そのオブジェクトをCONTROL権限付きのIPC添付で渡します。
要求の本文から別の宛先を指定することはできません。保存時は一時ファイルを同期してから
保持した宛先へrenameし、親ディレクトリも同期します。rename後の同期失敗はエラーとして返します。

実装は[保存先broker](../../services/workspace/src/file_targets.rs)と
[保存トランザクション](../../services/workspace/src/save_transaction.rs)にあります。
旧パスAPIと`fs.*.user`の広い権限は残るため、アプリのI/Oを移行してからmanifestの権限を外します。

## アプリ専用領域

capability.serviceが認証したBundle ID、developer、署名鍵、provenanceと、POSIXが確認した
UID・アカウント世代を使い、次の領域を発行します。

```text
/libraries/applications/{BundleID}/users/{uid}-{generation}/private/{provenance}-{key}-{developer}
```

要求にはパス、利用者名、UIDを含めません。同じUIDが再利用されても世代の違う領域を使い、
別の署名者が同じBundle IDを名乗っても同じ領域を取得できないようにします。
作成時はディレクトリHandleを保持して各成分をnofollow・beneathで開き、所有者を確認します。
アプリへはその領域の読み書きに必要な権限だけを渡します。
`fs.*.user`によるパスアクセスには、このprivate領域を含めません。

[領域の識別規則](../../services/capability/src/application_data_identity.rs)と
[発行・旧データ移行](../../services/capability/src/application_data.rs)が実装です。
CalculatorとEditは専用領域へ移行し、既知の旧設定・履歴を取り込んだ後も元データを残します。
両アプリのmanifestから`fs.read.user`・`fs.write.user`を外しています。
Viewerはpicker・受信文書をHandleで扱いますが、フォルダー移動とパスによるD&Dには
`fs.read.user`への依存が残っています。

## 既定アプリの変更

通常のアプリは、自身のBundle IDと宣言済みの文書形式・役割について変更を提案できます。
Workspaceは提案を保持し、OSが起動したFilesの確認画面からの返答で確定します。
確認tokenだけでなく、起動した確認プロセスと組み込みアプリの実行主体を照合し、
要求元の実行イメージ、候補アプリ、変更前の関連付けが変わっていれば確定しません。
取消、確認プロセスの終了、期限切れでは変更しません。

[確認処理](../../services/workspace/src/association_confirmation.rs)を参照してください。
旧`file-association.write`はsystem専用で、従来の直接書き込みopcodeは受け付けません。

## 本人向けアカウント操作

user.serviceのSelfInfoとChangeOwnPasswordは対象UID・利用者名を受け取りません。
対象は認証した送信元のPOSIX資格情報から決定し、実UIDと実効UIDが異なる場合は拒否します。
SelfInfoはパスワードハッシュを含まない公開レコードを返します。

パスワード変更は組み込みのSettingsまたはsecure-uiから受け付け、現在のパスワードを検証します。
失敗回数を利用者ごとに制限し、保存前にも要求元を再確認します。変更には既存のKeychain連携を使い、
アカウントDB更新前に旧・新パスワードのwrapperを準備します。他アカウントの管理操作は別の経路です。
`account.self.modify`はsystem専用とします。

[本人確認](../../services/user/src/self_account.rs)と
[user.serviceの受信処理](../../services/user/src/main.rs)を参照してください。
