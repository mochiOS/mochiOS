# 常駐RAM参照の計測基盤（段階223）

ru_maxrssの前提として、kernelのVMルートごとのRAM参照数と最大値を記録する
処理を追加しました。POSIXのrusage、親子関係、サービス名、単位変換は扱いません。
現段階ではsyscallへ公開しておらず、ru_maxrssへの接続も未完了です。

4 KiBのRAM-backed user leafをsoftware bit 11で識別します。COWのbit 9 / 10と
重複しません。PRESENTとは独立し、アクセスを止めてもRAM参照を保持するページは
引き続き数えます。仮想予約の長さやアドレスの空隙は加算しません。
各aliasは一つの参照として数え、COWで物理frameを交換しても同じleafの参照数は
増やしません。共有frameは各ルートの参照として数えます。

ELF / anonymous RAM、file / RAM object、DMA RAM、fork先のcopy / share、
unmap、frame保持のdetach、suspend後のreleaseを計測へ接続しました。
借用領域のWriteBackは現在の呼び出し元が明示的に選択する既知の共有RAM、
Uncachedはdevice storageであり、後者をRAM常駐量へ加算しません。
この分類を将来の呼び出し元も守る必要があり、一般にcache policyだけから
物理storageの種類を推測してよいという意味ではありません。

ルート作成時にfallibleな登録を行います。各leafの更新ではアドレス空間の走査や
allocationをせず、登録済みルートをbinary searchしてカウンターだけ更新します。
登録は上限を設け、未整列・重複・枯渇ではルート作成を失敗させます。
不整合な減算やoverflowで数値をwrap / saturateせず、その計測を取得不能にします。

物理ルートの寿命とは独立した参照で統計を保持できます。deferred root pinがある場合も
実際のleaf破棄まで登録を残し、root frameをallocatorへ戻す前に登録を外します。
同じphysical root addressを再利用しても、古い統計の最大値と混ざりません。
processを跨いだ共有やexecに対するprocess lifetimeの最大値への接続は次段階です。

hostでは5 target / 20件が成功しています。独立したleaf membership modelとの
10,000回の比較、保護変更・COW bits・借用storage、不整合の検出、ルート再利用、
8 threadでの同時更新、実装本体のPTE suspend / restore / releaseを含みます。
既存extent transaction、root lifetime、COWの試験も通りました。
記録はout/checks/mochios-residency-223-host-final.logです。

KVMでは新しいカーネルで段階222のC/C++試験を再実行し、通常資格情報と
seteuid(501)後のexecの両方でwait / fork、anonymous / file-backed mapping、
PROT_NONEとデータ保持、pthread、exception、filesystem、補助vectorが成功しました。
これは既存動作の維持の確認です。実機から計測値を読み出して正しさを確認するには、
次段階のnative取得APIが必要です。記録はmochios-residency-223-native.logです。

診断ELFを除去し通常managerへ戻してイメージを再生成後、新カーネルでの
input READY、Binder ready、デスクトップとDockを確認しました。normal.log / normal.pngへ
保存し、今回のVMは停止しました。配布SDKとRust std overlayは再生成していません。
NICのないrunnerのnetwork Failed(-6)、USB driver errno 22、画面中央に起動ログの
一部が残る問題は別件です。完全なPOSIX環境、ru_maxrss、clangの実行は未完了です。
