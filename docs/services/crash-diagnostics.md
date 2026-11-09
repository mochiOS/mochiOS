# ローカルクラッシュ診断

CPU例外で終了したプロセスについて、終了時のnative evidenceと、起動時に認証したコードの
対応をローカルに保存します。記録・保持・シンボル保管・ホストでの停止位置解決に加え、
Settingsの履歴・詳細・書き出しと、AppCoreの共通クラッシュウィンドウを提供します。
外部へのクラッシュ送信は接続していません。送信全般は[診断送信](diagnostics-upload.md)を参照してください。

## 責務

- mnuはnative Process/thread ID、確定したimage revision・SHA-256、終了値、CPU例外番号、
  命令・stack・fault address、error code、monotonic時刻を記録します。
  サービス名・アプリ名・ディレクトリ・POSIX signal・保存方針を解釈しません。
- `DiagnosticStreamRead`は既存の`diagnostics.read`を要求します。旧`DiagnosticRead`は維持します。
  新streamのrecordはversion 1、160 bytes。cursor・capacityはrecord境界に合わせます。
  カーネル側は64 recordsの固定履歴で、確保・ファイルI/O・ユーザーランドへの同期通知を行いません。
  上書き件数はreaderへ返します。件数はnative終了recordの数であり、クラッシュの確定数ではありません。
- loggerが既存の診断drain threadから最大8 recordsずつ読み、CPU例外終了だけを保存します。
  コンパイラやgrep等の通常の非ゼロ終了はクラッシュとして保存しません。
- 例外callbackから元の実行へ正常復帰した場合、pending例外情報を破棄します。
  callbackからプロセスを終了した場合と、callbackを使わず直接終了するCPU例外は記録します。
- `capability.service`が認証した起動入力から、アプリの識別情報・バージョン、source ELFの
  SHA-256、初期moduleのload biasとLOAD範囲をloggerへ渡します。
  送信には`capabilities.manage`が必要です。対象は明示添付したWAIT-only Process Handleで選び、
  metadataもREAD-only MemoryObjectで渡します。要求本文のPIDから対象を取得しません。

起動metadataは停止中の子を再開する前に登録します。動的execでは、認証・再リンク後の出力に
対する次revisionのmetadataを登録します。loggerはnative Processの現在値または直後のrevisionだけを
受け付けます。後者は実際の終了recordのProcess/revision/digestが一致した場合だけ参照されるため、
exec失敗時の予約metadataを別の実行へ適用しません。metadataの記録は実行許可を与えません。
診断側の不調は、既に認証された起動・execを成功と偽装したり、その許可範囲を変更したりしません。

## 保存と保持

保存先はユーザーランドのloggerだけが決めます。

```text
/var/lib/diagnostics/crashes/crash-<collector session>-<cursor>.json
/var/lib/diagnostics/images/image-<session>-<process>-<revision>-<digest>.json
```

親・子directoryはroot所有の0700、fileは0600です。保持したdirectory Handleを基準に操作し、
symlinkを追いません。fileの同期、renameによる公開、directoryの同期後にreader cursorを進めます。
保存失敗時は同じ内容を保持して再試行し、未完了fileを正常なreportとして読みません。
過去の`.pending` fileは次の書き込み前に回収します。

クラッシュは最大128件、image metadataは最大256件、それぞれ合計8MiBを上限とします。
1 fileは64KiB以内です。上限に達した場合は古いfileから削除します。
logger再起動時にはnative履歴を再読するため、重複reportが作られる場合があります。
sessionを分け、前の起動・collectorのPIDとmetadataを誤って対応付けません。

時刻は例外発生時のmonotonic値と、取得可能な場合の収集時UTCを分けて記録します。
metadataが不在の場合は`image_metadata_available=false`となり、アプリ名や配置先を推測しません。
raw memory、stack内容、資格情報、環境変数、コマンドラインは保存しません。
これらの詳細reportをupdate.serviceの外部送信queueへ登録する処理はありません。

## ユーザーごとの閲覧と共通UI

認証済みlauncherが起動元のreal UIDとアカウント世代をimage metadataに登録します。
loggerは保存時にその値をreportへ引き継ぎます。管理者として起動したアプリでも、所有者は
要求元のユーザーです。所有者不明の従来reportは管理者以外へ公開しません。

