# 実行パスに依存しない captured image の生成認可

## 実装

newlib runtime の fork は、実行名を package index へ渡す処理を廃止し、
`AUTHORIZE_CAPTURED_IMAGE_OPCODE` を capability.service へ送ります。
要求には protocol version と不透明な一回限りの照合 key だけを載せます。
パス、PID、identity の指定、Capability 一覧、自己申告した digest は受け取りません。
broker は Provider I/O や filesystem の再オープンを行いません。

broker は認証済み IPC sender を対象に native AuthorizeExec の `spawn-inherit` mode を使います。
カーネルは現在の application identity、image digest、通常の CapabilitySet を一つの
process-table borrow 内で取得します。Native kernel authority は付与せず、実行 class は
unprivileged に限定します。カーネルへ POSIX、サービス名、ディレクトリ構造を持ち込みません。

この認可は ProcessCreate の captured address-space context にのみ使用できます。
通常の ELF image を新規ロードする経路には使えません。captured image の ImageOrigin が、
identity と measurement の一致、および capture 時の権限上限を検証します。
さらに認可を消費するときと、image の一回限りの所有権を引き渡すときに、作成元の現在の
identity・digest・権限を再確認します。後者は既存の PROC → image lock 順序を守ります。
認可後や capture 後に取り消された権限は復活できません。
一致した認可は失敗時も消費し、後の再付与による再利用を防ぎます。

信頼済み launcher が別のアプリを起動する既存の `spawn` 認可とは区別しています。
その起動 policy を、現在の security domain の継承に置き換える変更ではありません。
set-id や privileged image の複製に対応したという意味でもありません。

## 検証

`services/posix/tests/` に、wire の境界・version、native authorization parser、
identity / digest の変更、権限の subset、取り消された認可の消費、image consume 中の
lock と権限再確認、libc 境界での native error → errno 伝播の試験を置いています。

`native_fexecve.c` の未登録・unlink 済み payload 内で fork → waitpid を行う試験を追加しました。
従来の FD / CLOEXEC / offset / signal / record lock の確認も保持しています。
KVM でこれらの成功と、パス経由 execve の回帰試験の成功を確認しました。

ホスト全体試験は 679 targets、3,295 件成功、失敗 0 件、8 件 ignored です。
最終状態の認可 parser・errno・失効・wire・ProcessCreate admission の追加確認も成功しています。
記録は `out/checks/mochios-captured-image-202-host-final.log`、
`mochios-captured-image-202-final-boundaries.log`、`mochios-captured-image-202-native.log` です。
SDK / Rust std overlay は再生成していません。

検証用 compositor と manager を除去して通常構成へ戻し、KVM で input READY、
デスクトップと Dock の表示を確認しました。ログと画像は
`out/checks/mochios-captured-image-202-normal.log` と同名の `.png` です。
VM は停止済みです。この一回の起動成功で、前回の入力サービスの起動時間超過の
再現条件を特定したとは扱いません。

## 残る範囲

パス経由 execve の未登録 image 対応は [pathname exec](posix-path-execution.md) で実装しています。
set-id と動的 ELF loader は別途必要です。
また、通常起動で input.service の初期化が READY 待ちを越える問題は、この認可の変更で
解決したとは扱いません。POSIX 互換環境全体の目標は継続中です。
