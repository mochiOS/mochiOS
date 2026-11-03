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
Vimの要求権限はmanifestで明示し、署名検査とCapability検査を通します。
gettimeofdayを使うためsystem.time.readも必要です。execの権限上限を維持するため、
信頼されたmshにも時計の読み取りを明示しています。

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

Terminalの実transport、VT parser、入力queueと検証付きmsh起動を再利用した実PTY試験を
追加しました。挿入モード後の停止ではreadが入力31byteを受け取り、空のselectも0を返して
いましたが、次の入力を送るまでVim自身がwriteを発行しませんでした。保存した全バイトは
一致しました。

原因はVimのinchar_loopにありました。初回の待機前に経過時間を差し引くため、0msのpollでも
時計取得やスケジュールに時間がかかると負の値になり、下位の端末処理が無期限待ちとして
扱っていました。patches/0001で、この有限待機の計算結果を0以上に保ちます。実際に無期限
待機へ切り替える既存分岐は保持します。build.jsonに適用パッチのSHA-256も記録します。

vim_waits.pyはVimソースの実inchar_loopを取り出してホストでコンパイルし、時計と待機の
callbackを制御します。未修正版では期限切れpollのassertionが失敗し、修正版では0ms、
期限切れ、残り時間、意図した無期限待機の7ケースが成功しました。357の実PTY試験でも
ASCIIの入力が即座に表示されるようになりました。

SDKのnewlibでは_MB_CAPABLEが無効で、localeがCのためVimの既定値がLatin-1でした。
patches/0002はmochiOS向けVimの内部エンコーディングをUTF-8で初期化します。SDKやCの
locale実装は変更しません。ユーザーは通常のVim optionでencodingを変更できます。

358のKVM試験では診断hookを外した通常版Vimで、UTF-8の既定値、localtimeとreltime、
既存のバッチ編集、実PTYでの日本語入力表示、保存全バイト、再読込、Vim終了、msh復帰、
試験directoryの削除がすべて成功しました。今回の実PTYの入力表示は274msでした。
これはVT parserでの検出までの値で、GUI描画を含む性能測定ではありません。
通常のTerminalでも`vim -Nu NONE /system/share/vim/vim92/tutor/tutor1.ja`による日本語文書の
描画を確認しました。`-Nu NONE`を使わない場合、この*.jaを上流のzip pluginがアーカイブと
解釈します。外部unzipはこの移植に含めていません。

別途、時計権限の不足によりgettimeofdayが失敗し、戻り値を検査しないVimの経過時間計算から
異常に長いselect待ちが発生していました。時計権限を明示したあと、直接起動と実PTYの双方で
gettimeofdayの成功と正規範囲の時刻を確認しました。直接操作の保存・終了も通りました。
改修前の試験を強制中断した際にはVimの_free_rでfaultも出ました。今回確認したのは
通常の保存・終了で、強制hangupの経路は別途検証が必要です。

services/posix/tests/vim/io_trace.cは実VimにLLDの--wrapで付ける診断専用hookです。
通常ビルドには入りません。read/select/writeの引数・結果と時計取得の結果だけを記録し、
入力内容は記録しません。DEBUG_WRITEにはコンパイル時に現在のmnu ABIの値を渡します。
調査用イメージには試験専用の実行ファイルが含まれ、通常配布イメージとは区別しています。

この段階はホストでクロスビルドしたVimの実行です。mochiOS内でVimを自己ビルドしたことや、
完全なPOSIX互換を示すものではありません。WebKitの検証は行いません。
保護されたmsh / GUI同意画面の本番接続と動的リンカの完成は、引き続き別の未完了項目です。
