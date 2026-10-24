# FileHandle からの POSIX exec

## libc の fexecve（201）

libc の `fexecve(fd, argv, envp)` を実装しました。SDK の newlib ヘッダには既に宣言があるため、
ヘッダの追加や SDK の再生成は不要です。実装は `user/crates/newlib-runtime/src/exec_handle.rs` にあります。

FD は process-local table のスナップショットから選び、既存の Lease でネイティブ参照を保持します。
同じ FD 番号の close / reuse で別の object に差し替わらない構造です。regular file として開かれ、
読み取りが可能で、READ | TRANSFER を持つ参照を要求します。POSIX service は添付された
FileHandle の現在の mode を、登録済み session の effective credentials で確認します。
uid 0 でも不足する Capability / Handle rights を補いません。

argv / envp は既存の fault-safe copy で所有バッファへ取り込み、policy IPC を呼ぶ前に固定します。
パスの復元、`/dev/fd`、ファイルの再 open は使いません。起動対象の identity / AT_EXECFN は
不透明な `fexecve` です。CLOEXEC の image FD も読み込み中は保持し、成功時の新しい FD table
からは除外します。digest は read_at で計算し、共有 cursor を変更しません。
認可と native image snapshot には同じ保持済み FileHandle を渡します。

起動データ、resource policy / heap / upcall stack の mapping、FD LaunchContext は通常の execve と
共有しています。policy-page の一時参照も callback resource scope へ登録します。
FD pins、record-lock の exec 準備と失敗時の cancel も共通です。新しい image の初期化でのみ
signal の image transition を反映し、失敗時には元の image の descriptor / handler を保持します。
カーネルの syscall や POSIX 知識は追加していません。

## 検証

ホスト全体試験は 678 ターゲット、3,292 件成功、失敗 0 件、ignored 8 件でした。
`fexecve_runtime.rs` は実際の runtime module を controlled native boundary で動かし、
FD と入力ポインタの検証、権限縮小、部分 read、digest、返信 Handle の cleanup、
transport / policy / format エラー、呼び出し後の FD と一時参照を確認します。
`image_context.rs` の既存検証は、FD metadata を共有 startup で生成する構造へ追従しています。

`native_fexecve.c` を C の呼び出し元と未登録 payload の二つの ELF にビルドしました。
KVM で、不正 FD / 非 regular FD / mode / EFAULT / ENOEXEC の拒否、失敗時の Handler・FD・offset・
ネイティブ Handle 数の維持、unlink 済み ELF の実行、非 UTF-8 を含む argv / envp、CLOEXEC、
dup / fork 後の共有 offset、signal の mask / pending / disposition、CLOEXEC に伴う record lock の
解放を確認しました。登録済みバイナリのパス経由 execve で同じ startup を使うことも確認しています。

C では newlib の補助 object を合成した archive を使用しました。既存の Rust std リンク経路は
補助 object を別に指定するため、そちらでは runtime の単体 archive を使用し、リンク成功を確認しました。
配布 SDK と Rust std overlay は再生成していません。別ディレクトリの runtime だけを再ビルドしています。

記録は `out/checks/mochios-fexecve-201-host-verified.log`、
`out/checks/mochios-fexecve-201-final-native.log`、
`out/checks/mochios-fexecve-201-std-final-link.log` です。

通常イメージの起動確認は完了していません。二回とも input.service の初期化が
READY 待ち上限を越え、その後に READY が到着しました。service-manager の配置バイナリは
変更前のものと SHA-256 が同一で、検証用 compositor / manager / payload は除去済みです。
ログは `out/checks/mochios-fexecve-201-normal.log` と
`out/checks/mochios-fexecve-201-normal-retry.log`、VM は停止済みです。
デスクトップ起動の成功としては扱いません。

## 続ける必要がある項目

既存のパス経由 execve にも package index に登録された実行名への依存が残っています。
現在の fork は、実行名を使って package index から生成認可を取得しています。
fexecve 後の不透明な実行名や未登録 image にはこの経路が適合しないため、
取得済み address-space image の origin と現在の security domain に基づく汎用生成認可へ
変更する必要があります。パスを復元する回避策では解決しません。
また、set-id 実行や動的 ELF loader の完了を、この変更の実績としては扱いません。
POSIX 互換環境全体の目標は継続中です。
