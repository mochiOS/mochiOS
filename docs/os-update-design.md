# 通常OS更新: 現状と安全な移行条件

2026-09-20時点で通常OS更新のdownload・適用は無効。`disk.img.zst`は初回インストール専用であり、更新payloadとして扱わない。

## 現状

- `version.toml`の`release`/`build`が現在の識別子。architectureはビルドターゲットの`x86_64`。
- GPTはESPと単一の書き込み可能なext2 rootfs。ESPの`BOOTX64.EFI`が`/system/kernel.elf`、`kernel.meta`、`initfs.img`を読み込む。
- `/system`、`/var`、`/home`は同じrootfsパーティション上。A/B slot、boot trial counter、rollback制御は存在しない。
- `update.service`は公開APIを読み取るが、現在は更新payloadをdownload・適用しない。Developer CAの公開鍵は通常OSリリース署名の信頼鍵ではない。
- 現在の`network.service`はHTTP応答本文を全量`Vec<u8>`に保持してから読み取りIPCへ渡す。大きなpayload向けの真のストリーミング受信ではない。redirectは全て拒否する。
- `http-client`には200・Content-Length必須の逐次応答パーサーを追加した。chunked、長さ不明、Content-Encoding付き、redirect応答は拒否し、本文を保持せず呼び出し側へ渡せる。ただし現行`network.service`とIPCにはまだ接続していない。
- `update::payload`には、任意の`Read`から小さな固定バッファで正確なサイズとSHA-256を確認し、一時ファイルを公開する部品を用意した。現在のHTTP経路やboot slotには接続していない。
- `release-2026-01`のEd25519公開鍵（SPKI）はOSビルドに固定埋め込み済み。秘密鍵は含めない。Developer CAの鍵とは別である。
- `available`は署名検証済みの`VerifiedManifest`に変換する。payloadの一時保存はこの型に含まれるサイズ・SHA-256を使う。まだ自動downloadはしない。
- `boot-selection`にCRC・世代番号付き32バイト記録とA/B試行・確定・rollbackの状態遷移を実装した。bootloaderは専用領域の二重記録からslotを選ぶ。
- `boot-selection::storage`に二つの記録を読み、古い方だけを書き、`sync`後に読み戻す手順を追加した。空の記録の明示的な初期化、trial回数の永続化、確定・rollbackを`RecordIo`経由で扱う。stage・trialに加え、B確定時の全32バイト位置での部分書込み、同期失敗、読戻し失敗のメモリ故障注入テストを実施する。実ディスクの電源断試験は未実施である。
- `mmake ab-layout-image`はESP、system A、system B、data、専用boot-stateの5 GPT領域を持つ**検証専用**イメージを`out/mmake/image/ab-layout.img`へ作る。試験用ESPには`/slots/A`と`/slots/B`のkernel、kernel.meta、initfsを別々に収める。A/Bのrootfsは現行イメージの複製、dataは空のext2であり、ユーザーデータ分離や移行は未実装。通常の`disk.img`、Artifact、リリースには使わない。
- bootloaderは同じ物理ディスク上のboot-state領域をUEFI Block I/Oで読み、CRC・世代番号を確認する。pending trialでは起動前に残り試行回数を減らし、`flush_blocks`と読戻しが成功した場合だけBを選ぶ。書込み失敗時は安定slotへ戻し、3回の未確定trial後の次回起動で自動rollbackする。対応slotのboot assetsを試験用ESPから読み込み、BootInfoのslot値をkernelへ渡す。BootInfo ABI 2では、UEFIが提供した起動ESPの一意GPT GUIDもfeature flag付きで渡す。取得できない場合はゼロにしてflagを立てず、将来のboot-state確定は許可しない。ext2 CEXTはABI 4のcallbackでslot値を受け、GPTの種別と名前が一致するSystem A/Bだけを選ぶ。従来イメージでは旧探索を維持し、不正なboot-stateは拒否する。OS側からの初回起動成功の確定処理はまだない。
- 読取り専用`BootSystemSlot` syscallにより、ユーザー空間はBootInfo由来のlegacy/A/Bを取得できる。`update.service`は起動slotをReleaseでも有効な状態ログへ記録するが、boot-stateの書込みや自動確定は行わない。`mmake ab-boot-slot-smoke-test-kvm`でB起動とサービスログ上のBを確認済み。現在のdisk CEXTには安全な対象ディスク同定と、flush非対応時の厳密な永続性保証がないため、単に更新サービスへraw disk書込み権限を与えて確定する方式は採らない。
- `mmake ab-layout-test`で5領域の境界、A/B複製、ESP内のslot別boot assets、dataのext2、boot-stateの初期値を検証する。`mmake ab-slot-selection-test`はBootInfoとGPT slot照合のunit testを実行する。`mmake ab-layout-smoke-test-kvm`は安定A、`mmake ab-slot-b-smoke-test-kvm`は安定BをKVMで確認する。後者はABI 2で起動ESP GUIDを取得できることも確認済み。`boot-selection::gpt_identity`にはOS可視ディスクのGPTヘッダーとエントリー表のCRCを検証し、UEFIから渡されたESPの一意GUIDを照合する読取り専用ロジックと故障系unit testを追加した。`mmake ab-layout-test`はホスト専用の読取り専用probeを使い、実際のA/BイメージでもGUID・範囲一致と異なるGUIDの拒否を検証する。これはまだ実ディスク列挙・OSサービス・書込み処理には接続しておらず、単独では書込み許可にならない。disk CEXTはflush非対応を成功と報告せず`ENOSYS`を返すようにしたが、複数ディスクの同定、容量取得、実ディスクの永続化・電源断試験は未実施。`mmake ab-trial-b-test`は別のpending B試験イメージを検証する。`mmake ab-trial-rollback-smoke-test-kvm`は同じdisk状態を引き継いでBを3回試験起動し、4回目にAへ戻ることを確認済み。`mmake ab-trial-confirm-smoke-test-kvm`はBの試験起動成功後に**ホスト側の試験ツールで**状態を確定し、次回も安定Bで起動することを確認済み。これはOS側の自動確定ではない。KVMテストには`/dev/kvm`が必要である。
- `mmake ext2-write-test`はパーティション境界ガード追加後にKVMで再確認し、prepare、再起動後の永続性、容量不足、各段階の`e2fsck`まで完了した。テスト専用fixtureは`mmake ext2-write-fixture-test`で起動前に検査する。ext2 CEXTはGPTで選択したsystem領域の開始・終端を保持し、filesystem容量と各I/Oのパーティション境界を検査する。境界外I/Oのunit testは`mmake ext2-cext-test`で実行する。電源断中のext2書込み整合性は未実施であり、ext2はjournalを持たないため、今回の成功を電源断耐性の証明とは扱わない。

