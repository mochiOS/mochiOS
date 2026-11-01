# 共有メモリを使う位置指定オブジェクトread

段階318は、大きい実行イメージの取得で発生していた4KiBごとのProvider往復を減らします。
通常の連続readにあるMemoryObject転送を共用し、既存のHandleReadAtに接続します。
カーネルにPOSIXのFD、filesystem / service / directoryの名前、新しいPOSIX syscallは追加しません。

ProviderのOP_MEMORY_OBJECTは、従来の空payloadか、非zeroのbacking identityと8byteの
feature recordを返します。READ_AT_SHAREDを明示したProviderだけに新しい要求を送ります。
未知のfeature、identityのないfeature、不正な長さはエラーです。Providerのエラーを
未対応として読み替えず、既存の無feature Providerは従来の位置指定readを使います。

OP_READ_AT_SHAREDはsourceのbyte offsetをinlineのu64、転送MemoryObjectのoffsetをbasisに
分けて持ちます。一回の転送は最大256KiBで、FD番号や生ポインタは送りません。
kernelはsource HandleのREADを確認し、所有する一時MemoryObjectのREAD / WRITE / MAPだけを
添付します。受信側はnative deliveryの証拠、添付数、権利、型、長さを確認します。

返却されたbyte数・opcode・object IDだけでなく、source位置も要求と照合します。
cache overlayまでは既存のbacking I/O guardを保持し、user bufferへコピーする前に解放します。
エラー後は転送をやり直しません。明示的な空の成功によるdeclineだけが、未転送時のinline切替を
許します。途中の失敗は完了済みprefixを返し、offsetを進めずに資源を回収します。

filesystem側は保持中inodeへの位置指定readを、再利用するprivate bufferへ行います。
一時MemoryObjectへ公開するのは読み終えたprefixだけです。名前の再検索やshared cursorの変更は
なく、同じ名前の置換後も保持したinodeを読みます。

共通image preparerは、root / interpreter / dependencyの保持領域へ直接読み込みます。
満杯の領域は1byteでEOFを確認してから増やし、最大256KiBの空き領域を次の要求へ渡します。
追加のheap / stack scratchをcallback可能なI/Oの途中へ持ち込まず、既存の所有者が回収します。
rootと依存群の合計byte上限、短いread、capture後の認証、失敗時の所有権は維持します。

ホスト8対象157件とkernelのnative向けcheckが成功しました。2MiBを超える実ELFの取得が
256KiB要求を使い、rootの位置指定readが100回未満となること、入力byteの保持、実ext4での
offset・EOF・unlink後のinode、共有helperの不正返却位置・通信失敗・コピー失敗を確認しています。
記録は`out/checks/mochios-positioned-shared-318-host-final.log`と
`out/checks/mochios-positioned-shared-318-kernel-check.log`です。

通常のDockから開いたTerminalとmshで、native_shared_positioned.cの4グループが成功しました。
524,599byteの位置指定readは420,000,000nsでした。256KiBをまたぐデータ、短いEOFの未書込みtail、
dupの共有cursor維持、msync前のdirty MAP_SHARED byteの反映、unlinkと同名再作成後の元inode、
write-only FDでのEBADF、資源の解放とprompt復帰を確認しました。比較用の旧経路を同条件で
測定していないため、この数値から高速化倍率は主張しません。画面は
out/checks/mochios-positioned318-read-complete.png、所有するstageの記録は
out/checks/mochios-positioned-shared-318-stage.jsonです。

clang試験ではdefault targetのPASSまで確認しましたが、配置したfixtureが旧成果物だったため
時間計測は採用しません。計測版もbuild済みでしたが、配置が古いhashのpathを選んでいました。
後続は静的ELFの別の4KiB経路を発見した時点で中断し、全ケース成功とは扱いません。
VMを記録PIDで停止し、解析プロセスの復帰処理を終えています。所有する診断用395ファイルを
除去し、検証済みのkernel / filesystem / mshを保持した通常イメージを再生成しました。
Terminal / msh / filesystem / POSIX / inputの5実行体をimageから読み戻してstageと照合し、
診断rootとprobeの不在、配布物6件のchecksumも確認しました。記録は
out/checks/mochios-positioned-shared-318-{cleanup,readback}.jsonです。

## 静的ELFの取得経路（319）

静的ELFは共通preparerを通らず、exec前のSHA-256計算とnativeのread_allがそれぞれ
4KiB単位でsourceを読んでいました。newlib-runtimeのdigestは既存VectorBufferを
callback ownerへ登録し、最大256KiBで要求します。全byteのhash、画像byte上限、短いread、
不正な返却長とerrnoを維持し、大きいstack scratchや未登録のheap ownerを追加しません。
実際の固定ELFへ非LOADのsuffixを加えた回帰試験は、修正前に4096byte要求で失敗し、
修正後に全byteのbroker照合と20回未満のsource readで成功しました。

kernelはProviderReadTransferへ既存のwire検査をまとめ、user copyとowned snapshotで
共用します。read_allはbacking identityと明示featureを要求し、extent queryからEOF probeまで
同じI/O guardを保持します。sourceの位置・長さ、dirty cache overlay、短いEOFと成長の拒否を
維持し、途中の転送エラーでsnapshot全体を失敗にします。明示的なdeclineだけがinlineを
選び、その後はsharedを再試行しません。transferのHandleはguard解放後に回収します。
ユーザーへのcopyでは従来どおり、guardを解放してからuser memoryへ書きます。
新しいsyscallやPOSIX専用の意味論は追加しません。

digestの関連4対象55件、snapshotの関連4対象13件とkernel native checkが成功しました。
記録はout/checks/mochios-positioned-shared-319-{digest-host,snapshot-host,kernel-check}.logです。
この追加修正のnative buildとKVM実IPC検証も成功しました。fixtureはCargoの
compiler-artifactから選択し、計測文字列とruntime更新後のlink時刻を確認して配置しました。
通常DockのTerminal / mshから共有readの4グループを再確認し、524,599byteのreadは
24,000,000nsでした。異なる試験時の負荷・cache状態を揃えていないため、318との差から
改善倍率は主張しません。

同じ通常mshでnative_clang_driverの全gateが成功しました。clangのtarget取得は6,336ms、
通常引数のcompile / link / 自動署名 / msign verifyは62,980ms、試験全体は132,352msです。
./mainの正確なstdoutと終了値37、compile失敗・link失敗後の旧ELFのbyte一致、
改変したELFに対するmsign verifyの失敗、cleanupとprompt復帰を確認しました。
従来のalarm 900秒は変更していません。改変拒否はmsignの検証機能によるもので、
共通実行認可へ署名検証が接続済みという意味ではありません。画面は
out/checks/mochios-positioned319-{read-result,clang-complete}.png、結果と選択したfixtureは
out/checks/mochios-positioned-shared-319-artifacts.jsonです。

記録したVMを停止し、解析プロセスの復帰処理を終えています。SDK入力を変更せず、所有する
診断用395ファイルを撤去し、検証済みkernel / filesystem / mshを通常イメージへ保持しました。
imageから読み戻したTerminal / msh / filesystem / POSIX / inputの5実行体はstageと一致し、
診断rootとprobeの不在、配布成果物6件のchecksumも確認しました。記録は同接頭辞の
readback.jsonとnormal-image.logです。

SDKとRust overlayは再生成しません。完全なPOSIX環境、製品のコンパイラ配置、実行時の
署名検証・共通Capability同意のgoalも引き続き未完了です。
