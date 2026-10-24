# POSIX volume status

現行の実装は 196 の statvfs / fstatvfs 接続まで進んでいます。195 は基盤追加時の記録です。

## 195: filesystem owner の容量照会

`statvfs` / `fstatvfs` のため、filesystem.service に読み取り専用の volume status を
追加した段階です。POSIX の C ABI / client / dispatcher への接続はまだ実装していません。

取得するのは allocation block size、総 block 数、現在の空き block 数、総 inode 数、
現在の空き inode 数、owner 内の mounted-volume identity、名前長上限と read-only 属性です。
値は実際に選択された ext4plus instance の superblock と writer 有無から取得します。
空きカウンタは起動時に読み取ったディスク値をコピーして固定せず、allocator が更新する
値を参照します。ext4plus には総 inode 数の読み取り accessor だけを追加しました。

native Provider では `volume_status::GET` を既存 HandleQuery / HandleControl で呼びます。
対象は native dispatcher が認証した、保持済みの open-description Handle です。
数値の volume ID や pathname を Handle の代わりに信用する API は作っていません。
write-only の description でも既存 CONTROL 権限で照会でき、cursor や status flags は
変更しません。device owner は ext4 のカウンタを流用せず、未対応として拒否します。

パス側の `OP_VOLUME_STATUS_AT` は認証済み credentials と添付 root / base Handle を
使って一度だけ解決します。target data を開くことや data-read permission は要求せず、
path search permission は確認します。取得した mount をそのまま照会するため、別の
root volume の容量を返したり、取得後にパスを再 open したりしません。

wire は明示的な little endian の 56 bytes です。block / inode counters は 64 bit とし、
短い応答、余分な bytes、予約領域、未知の flags、ゼロの単位、総数を超える空き数を拒否します。
mnu の ABI や kernel に filesystem / POSIX policy は追加していません。

`services/posix/tests/` の関連 87 件が成功しました。実 ext4 image の生成、dumpe2fs の
inode 総数との照合、異なる block size の mount、write-only の CONTROL observation、
read-only backend、target mode 000 の照会、親 search 拒否、作成 / 書き込み時の減少、
unlink 後の FD 保持と最後の close による解放を含みます。

- ホスト結果: `out/checks/mochios-volume-status-195-host-final.log`
- filesystem.service の native build: `out/checks/mochios-volume-status-195-native-build.log`

この段階の native build は成功しましたが、新しい要求の実 IPC / KVM 証明は未実施です。
通常の rootfs staging と起動用イメージは変更していません。SDK / Rust std も再生成していません。
POSIX 側と C 関数の接続後に実 syscall / IPC を含む統合試験を行う必要があります。

## 195 時点で残っていた接続と監査事項

- POSIX path query と libc の statvfs / fstatvfs、および C header の接続。
- FIFO / terminal が保持している filesystem metadata からの volume observation。
- devfs や他の filesystem の owner が返すべき容量・属性の定義。
- ext4 の reserved-block field と現在の allocator policy を混同せず、f_bavail を定義する。
- 匿名 pipe と匿名 PTY の fstat は現在 mode 等の固定値で、uid / gid / inode / times を
  正しく共有する metadata が未実装。native Stream の高速 data path を保ち、POSIX metadata
  を kernel へ持ち込まない構成で解決する必要がある。

## 196: statvfs / fstatvfs の接続

`posix-client` の path / Handle observation と、POSIX dispatcher、runtime の C 関数を
接続しました。`user/libc-port/include/sys/statvfs.h` が C layout を定義し、既存の SDK
header install がこの header も取り込みます。既存 SDK 自体は変更・再生成していません。
newlib の fsblkcnt_t は 64 bit、fsfilcnt_t は 32 bit なので、inode counter の overflow は
EOVERFLOW として扱います。出力は padding も含めゼロ初期化した byte image へ field ごとに
詰め、fault-safe usercopy で渡します。成功時の errno は変更しません。

