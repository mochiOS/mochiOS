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
公開C ABIの初期対応は以下に記録します。process全体の共有namespace、初期scopeとの合流、
動的TLSとresolverの接続は未完了です。production ld.soの完成を示す段階ではありません。

今回の共通実行許可への接続後も、実Clang / LLDで作ったDSOを使うResidentの8件を再実行し、
constructor / finalizer、関数呼出し、RELRO、失敗時の実VM解放が成功しています。
記録は`out/checks/mochios-consent-368-resident.log`です。新しいnativeのdlopen試験を実施した
という意味ではなく、この時点では公開C ABIへの接続は残っていました。

newlib runtimeに`dlopen` / `dlsym` / `dlclose` / `dlerror`の初期実装と`dlfcn.h`を追加しました。
`RTLD_LOCAL | RTLD_NOW`に対応し、`RTLD_LAZY`も即時解決します。スラッシュのある入力は
呼出元のroot/cwdから開き、裸の名前は`LD_LIBRARY_PATH`と共通の標準探索先から取得します。
set-IDのidentityでは環境の探索先を使いません。依存先のRPATH/RUNPATH処理は起動時の共通実装です。

入力は明示的に渡したREAD|MAPのRAM参照を通してcapability.serviceが検証します。
インストール済みlibraryは認証済みpackageのコード照合とcertificate allowance、単体libraryは
共通の署名検証を使います。返されるfull-file SHA-256を、配置に使う保持中のbytesと照合します。
単体manifestはlibraryを一つだけ宣言でき、privileged/system-onlyの要求は拒否します。
コードは呼出元と同じsecurity domainで実行します。要求されたCapabilityは呼出元の既存権限内で
あることだけを確認し、identity変更、追加grant、
実行permitの発行、同意の永続化は行いません。暗号処理をC API内へ複製していません。

同じ開いたファイルの再オープンは同じhandleと参照数を共有し、最後のcloseでfinalizerとVM解放を
行います。閉じたhandleは世代番号で拒否し、`dlerror`はthreadごとに保持して読取後にclearします。
scopeのborrowは操作ごとに再構築し、永続ownerには保持元bytes、VM、phase、開いたファイル参照を
置きます。入力・名前・配置・検索順が変わったscopeへの再接続は拒否します。
ファイル参照はownerの退役まで保持するため、unlink後のinode再利用で別の実体に化けません。
通常終了では追加groupを逆順に終了してから初期scopeを終了します。unmap失敗はownerを保持し、
失敗した公開closeはsymbolを返さずcleanupを再試行できます。constructorの中断でもleaseを解放します。
loaderの操作はfork用の既存callback-resource gateに登録し、C callback中にtable mutexを保持しません。

未対応の機能を成功扱いにはしません。`dlopen(NULL)`、`RTLD_DEFAULT` / `RTLD_NEXT`、
`RTLD_GLOBAL` / `NOLOAD` / `NODELETE` / `DEEPBIND`、初期executableのexportとの合流、
呼出元初期objectのRPATH参照、追加TLS、IFUNC/COPY/preinitは残っています。
再入は`EBUSY`、未対応modeは`ENOTSUP`です。別groupと共有する依存先は、実際に開いたFileHandleの
device/inodeで検出して`EBUSY`にし、同一group内の同じ実体の重複も配置前に拒否します。
共有moduleの参照数・scopeを合流する処理を省略して二重初期化することはしません。
同じinodeの内容が変更された状態でのroot再オープンも`EBUSY`です。上限は32 group、各64 fileです。

ホストの156件では、実Clang/LLDのDSOの配置と呼出し、捕捉入力の認証、scope再構築、
callback中断、失敗時の再解放、共有依存の検出、実ファイルidentityの境界、起動応答を検証しました。
記録は`out/checks/mochios-dlopen-387-regression.log`、`mochios-dlopen-387-login-race-final.log`、
`mochios-dlopen-389-residency-tests-final.log`、`mochios-dlopen-390-owner-tests.log`です。
一部の試験adapterは以前の共通実行許可APIに追従しておらず、今回その不整合も別commitで修正しています。

389のKVMでは、通常のDock→Terminal→mshから`native_dlfcn.c`を実行し、公開C ABIを通じて
署名付きの依存DSOの関数を呼びました。4 threadの参照共有とerror分離、fork後の独立したclose、
最後のcloseと通常終了のfinalizer順、stale handle、未署名・改ざん・過剰要求の拒否が成功しています。
画面は`out/checks/mochios-dlopen-389-result.png`です。
390でもFileHandleをretireまでpinした状態で同じC試験を通し、別groupから同じ依存実体を
開いた場合の拒否も確認しました。画面は`out/checks/mochios-dlopen-390-result.png`です。

最初の387はログイン待機中の非同期Endpoint応答を不正とする経路で停止しました。
Network応答を含む再現試験を追加し、所有者照合とhealth状態を保持したまま他serviceの応答も
記録するよう修正し、388/389ではBinderまで起動しています。388のC試験は、再リンク後の
試験packageのコード照合情報の更新漏れで起動前に拒否されました。389はbinaryとmanifestを
再照合してから実行しています。これらの失敗をAPI試験成功には数えていません。

SDKとRust overlayは読み取り専用で使用しています。既存SDKの再生成や上書きはしていません。
次回のSDK生成で`libc-port/include/dlfcn.h`と更新したruntime archiveを取り込む必要があります。
C ABIの実体はruntime archiveにあり、独立した`libdl.a`は今回生成していません。
ABIの基準は[Linux man-pagesのdlopen](https://man7.org/linux/man-pages/man3/dlopen.3.html)と
[dlerror](https://man7.org/linux/man-pages/man3/dlerror.3.html)です。上記の未対応範囲は互換性の残りです。

試験用8ファイルを除いた通常イメージでは、2,654ファイルと4個のcommand symlinkを読み戻しました。
全6成果物のchecksumとSDKの6入力、kernelのhashを確認しています。
記録は`out/checks/mochios-dlopen-391-readback.json`と`mochios-dlopen-387-artifacts.json`です。

391の通常起動でもBinder、Dock、menubarを実画面で確認しました。
`out/checks/mochios-dlopen-391-normal-desktop.png`に保存しています。NICなしのnetwork readiness
失敗(-6)は残ります。387〜391の所有VMはすべて停止し、一時停止記録の復帰も完了しています。