`MDIAGLS1`（一覧）と`MDIAGRD1`（指定report）のIPC窓口は、native測定で確認した
system built-inのSettingsとWorkspaceだけを受け付けます。senderのProcess Handleを
POSIX側で照合し、real/effective UIDの一致と現在のアカウント世代を要求します。
通常ユーザーには所有UID・世代の両方が一致する記録だけを返し、rootには全記録を返します。
返信前に実行イメージと資格情報を再照合します。任意PIDやパスを指定する閲覧APIではありません。
Settingsにはkernelの`diagnostics.read`を付与しません。

返信はREAD-only MemoryObjectのsnapshotです。一覧は最大128件・128KiB、詳細は64KiB以内。
一覧は起動時にprivate storageから読み込んで更新するため、毎秒全reportを読み直しません。
読み出し・表示・書き出しは同意ストアや外部送信queueを変更しません。

Workspaceは現在のcollector sessionで起動後に発生した記録だけを監視し、既存Settingsの
`--show-crash=<report ID>`で共通ウィンドウを開きます。開いている間の後続記録はまとめ、
閉じた後は30秒の間隔を設けます。診断表示プロセス自身のクラッシュは再表示ループにしません。
再ログイン時に過去の記録を一斉表示しません。監視は独立threadで、通常のWorkspace IPCを止めません。

SettingsのSecurity → Local Crash Reportsから履歴を開けます。AppCoreが取得・詳細整形・
共通ViewKit Dialogを提供し、Details / Reopen / Closeを表示します。履歴・詳細は標準ViewKit、
書き出し先はAppCore SavePanel、操作エラーはAppCore Alertの別ウィンドウです。
再起動は記録されたpackage IDを現在のインストール一覧で解決し、既存の起動認可経路へ渡します。
記録中のパスやCLI引数を直接実行しません。元のドキュメントや起動引数の復元は行いません。
UIのIPCと読み出しはbackground threadで行います。

## シンボルの保管

```sh
mmake symbols
```

完成したrootfs内のx86_64 executable/shared ELFとkernelを走査し、配布するELF全体のSHA-256で
debug情報を保管します。ELF本体を変更しないため、既存の署名・file bindingと同じbytesを参照します。
kernelの分離済みdebug情報はGNU debuglink CRCとも照合します。

```text
out/mmake/symbols/index.json
out/mmake/symbols/objects/<ELF SHA-256>/image.json
out/mmake/symbols/objects/<ELF SHA-256>/image.debug
```

indexにはOS version、各projectのcommit・未コミット変更の有無、設定のSHA-256、対象ELF、
debug fileのSHA-256、LOAD範囲、symbol/line情報の有無を記録します。
通常の`mmake image`にも組み込み、`out/artifacts/symbols/`へそのビルドの必要なobjectsを
書き出します。既存のCIのdevelopment artifactへ含まれ、SHA256SUMSにも記録されます。
保管庫の古いobjectsは、別の配布ビルドの診断に必要な可能性があるため自動削除しません。

mochiOSのRustアプリ・サービスとkernelの通常ビルドにline table生成を指定します。
変更前のELFや外部でdebug情報なしに作成したELFへ、後からソース行を復元することはできません。
既存ビルドのsymbol保管が成功しても、line情報が存在することを意味しません。

## 停止位置の解決

Settingsから必要なreportを書き出し、対応するsymbol artifactを用意します。

```sh
python3 scripts/dev/symbolize-crash.py report.json --symbols out/artifacts/symbols
# machine-readable出力
python3 scripts/dev/symbolize-crash.py report.json --symbols out/artifacts/symbols --json
```

初期moduleに命令位置が含まれる場合は、記録されたload biasを引いて元のsource ELFへ対応付けます。
symbol objectとdebug fileのSHA-256、architecture、executable LOAD範囲を確認してから
addr2lineで関数・ソース行を解決します。metadataなしでも、native digestが一致する固定配置ELFの
命令位置は解決できます。PIE等の配置情報がない場合、違うビルドや推測したbiasへ切り替えません。

## 現在の範囲

- CPU例外の停止位置1か所が対象です。stackのunwindや全threadのbacktraceは実装していません。
- POSIXの通常signal終了や、CPU例外にならない明示的abort/panicは、専用のクラッシュ記録として
  識別していません。非ゼロ終了値だけからクラッシュと判定しません。
- metadataは共通authorizerを通る起動と動的execの初期moduleが対象です。
  ELF interpreter自身や後のdlopenによる読み込みは完全に捕捉していません。
  `initial_modules_complete`と`later_dynamic_loads_captured`を確認します。
- 停止位置が捕捉していないmodule内にある場合は未解決として扱います。
  kernel panicの再起動越しの保存と外部クラッシュ送信はこの実装の対象外です。

試験コードの追加やKVMによるクラッシュ実行・画面の実機確認は行っていません。