パス照会は stat と同じ session credentials / root / cwd を保持して filesystem RPC を
一回だけ行います。対象 data を開かず、target の read permission は要求しません。
FD は通常ファイル / directory なら retained native HandleQuery、write-only なら
HandleControl を直接呼び、POSIX service への中継は行いません。FIFO / terminal-file は
TRANSFER だけに縮小した添付 Handle を使い、既存 provider metadata 解決を POSIX lock の
外側で行います。元の所有者が EBADF を返した場合だけ別 provider へ問い合わせます。
返信時には元の execution image を再検証し、別 image の要求へ結果を返しません。

FS control の `OP_VOLUME_STATUS_HANDLE` でも数値 open ID を authority とせず、IPC に
添付された Handle を kernel の Provider identity で識別します。cursor / status flags や
FD table を変更せず、rename / unlink 後の FD も元の volume を照会します。

現在の ext4 allocator は reserved-block field に基づく privileged reservation を実施
していないため、f_bavail は実際に利用可能な f_bfree と同じです。適用されない予約数を
形式上だけ差し引くことはしていません。f_favail も現在の allocator に合わせ f_ffree と
同じです。ST_RDONLY は実際の writer 有無から取得します。devfs、匿名 pipe / PTY、
socket、POSIX shared-memory backing 等の volume accounting は引き続き未対応です。
valid FD を EBADF と偽らず、runtime で未対応種類を拒否します。

初回 KVM 試験で、195 の path observation opcode が既存 FIFO 作成の 52 と重複していた
誤りを検出しました。path / Handle volume observation は未使用の 53 / 54 に修正し、
filesystem と POSIX の全公開 opcode 定数に numeric alias がないことを確認する試験を
追加しました。互換用の alias は残していません。初回失敗ログは
`out/checks/mochios-statvfs-196-native-first.log` に保持しています。

runtime と両サービスを native build し、C の `native_statvfs.c` を既存 SDK の
crt0 / libc / libm / libgcc と新しい runtime archive でリンクしました。SDK に未収録の
新 header は source tree を `-idirafter` で追加し、SDK を書き換えず C ABI を確認しています。
修正後の KVM 試験では次の 5 群が成功しました。

- Data の実容量と System の ST_RDONLY、異なる filesystem identity、C layout / errno。
- write-only FD、dup、rename / unlink 後の retained FD、cursor 不変。
- unlink 後の FIFO FD から filesystem metadata Handle を通す容量照会。
- target mode 000 での照会、親 search permission 拒否、fork / chroot の root confinement。
- 無効 FD / path / 出力 pointer、未存在 path、未対応 stream、失敗後の正常照会と cleanup。

`out/checks/mochios-statvfs-196-native-fixed.log` が成功ログです。診断では compositor を
C 試験へ置き換えているため、終了後の CompositorNotFound は通常 desktop 起動の
結果とは扱いません。SDK / Rust std の再生成、全アプリの再リンクは行っていません。

全ホスト試験 3,275 件が成功し、失敗 0 件でした。
`out/checks/mochios-statvfs-196-host-all-final.log` が最終結果です。ABI padding / overflow、
READ と CONTROL の選択、malformed response と余分な返却 Handle の解放、session credentials
の固定、添付 authority、opcode の一意性も含みます。

診断用 compositor / manager と package verification record を元へ戻し、更新した通常版
filesystem.service / posix.service のイメージを生成しました。別の KVM 起動で Binder ready と
Dock・上部バーの表示を実際に確認し、VM は停止しています。
`out/checks/mochios-statvfs-196-normal.log` / `-normal.png`、`-artifacts.json` / `-evidence.json`
に結果と範囲を保存しています。C 関数の実 OS 証明は今回再リンクした試験 ELF に限ります。

196 で未解決なのは、devfs 等の volume accounting、read-only 以外の mount 属性、匿名
pipe / PTY の共有 metadata といった残りの監査事項です。Vim / WebKit の動作確認や
POSIX 全体の完了を意味しません。
