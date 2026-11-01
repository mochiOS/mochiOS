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

実IPC、dirty mappingとの実機整合、clangの読込み時間と後半の失敗経路は未確認です。
SDKとRust overlayは再生成しません。完全なPOSIX環境、製品のコンパイラ配置、実行時の
署名検証・共通Capability同意のgoalも引き続き未完了です。
