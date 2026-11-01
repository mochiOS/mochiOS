# Native RAMオブジェクトの容量（325）

静的ELFも共通の捕捉処理を通すと、約117MiBのclangがRAMオブジェクト作成時に
ENOMEMとなりました。従来のResourceLimits.max_mapped_pagesは4096でしたが、
実際の利用箇所はMemoryObjectCreateのオブジェクト長の検査だけです。
総mapping数やresident memoryを計測する値ではなく、1個16MiBの固定上限でした。

これをmax_memory_object_pagesへ改名し、汎用設定memory.max_object_pagesから取得します。
既定値は65536 pages（256MiB）です。起動時に明示された設定bytesから読み、
0・負数・不正な値・byte換算のoverflowは採用しません。
カーネルへclang、ELFの署名、POSIX、サービス名やSDKの配置を知らせる変更ではありません。

固定長と拡張可能なRAMオブジェクトは同じ上限を使います。オブジェクトは作成時の上限を
保持し、resize時にも検査します。論理長の予約で全ページを確保せず、既存の疎なstorageを
利用します。POSIX層のrlimit、VMへのmapping admission、resident accountingとは別の制限です。

## 検証

実際の設定parser、SharedMemory、SharedPagesと長さ計画を使うホスト32件が成功しました。
設定のdecimal / hex / 不正値、256MiBの論理長、旧上限より後ろのページ境界をまたぐI/O、
上限超過の作成・resize拒否と拒否後の長さ維持、別の小さいオブジェクトの上限維持を確認します。

native_memory_object_capacityは通常Dockから起動したTerminal / mshで実行し、
256MiBのRAMオブジェクト作成、32MiB付近のページ境界I/O、上限超過の拒否とprompt復帰が
成功しました。証跡はout/checks/mochios-memory-object-capacity-325-のhost.log、
kernel-build.log、native-build.log、artifacts.jsonとresult.pngです。

この容量試験だけでclang経路の回帰解消や、完全POSIX環境の完成とは扱いません。
