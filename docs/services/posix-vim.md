# Vimのネイティブ実行と配布ビルド

Vim 9.2.0000とncurses 6.5を、既存のmochiOS SDKを入力としてホストでクロスビルドします。
SDKの生成はこの処理に含めません。ソースはscripts/vim/sources.tomlのURLとSHA-256で固定し、
Gitリポジトリの作成・clone・ソースのルートリポジトリへの取り込みは行いません。

配布物は/system/bin/vim、/system/share/vim、/system/share/terminfoとライセンスです。
端末版・通常機能・UTF-8を有効にし、GUI、X11、Wayland、ホスト側pkg-configへの依存を
無効にします。ホストのticでxterm / screen / tmuxなどのterminfoを生成します。

## 既存SDKを読み取り入力にするビルド

対応するnative runtime archiveを明示します。今回の互換APIを含む独立runtimeを使って
検証しました。runtime archiveの生成とSDKへの組み込みは別の処理です。

```sh
python3 scripts/vim/build.py \
  --sdk out/newlib-port/sdk \
  --runtime out/vim-port/runtime/x86_64-unknown-mochios/release/libmochi_user_newlib_runtime.a
```

出力はout/vim-port/releaseです。build.jsonへソース、runtime、CRTと配布ELFのhash、
実際のLLDのversionを記録します。.readyは署名検証まで成功したELFのSHA-256です。
失敗時は.readyを公開せず、手前の配布物を成功扱いしません。
出力の所有markerと排他lockを検査してから専用workを再生成します。

ホストのx86_64-elf-gccをCコンパイルに使い、Autoconfのリンク試験を含めてすべての
ターゲットリンクを実際のLLDで行います。ld.lldがPATHにない場合はインストール済みRustの
LLDを使えます。必要なら--cc、--lld、--msignを指定します。
scripts/vim/cc.pyはこの移植ビルド専用のadapterで、OSにインストールするldとは別です。
未対応のoptionを黙ってホストリンカへ渡しません。

CRT、linker script、libc、libm、compiler runtimeは指定SDKから解決します。
compiler_runtimeがlibgccかcompiler-rtかをSDK manifestから選択し、Rust runtimeに必要な
既存C helperは同じSDKのヘッダーでコンパイルします。
インストール時のstripが完了してから既存msignで署名し、その最終バイト列を検証します。

通常のmmakeではUSER_INSTALL_DEVELOPER_TOOLS=yのときVimをビルド・収録します。
通常グラフの既存runtime / Rust toolchain準備後、Vim用の独立runtimeをビルドします。
rootfs-stageは.readyのhashを照合し、共通のbuilt-in manifest処理でVimを登録します。
Vimの要求権限は通常mshの範囲内で、署名検査やCapability検査を省略しません。

## 検証範囲

Vimのリンクで見つかったgetpwuid / getpwnam、getgrgid / getgrnamと_r版の不足、および
newlibの16bit UID/GID ABIは公開アカウント経路へ実装しました。
詳細は[公開アカウント情報](posix-public-accounts.md)を参照してください。

services/posix/tests/native_vim.rsは通常のCommand経路で実際のVimを起動します。
実C APIによるUID/GID照会とfault検査のあと、Vim自身でUTF-8の文書を編集し、undo、redo、
検索、保存、buffer破棄後の再読み込み、インストール済みsyntax runtimeを確認します。
最後に親processが保存ファイルの全バイトを比較し、試験用directoryを削除します。
Vimを模したプログラムやPOSIX APIの成功stubは使いません。

343〜345のKVM試験で上記のバッチ試験が成功しました。最初の対話試験ではイメージ内の
Terminalが古く、さらに検証用フォント設定も可変幅フォントを代用していました。
現行Terminalを再ビルドし、0byteだった標準UDEV Gothicの生成物をfonts.confの既存指定から
ライセンスとともに復元しました。検証専用の代替フォント設定も標準設定へ戻しました。

対話試験ではTerminalがEnter/TabのKeyPressedを処理したあと、ViewKitが送るTextInputの
制御文字もPTYへ送っていました。これによりVimへ余分な改行が入り、終了後のmshへも
空コマンドが届きました。制御キーは既存のsend_keyで処理し、同じキーのTextInput表現を
二重送信しないようにしました。通常の文字・IMEの複数行commit・pasteの経路は保持します。
この修正のbyte変換と、既存のpartial-write / backpressure / UTF-8 / 順序保持の試験7件が
成功しています。さらに、入力とVT照会への返信を次の描画まで待たずに非同期PTYへ
送信するようにしました。partial writeやWouldBlockの残りは従来のbounded queueで保持し、
後続の入力を追い越しません。描画なしの即時送信とbackpressure復帰の試験も通りました。
公開アカウント関連と合わせ、ホストのRust試験は36件です。
別途、実SDKとLLDでCをリンクし、static ELF・NX stack・C flags保持・未対応profileの拒否を
確認しました。通常mmakeグラフのdry-runも成功しています。

対話操作の確認は未完了です。実際のTerminalで入力表示が遅れるため、描画を含まない
実PTY・msh・Vimの経路を追加で検査しています。バッチ試験の成功を対話操作の成功とは
扱いません。調査用イメージには試験専用の実行ファイルが含まれ、通常配布イメージとは
区別しています。

この段階はホストでクロスビルドしたVimの実行です。mochiOS内でVimを自己ビルドしたことや、
完全なPOSIX互換を示すものではありません。WebKitの検証は行いません。
保護されたmsh / GUI同意画面の本番接続と動的リンカの完成は、引き続き別の未完了項目です。
