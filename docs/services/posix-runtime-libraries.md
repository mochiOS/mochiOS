# 実行中プロセスの共有ライブラリ

`posix-client::pie::link_scope::resident`は、既存のScopeとVM配置を使って実行中のプロセスへ
共有ライブラリ群を配置する共通処理です。path探索・取得・認証は呼出側が担当し、ここでは
明示された捕捉済みscopeだけを扱います。カーネルAPI、POSIX wire、SDKは変更していません。

`Resident`は呼出側のslot配列と一つの再利用可能な出力bufferを使います。
scopeで再配置した各objectを既存Installationから独立したMappingへ移し、全memberの配置と
RELRO保護が終わってからconstructorを呼びます。W+Xを作らず、既存のmappingも置換しません。
出力bufferは配置後に上書きできます。元の解析済み入力はscopeの借用として保持します。

初期化は依存先から、終了処理は依存元から行います。init/fini arrayの値は保持中の元入力と
同じsymbol resolverから計算し、callback前に全範囲を検査します。アプリが実メモリ上の
終了処理配列を書き換えても、それを新しいfinalizerとして採用しません。
通常のexport取得は初期化完了後にだけ可能で、Symbolはmappingを借用します。
その借用中に同じResidentをcloseすることはできません。raw pointerへ変換する呼出側には、
ABIとライブラリ寿命を守る責任が残ります。

配置失敗では全memberを巻き戻します。最初の失敗とcleanup失敗を両方返し、解放できなかった
範囲は所有したまま再試行できます。Installationのretain_failedは失敗した範囲をscratchの
借用から切り離し、callback再開前に上位ownerへ登録します。失敗したownerからexportは返しません。
constructorの失敗後もmappingを保持し、明示的な終了処理・解放を必要とします。

ホストでは実Clang 18とLLDでDSOを作り、constructorの依存順、DSO間の参照、exportされた
関数の実呼出し、finalizerと実VM解放を確認しました。各VM操作の失敗注入、解放の連続失敗、
scratch破棄後の再解放、変更されたfini arrayの拒否を含む7件が成功しています。
追加レビューではRELROが元のRX LOADに含まれる場合にも実行権限を保持するよう直し、
実VMのページ権限と関数実行を検査する8件目を追加しました。
既存のVM配置7件、export検索4件、許可要求80件を合わせて7対象99件が成功しています。
記録は`out/checks/mochios-resident-363-regression.log`です。

362のKVMでは、通常DockのTerminal / mshから同じClang / LLDのDSOを読み、native VMで
再配置、constructor、関数呼出し、2個のfinalizer、解放後のMemoryCopy拒否を確認しました。
成功画面は`out/checks/mochios-resident-362-result.png`です。RX RELROの追加ケースはホストの
実VM試験で、KVMで追加検証したものではありません。初回のVMは入力操作後に進められず停止し、
再起動後は小さい相対マウス入力でTerminalを開いて試験を完了しました。

試験後に5個の診断用ファイルを除き、更新した通常サービスを保持してイメージを再生成しました。
2,597ファイルと4個のcommand symlinkを読み戻し、6成果物のchecksum、SDKとkernelのhash不変を
確認しました。363の通常起動でもBinder、Dock、menubarの表示を確認しています。
NICなしrunnerの既存network readiness失敗(-6)は残ります。所有VMは停止し、一時停止記録の
復帰も完了しました。記録は`out/checks/mochios-resident-361-{artifacts,readback}.json`と
`out/checks/mochios-resident-363-normal-{boot.log,desktop.png}`です。

現時点のResidentは、追加TLS module、IFUNC / IRELATIVE、COPY relocation、preinitを要求する
入力を配置前に拒否します。起動時scopeでの既存対応を削除したものではありません。
公開C ABIのdlopen / dlsym / dlclose、process内の共有namespace・参照数、初期scopeとの合流、
動的TLSとresolverの接続は未完了です。production ld.soの完成を示す段階ではありません。

今回の共通実行許可への接続後も、実Clang / LLDで作ったDSOを使うResidentの8件を再実行し、
constructor / finalizer、関数呼出し、RELRO、失敗時の実VM解放が成功しています。
記録は`out/checks/mochios-consent-368-resident.log`です。新しいnativeのdlopen試験を実施した
という意味ではなく、公開C ABIへの接続は上記のとおり残っています。
