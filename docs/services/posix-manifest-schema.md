# 実行宣言の共通TOML schema（327）

user/crates/package-manifestへ、ツールとサービスで共用するno_std + allocのparserと
PackageManifest型を置きました。platform::packageは同じ型・parserを再公開し、
capability.serviceの既存のmanifest認証・依存関係・要求Capabilityの読取もこれを使います。
msign elf sign --manifestも、埋め込む同じ宣言をこのschemaで検査します。

従来の行単位parserは、重複キーの上書き、不正escapeの受理、コメント・配列中の文字の
取り違えや未完了配列による要求の欠落を起こせました。標準TOML parserのparse / serde機能を
stdなしで使い、syntaxと型を検査してから既存のパス・package・dependency検査へ渡します。
旧parserへのfallbackはありません。旧行parserを撤去し、既存の検査を共通crateへ移しました。

文字列・配列・数値の型不一致、重複キー、曖昧なvendor / developerやpath / entryの併記、
重複・空・制御文字を含む要求Capabilityを拒否します。正規のUnicode escapeやliteral string、
引用符内のcomma / #、複数行配列はTOMLの規約で解釈します。
parserの既定の深さ制限を維持し、極端に深い配列の入れ子も拒否する試験を加えています。
[service] / [driver]の既存entry形式も同じtyped modelへ変換します。

一般manifestは1MiB、主要entry配列は8192、dependencyは256、requiresは256を上限とします。
ELF内manifestは既存codecの64KiB上限に従います。構文検査やpackage ID・provenanceの宣言から
信頼や権限を作りません。Install Recordと元bytesの照合、署名者の信頼、共通の実行認可、
保護された同意と最終grantの結合は各認証境界が引き続き担当します。
通常exec全体への署名検証強制と、完全POSIX環境のgoalは未完了です。

## 検証

関連ホスト79件、既存platform parserの5件、msign既定構成34件、実LLD / driver / msignの3件、
配置済み37パッケージを検査する1件が成功しました。ソース内のpackage manifest群も走査しています。
CExtの[cext] ABI manifestは別形式です。coreとallocだけのx86_64-unknown-none向けcheckも通り、
parserにstdやサービスI/Oを要求しないことを確認しました。

古いdynamic grantのパス再読込を前提にしていた検査は、現在その処理が存在しないことを
検査する形へ更新しました。実manifest読込のI/Oエラー保持・一回の読込と、package認証・依存関係の
試験は継続しています。テストはservices/posix/tests以下にあります。

記録はout/checks/mochios-manifest-schema-327-のhost.log、platform-regression.log、
msign-default.log、real-host.log、installed-host.log、nostd.log、native-build.logとartifacts.jsonです。

検証用targetのincremental cacheがディスクを圧迫していたため、実行中の利用者がないことと
Cargo lockを確認して、再生成可能な同cacheだけを削除しました。ソース、SDK、実行イメージと
検証記録は対象に含めていません。記録は同接頭辞のcache-cleanup.jsonです。

更新済みcapability.serviceで通常起動し、Binder READY、デスクトップ、DockからのTerminalと
msh promptを確認しました。通常PATHのclang / ld / ld.lld / msignでmanifestを含むELFを生成し、
元宣言との一致、入力manifest削除後のstdoutと終了値37、構文エラーと型エラーの両方での署名拒否と
旧ELFのbyte一致、宣言改変の検証拒否、cleanupとprompt復帰が成功しました。
画面は同接頭辞のresult.pngです。追加のCapabilityを付与する試験ではありません。

VM停止・解析処理の復帰後、所有するprobeだけを撤去し、更新したcapability.serviceと対応する
verification record、通常msignを保持してイメージを再生成しました。401ファイルと4つの
command symlinkを読み戻して照合し、probeの不在と6成果物のchecksumを確認しました。
SDKの入力hashは不変で、SDKとRust std overlayは再生成していません。
記録はnormal-image.logとreadback.jsonです。
