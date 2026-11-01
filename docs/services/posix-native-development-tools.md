# 通常PATHの開発ツール（321）

`USER_INSTALL_DEVELOPER_TOOLS=y`で、rootfs-stageは提供されたネイティブツールとSDKを
`/system/developer`へ配置します。既定値はnで、未構築のLLVMを通常イメージのビルド時に
暗黙に取得・構築しません。この検証用workspaceの設定はyです。

| 入力 | イメージ内 |
| --- | --- |
| out/newlib-port/sdkのCRT・script・runtime・標準ライブラリ・headers | /system/developer/lib、sysroot |
| out/llvm-port/build-target/bin/clang-18、bin/lld | /system/developer/bin/clang、ld.lld |
| out/llvm-port/build-target/lib/clang/18/include | /system/developer/lib/clang/18/include |
| out/rust-std/target/x86_64-unknown-mochios/releaseのld、msign | /system/developer/bin/ld、msign |

`/system/bin`のclang・ld.lld・ld・msignは相対symlinkです。LLVMのrealpathと、runtimeが
起動時に保持するgetexecnameのcanonical pathにより、既存driverの隣接SDK検出を使います。
カーネルへSDKの配置、PATH、サービス名の知識は追加しません。

配置は`scripts/mmake/stage-development-tools.py`へ共通化しました。SDKは生成・再生成せず、
既存manifestのversionと内容を維持します。compiler_runtimeのlibgcc / compiler-rt選択は
ldと同じファイル名を使います。欠落・空入力、不正なmanifest、異なるarchitecture、
ELF interpreterを要求するtool、既存の配置先との衝突は公開前にエラーとします。
ELF64 / x86_64 / staticの検査はbuild profileの確認であり、出所の認証やCapability付与ではありません。

SDKとresource headersだけをコピーし、入力tree外へのheaderリンクやディレクトリへのリンクを
取り込みません。既存のtool pathを上書きせず、配置途中の失敗では自身の出力を回収します。
umask 077でもSDKのdirectoryは0755、toolは0755、それ以外のfileは0644です。

msignはオンライン証明書取得を省いた既存実装のnative buildを使用しました。
ネットワーク依存を含むmsignのnative移植、C++ SDKの配置、実行時の共通署名検証と
保護されたCLI / GUIのCapability同意、完全POSIX互換性は引き続き必要です。

配置の5試験、stage-rootfs.shの構文確認とrootfs-stageのdry-runが成功しました。
記録は`out/checks/mochios-development-tools-321-host.log`と同接頭辞のdry-run.logです。
native_clang_driverは既定で通常PATHを使い、診断SDKだけを明示するときは
`--tool-directory PATH`を指定します。clangへSDKやtargetの追加引数は渡しません。

通常Dockから開いたTerminal / mshで、ツールの絶対パスやMOCHIOS_SDKを指定せずに
native_clang_driverの全gateが成功しました。clangのtarget取得14,424ms、
compile / link / 自動署名 / msign verify 105,946ms、全試験211,024msでした。
./mainの正確なstdoutと終了値37、compile / link失敗後の旧ELFのbyte一致、
msignによる改変拒否、cleanupとprompt復帰を確認しています。負荷やcache条件を
揃えていないため、319の診断配置との性能比は主張しません。
画面はout/checks/mochios-development321-complete.png、入力と結果は
out/checks/mochios-development-tools-321-artifacts.jsonです。

検証VMを停止し、解析プロセスの復帰処理を終えました。一時probeだけを撤去して通常イメージを
再生成し、既存実行体・SDK・toolの398ファイルと4つのsymlinkをimageから読み戻して照合しました。
提供SDKの入力hashは変わらず、probeの不在と配布成果物6件のchecksumも確認しました。
記録は同接頭辞のreadback.jsonとnormal-image.logです。製品の393ファイルと4リンクは保持しています。
