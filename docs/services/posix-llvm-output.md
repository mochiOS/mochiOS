# LLVMの出力ファイル処理の検証

LLVM 18.1.8の`llvm/lib/Support/FileOutputBuffer.cpp`を調べ、LLDが利用する
出力経路を実際のtarget LLVMSupport archiveで試験した。
OS機能の新規実装や、LLVMに成功を返す代替処理の追加は行っていない。

`services/posix/tests/native_llvm_output.cpp`は次を確認する。

- 実行権限付きFileOutputBufferを作成し、8192bytesを書き込み、commitする。
- 最終パスを再度開き、書き込んだ内容とownerの実行権限を確認する。
- 置換前から開いていたFDは、元の3bytesを引き続き読み出せる。
- 同じファイルの4096bytes目からreadwriteのmapped_file_regionを作成し、書き込む。
  mapping失敗は試験失敗とし、anonymous bufferへの代替をmmap成功として数えない。
- 明示的なmsyncを使わずunmapした後、再度開いたファイルから更新内容を読める。
  ファイルoffsetはmappingで変更されない。
- commitしなかったFileOutputBufferを破棄しても、既存ファイルの内容を変更しない。

これは単なるCのmmap試験だけではなく、実LLVMSupportの一時ファイル、共有mapping、
unmap、rename、permissions、discardを通す。kernelやPOSIX runtimeの代用品は使用しない。

## 結果と範囲

KVMでroot/euid 501の双方が成功した。既存の壁時計、システム情報、TLS、
LLVMSupportのbuffer/offset mmap/子プロセス、PHDRの試験も成功した。
記録は`out/checks/mochios-llvm-output-232-native.log`。
`native_llvm_runtime.cpp`で`MOCHIOS_LLVM_OUTPUT_PROBE_232`を有効にして呼び出す。

壁時計試験用のsystem.time.readは診断用のstaged manifestで明示し、
msignのbuilt-in-record生成と通常のイメージ認証を通した。
試験後には元のmanifestとverificationの完全一致を確認し、診断ELFを取り除いた
通常イメージを再生成した。追加の本番権限や製品コード変更はない。
今回は試験コードのみの変更なので、通常起動の再試験は行っていない。
通常起動の直近の検証記録は段階231のログと画面である。

検証中は、PIDと開始時刻を照合した自分のLLVM buildを一時停止し、
VM終了後に同じjobを再開した。clang/ld.lld本体のbuild完了や、
実compilerによるC生成物の実行成功を示す試験ではない。
