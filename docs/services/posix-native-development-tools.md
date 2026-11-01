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

## 実行ファイルを結び付けたpackage情報（330）

開発ツールを配置するとき、system/packages/development-tools/manifest.tomlも生成します。
package IDはorg.mochios.development-tools、kindはbinaryです。4つのbinary entryは
system/developer/bin内の実際のパスを選び、それぞれのfile entryへ結び付けます。
fileにはコピー後の全bytesのSHA-256、size、0755のmodeを記録します。
ELF headerとloadable segmentの検査もコピー後に行い、入力の事前検査だけには依存しません。

供給されたSDKのmanifest.tomlは元のままコピーし、OSのpackage情報とは別に保持します。
SDKの生成・再生成やファイル書換えは行いません。package情報だけで信頼やgrantは作らず、
stage-rootfsが既存のmsign package built-in-recordを実行して検証recordを生成します。
それをimmutableなSystem generationから既存のverified_manifestとCatalogが認証します。

要求権限は、既存のnative compiler試験で使ったfs.read.all、fs.write.tmp、fs.write.user、
ipc.client、ipc.server、process.spawn、system.random.readを明示します。
これはmshのidentityやtrusted prompt roleを与えるものではありません。
各toolを更に小さいgrant setで動かす試験は別途必要です。

既存のpackage・SDK・command aliasの上書きを拒否します。途中の公開で失敗したときは、
今回公開したSDK、packageとaliasだけを回収し、他のpackageは保持します。
umask 077でもpackage directoryは0755、manifestは0644です。

配置8件、既存の認証・schema・署名・brokerの53件、実際の4ツールを使う1件と、
配置済み38 manifestのschema検査1件、計63件の
host試験が成功しました。実tool試験は既存msignで生成したrecordを本番のresolverとCatalogで
認証し、元codeの受理と改変・切詰め・追記の拒否、manifest差し替えの拒否を確認します。
コピー後の非LOAD trailerを含む計測、公開失敗時の回収と既存package保持も検査しています。
記録はout/checks/mochios-installed-tools-330-のstage-host.log、host.log、real-host.logです。

今回は配置処理と認証metadataの変更です。kernel / IPC / libc / runtimeの境界は変えておらず、
KVMは起動していません。この新packageのidentity・grantで4ツールを実行した証拠や、
通常exec全体への共通署名認可の接続が済んだ証拠とは扱いません。完全POSIX環境のgoalも継続します。
通常イメージにはmanifestとverificationの2ファイルだけを追加し、検証対象403ファイルと
4つのcommand linkを読み戻して照合しました。既存の実行ファイル、SDKとkernelは不変です。
6成果物のchecksumも確認し、記録は同接頭辞のnormal-image.logとreadback.jsonにあります。
