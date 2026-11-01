# 単体ELFに結合するmanifest宣言（326）

単体ELFの要求Capabilityを、後から交換できるsidecarから取得しないための形式を追加しました。
既存のmanifest.tomlを明示的な入力とし、元のUTF-8 bytesをELF内へ入れてから既存msignで
署名します。署名やmanifestの存在だけでは、Identity・開発者の信頼・Capabilityを付与しません。

```sh
clang main.c -Wl,--manifest,manifest.toml -o main
```

ldはmanifest指定を消費し、実ld.lldのリンク後、msignへ同じ入力を渡します。
msignがサイズ制限付きで通常ファイルを捕捉し、TOML構文を検査してから共通codecへ渡します。
入力の変更・削除は、公開済みELF内の宣言を変更しません。作業ディレクトリから暗黙にmanifestを
探索せず、ldの--no-sign / -rとの併用は拒否します。出力でmanifestやそのhard linkを置換せず、
不正な宣言・署名・検証の失敗では、既存ELFを維持します。

共通codecのembed_manifestと署名encoderは、同じsection-table構築処理を使います。
元のprogram headerとLOAD bytesを変更せず、非ロード領域の.mochios.manifestへ格納します。
形式は[ELF内manifestの定義](../../tools/devkit/docs/elf-signature-v1.md#embedded-manifest-declarations)に
あります。署名NOTE v1や正規化digestの規約は変更しません。

verifyとverify_imageは、予約sectionの重複、不正な型・flags・header、64KiBを超える宣言、
NUL・不正UTF-8、他section・LOAD / NOTE領域・header tableとの重なりを拒否します。
verify_imageは同じ捕捉bytesを一度parseし、署名と全file digestを検証してから、宣言を借用する
VerifiedImage::manifestを返します。未検証の別ファイルやpathから宣言を再取得しません。

codecはELF内の構造と整合性を検査し、manifestのschema解釈やCapability policyは担当しません。
capability.serviceの既存の明示的な継承認可も、宣言から追加grantを作りません。
通常execへの署名検証強制、宣言の意味論の検査、既存package認証との統合、保護された確認画面と
最終grantの結合は引き続き必要です。完全POSIX環境のgoalも継続します。

## 検証

関連ホスト38件、実LLD / driver / msignの3件、msign既定構成34件が成功しました。
同じVerifiedImageからの借用、AdHoc / 鍵付き署名での宣言改変拒否、不正なenvelopeと重複・alias・
LOADの拒否、TOML・UTF-8・サイズ制限・symlink入力の拒否、失敗時の出力維持を確認しています。
従来版msignと同一入力のAdHoc出力を比較し、manifestなしの場合のbyte一致も確認しました。

記録はout/checks/mochios-embedded-manifest-326-のhost.log、real-host.log、
msign-default.log、tools-build.log、native-build.logとartifacts.jsonです。
テストはservices/posix/tests以下にあります。

通常DockのTerminal / mshからnative_embedded_manifestを実行し、通常PATHのclang・ld・
ld.lld・msignで全項目が成功しました。追加SDK / target指定なしにstdioを含むCをビルドし、
署名付きELFから正確な宣言を取得しています。入力manifest削除後の./mainのstdoutと終了値37、
不正TOML時の旧ELFのbyte一致、埋め込み宣言の変更後のmsign verify拒否、cleanupとprompt復帰を
確認しました。画面は同接頭辞のresult.pngです。これは実行時のCapability同意の試験ではありません。

VM停止と解析処理の復帰後、所有するprobe 1件だけを撤去し、更新した通常ld / msignを保持して
イメージを再生成しました。401ファイルと4つのcommand symlinkをimageから読み戻して照合し、
probeの不在と6成果物のchecksumも確認しました。供給SDKの入力hashは不変で、SDKとRust std
overlayは再生成していません。記録はnormal-image.logとreadback.jsonです。
