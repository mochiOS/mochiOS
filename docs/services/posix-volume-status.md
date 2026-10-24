# POSIX volume status

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

## 残る接続と監査事項

- POSIX path query と libc の statvfs / fstatvfs、および C header の接続。
- FIFO / terminal が保持している filesystem metadata からの volume observation。
- devfs や他の filesystem の owner が返すべき容量・属性の定義。
- ext4 の reserved-block field と現在の allocator policy を混同せず、f_bavail を定義する。
- 匿名 pipe と匿名 PTY の fstat は現在 mode 等の固定値で、uid / gid / inode / times を
  正しく共有する metadata が未実装。native Stream の高速 data path を保ち、POSIX metadata
  を kernel へ持ち込まない構成で解決する必要がある。
