# FileHandle に結び付いた POSIX pathname exec

## 実行経路

libc の execve は pathname・argv・envp を fault-safe に所有コピーした後、
posix.service の OPEN_READ | OPEN_EXECUTE で image を一度だけ開きます。
その FileHandle を callback-resource owner に保持し、hash、同一 security domain の
認可、native ProcessImageReplaceBounded のすべてに同じ参照を使います。
認可や loader へ pathname を渡して再オープンする処理はありません。
失敗、通常の復帰、C callback による処理の放棄では owner が参照を解放します。
fexecve は既存の FD snapshot を借用し、同じ image 実行処理を使用します。

POSIX の実行名を capability.service の package index へ照会する処理は廃止しました。
image の hash を照合し、現在の application identity と通常の Capability を維持する
既存の image-replace-inherit 認可を使います。認可の取得や消費に失敗した場合の
fallback はありません。新たな権限の付与や manifest 起動 policy の上書きは行いません。
manifest から別アプリの起動権限を決める launcher は従来の経路を使います。

## パスと権限

pathname は POSIX protocol と同じ最大長を使い、UTF-8 を要求しません。
認可には短い不透明 key を用い、AT_EXECFN の bytes と分離します。
相対パスは OPEN_AT の一回の操作で、セッションの cwd / root と実効 credentials を使って解決します。
別 RPC の getcwd で実行名を再構築する処理はありません。

fork の captured-image 認可も、AT_EXECFN を照合 key として使わなくなりました。
非 UTF-8 や長い実行名でも、native が実際の identity / measurement を検証します。

filesystem の OPEN_AT_EXECUTE は、regular file と実行 mode を検証します。
image 読み出し用の Handle の READ 権限と、POSIX のファイル read mode は別の判定です。
POSIX mode が実行専用でも image をロードできます。owner / group / supplementary groups /
other と uid zero の execute-bit 条件を保ち、native のファイル Capability を越える権限は付与しません。
普通の読み取り open には従来どおり read mode が必要です。

FD、CLOEXEC、signal、resource limit、startup の受け渡しは、fexecve と共通です。
追加の SEEK 権限を要求せず、hash と loader は positional read を使用します。

## 検証範囲

`services/posix/tests/fexecve_runtime.rs` で実装本体を呼び、開いた image の所有権、
READ / TRANSFER の不足、SEEK 不要、hash / 認可 / loader の各失敗での解放を確認します。
`exec_inputs_runtime.rs` は非 UTF-8 と POSIX pathname 上限、`support/exec_open.rs` は
実 ext4 inode の execute-only と credentials の組合せを検証します。
以前の切り出した READ gate の試験は、実装本体を通す rights matrix に統合しています。

`native_execve.c` は不正 image の失敗と Handle 数の維持、未登録の execute-only ELF、
256 bytes を越える非 UTF-8 pathname、相対パス、PATH 検索、symlink、実行後の fork / wait を扱います。
同じ image で既存の native_fexecve.c も実行します。

ホスト全体試験は 678 targets、3,296 件成功、失敗 0 件、8 件 ignored でした。
KVM では pathname の 6 groups と既存 fexecve の 3 groups が成功し、
各 pathname payload の実効 UID が非 0 であることと、その中での fork / wait も確認しました。
通常構成へ戻した後、input READY とデスクトップ・Dock の表示も確認しています。
検証用バイナリは除去し、VM は停止済みです。

記録は `out/checks/mochios-execve-object-203-host-final.log`、
`mochios-execve-object-203-native.log`、`mochios-execve-object-203-normal.log` と `.png`、
`mochios-execve-object-203-artifacts.json`、`mochios-execve-object-203-evidence.json` に保存しています。
配布 SDK / Rust std overlay とカーネルは今回変更していません。

## 残る項目

この変更で POSIX exec 全体や POSIX 互換環境全体を完了とはしません。
static PIE の配置と相対 relocation は [ユーザー空間の PIE 準備](posix-pie-preparation.md) で追加しています。
共有ライブラリを扱う動的 loader、set-id の credential 遷移、実行 image に対する書き込みとの排他は別途必要です。