## 必要な新ディスク形式

新規インストール用イメージを、ESP、system A、system B、永続data、専用boot-stateの5領域に変更する。boot-stateは二重化記録を生のブロックとして保持し、ext2やFATのmetadata更新に巻き込まない。両system slotには同じ形式のboot payload（kernel、initfs）と読取り専用system filesystemを置き、ユーザー設定、診断キュー、アカウント、ホームはdataにだけ置く。起動中のslotとdataは更新適用中に書き換えない。

bootloaderはCRC付き・世代番号付きの二重化boot選択記録を読み、`active_slot`、`pending_slot`、`attempts_remaining`を扱う。新slotへの切替えはpayloadの検証・書込み・再読取りが終わった後にのみ永続化する。電源断で選択記録が片方壊れても、前世代の有効記録と旧slotから起動できる必要がある。初回起動をユーザー空間まで確認した場合だけ新slotを確定し、失敗または試行回数超過なら旧slotへ戻す。

現行の単一ext2イメージは、起動中に分割・上書きして移行しない。バックアップを明示したオフラインの再インストール／移行ツールを別途用意し、dataコピーと整合性検証後にA/B対応版を起動する。既存端末を更新可能と表示するのは移行完了後に限る。

## 通常更新payload契約（未実装）

初回インストール用raw disk imageではなく、非アクティブsystem slotにだけ書ける独立したpayload形式を定義する。形式にはslot imageの長さ・SHA-256、対象architecture、必要bootloader世代、data schema互換性を含める。形式とCloud側配布登録が決まるまで、APIの`available`は情報としてのみ扱う。

適用時は、固定されたリリース公開鍵でAPI manifestを検証してから、HTTPSの許可済みhostへ接続し、redirect先を再検査する。stream保存中にsize上限とSHA-256を検査し、一時ファイルをsyncする。検証済みpayloadだけを非アクティブslotへ書き、書込み後に再読取り検証する。Kill Switch、配信停止、同意取消しまたは検証失敗ではboot選択記録を変更しない。更新結果Reportは既存の基本診断同意と永続再送キューを使い、結果が確定した時だけ送る。

## 鍵ローテーション時の条件

署名用秘密鍵はOSやリポジトリへ置かない。鍵ローテーションは新旧鍵を含むOSを先に配布し、その後に署名鍵を切り替える。通常ビルドは固定公開鍵を使用する。ローテーション時のみ`MOCHIOS_RELEASE_PUBLIC_KEYS='key-id:base64-SPKI[,next-id:base64-SPKI]'`で公開鍵リスト全体を指定する。Developer CA鍵を流用しない。

## 有効化のブロッカー

1. 検証用5領域を本番用system/data分離へ変えること、dataの永続mount、OS初回起動成功からのslot確定、更新サービスからの非アクティブslot書込み、電源断回復のQEMU故障注入テスト。bootloaderのpending trialと自動Rollbackは試験用イメージに接続したが、メモリ上の部分書き込みテストと通常のKVM起動だけでは電源断時のストレージ永続性を証明できない。
2. 通常更新payloadの独立形式、Cloud側の通常更新Artifact、mock APIでのdownload・checksum・失敗試験。
   現行HTTPサービスの全量RAM保持を解消し、通信中のchunkを直接一時ファイルへ渡す受信経路も必要。
3. 旧単一rootfs端末を安全に移行するオフライン手順。

上記が揃うまで、署名済みの`available`が返ってもdownload・適用を行わない。
