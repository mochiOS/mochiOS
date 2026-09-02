# mnuを小さく、速くするために

目標は、配布するmnuカーネルをおよそ1 MiBに収め、操作に対する待ち時間を実機で継続して下げることです。単にELFファイルだけを小さくして、起動後に同じ量のメモリを確保する変更は成功と数えません。ファイル上の大きさ、起動後の常駐量、処理時間を別々に測ります。

「macOSレベル」は、そのままでは判定できません。IPC、スケジューラ、メモリ、VFS、プロセス生成、起動、アイドル時の負荷に分け、同じ機材と条件で比較できる数値にします。比較結果が揃うまでは、同等になったとは扱いません。

ViewKit、Binder、compositorなどのUI実装は、今回のmnu最適化には含めません。カーネル側の基準が固まった後、アプリとサービスを別の計測対象にします。

mnuのファイルサイズが1 MiBを下回ったため、mBoot、mDriver、アプリの調査も始めました。現在の内訳と変更順は[バイナリを小さくする順番](binary-size.md)に分けています。

## 現在わかっていること

2026年9月1日の最初のreleaseビルドは18,710,928 bytesでした。このうち17,738,760 bytesを固定メールボックスが占めていました。`.text`は539,574 bytesだったため、最初に直すべきなのは命令数ではなく、未使用時にも実体を持つ固定領域でした。詳しい内訳は[mnuカーネルのサイズ](mnu-kernel-size.md)に保存しています。

メッセージ本文を必要になったときだけ確保するように変えた後は、stripしていないrelease ELFが1,012,312 bytesになりました。その後、IPC領域の再利用とstackの整理まで進めた時点では1,008,296 bytesです。最初の値から17,702,632 bytes、率にして94.6%減りました。固定メールボックス領域は44,040 bytesまで減っています。

load対象のメモリ量は48,656,814 bytesから1,261,983 bytesへ減りました。通常threadの固定stack poolをなくし、実際に生きているthreadの物理pageだけを確保しています。[現在の計測結果](baselines/mnu-kernel-dynamic-stacks-2026-09-01.json)には、revisionとsectionごとの値も含めています。

起動に必要な`secondary_cpu_entry`は`kernel.meta`へ分離しました。配布用`kernel.elf`からsymbolを外した現在のファイルサイズは866,728 bytesです。開発用の`kernel.debug`は別に残しているため、crash時のアドレスを関数名へ戻せます。[配布用kernelの計測結果](baselines/mnu-kernel-stripped-2026-09-01.json)にstrip後のsectionを記録しています。

この段階でファイルサイズは1 MiBを下回りましたが、IPCが十分に速くなったという意味ではありません。送信側の一時bufferはなくなり、cacheが温まった後はメッセージ保存用のheap allocationも発生しません。受信側も4,128 bytesの一時`Vec`を廃止し、通常の本文とreplyはキューからユーザー領域へ直接コピーします。外部pageの通知だけは、mapping結果を書き戻すため16 bytesのstack領域を使います。[受信経路変更後の計測結果](baselines/mnu-kernel-ipc-receive-2026-09-01.json)では、ELF全体が1,007,024 bytesです。実機でp50、p95、p99を取るまでは、性能目標を達成したとは扱いません。

4,460,544 bytesあった`KSTACK_POOL`は削除済みです。配布用kernelは863,296 bytes、LOAD segmentのメモリ量は1,261,983 bytesになりました。ここから先は小さな固定領域と実行時allocationを分けて見ます。

kernel heapも32MiBを最初から物理mapする処理をやめました。256KiBから始め、必要になった分だけ追加します。service起動地点では1,323,008 bytesだったため、従来より32,231,424 bytes少なく済みました。[heap変更後の計測結果](baselines/mnu-kernel-growable-heap-2026-09-01.json)で1 vCPUと4 vCPUの確認結果を読めます。

## 先にそろえる計測

変更の前後で、同じ処理をウォームアップ後に複数回実行します。待ち時間は平均だけでなくp50、p95、p99と最大値、試行回数を残します。ビルドのrevision、feature、コンパイラ、CPU、CPU数、メモリ量、実機か仮想マシンか、キャッシュ条件も結果へ含めます。

対象は次のとおりです。

- 小さいIPCの片道と往復、4 KiB IPC、ロック待ち、起床までの時間
- context switch、run queue操作、wake-up latency、timer interrupt数
- allocationとfree、確保量、失敗数、使用中とpeakのheap・frame数
- path lookup、open、read、write、close、stat
- execの解析、読み込み、relocation、entry到達まで
- mBootからmnu、各CPU、filesystem、system service、最初のidleまでの起動時刻
- アイドル時の割り込み回数とCPU wake-up回数

QEMUは正しさと回帰の確認に使います。性能値は仮想化方式やホスト負荷に左右されるため、採用判断は実機の同一条件で行います。

## 変更する順番

### 固定メールボックスをなくす

各メールボックスが最大メッセージ本文を抱える構造は解消しました。現在は実際にキューへ積まれた本文だけを確保し、枯渇時には送信失敗を返します。解放前のzeroingと、壊れたキューを隔離する処理も残しています。

本文には64件まで保持するシステム共通cacheを設けました。cacheを越えた空き領域はheapへ返すため、使った最大量をそのまま保持し続けません。送信時はユーザー領域からキュー所有の本文へ直接コピーします。IPCのCapability検査は維持し、共有ページを使う大きなIPCとは分けています。

受信時に最大本文と同じ大きさの`Vec`を確保する処理もなくしました。通常の本文はユーザー領域へのコピーが成功してからキューを解放するため、不正な受信先を渡してもメッセージを失わずに再試行できます。外部pageはmapping後に返すアドレスが変わるので、16 bytesのheaderだけをstackへ取り出します。

### カーネルスタックを必要な分だけ確保する

kernel stackはthread生成時にpage単位で確保します。終了中のthreadがまだ使っているstackは即座にfreeせず、CPUが別のstackへ移った後で回収します。各slotには所有するpage tableを記録し、`exec`とprocess終了のどちらでも古いmapを残しません。

仮想アドレスのslot間には未mapのguard pageがあります。ユーザー用page tableにも同じstackをmapしますが、`USER_ACCESSIBLE`は付けず、書き込み可かつ実行不可に固定しています。これでRing 3から割り込みへ入るときのTSS.RSP0を維持しつつ、ユーザーコードからは読めません。

BSPのISTとRing 0 stackはheap初期化前に必要なので静的に残します。AP用はCPUがonlineになるときにframe allocatorから確保し、zeroingしてからTSSへ設定します。存在しないCPUのstackは持ちません。

performance instrumentation buildではstackをpattern初期化し、解放時にhigh-water markを更新します。今回のservice起動試験では65,536 bytes中1,952 bytesでした。例外、深いsyscall、複数threadの負荷をまだ測っていないため、設定値は維持します。

APがlong modeへ移るまで使う8 KiBのbootstrap stackも、固定64本からAPごとの確保へ変えました。さらに、APが通常のidle stack上で動き始めたことをCPU別のtokenで通知し、BSPが一致を確認した場合だけframeを解放します。4 vCPUの起動試験では3本、合計24 KiBをすべて回収してからサービス起動まで進みました。通知が来ない場合はuse-after-freeを避けるため保持します。[AP stack回収後の計測結果](baselines/mnu-kernel-ap-stack-reclaim-2026-09-01.json)に確認条件を残しています。

### allocatorとpage管理を整理する

boot時だけ必要なallocatorを通常運用へ持ち越さず、使い終えた領域を回収します。小さいallocationはsize class別に扱い、断片化、lock待ち、zeroingの費用を測ります。per-CPU cacheは競合が実測された場合だけ導入します。

現在のkernel heapは仮想上限32MiBのまま、256KiBずつ物理pageを追加します。追加処理は一つのlockで直列化し、page table、frame allocatorの順でlockを取ります。部分的な確保に失敗した場合も、mapできた範囲だけをheapへ渡してから再試行します。

allocationごとの検査headerは64 bytesから48 bytesへ縮めました。raw pointerと二つの`usize`をそのまま保存せず、allocation先頭からのoffsetと32-bitのsizeへ置き換えています。magic、cookie、checksum、tail canary、解放時zeroing、layout不一致の拒否は残しています。

計測buildでは、allocation数を要求サイズ、CPU、処理元ごとに分けて記録します。処理元はscheduler、IPC、VFS、page fault、network、block I/O、process生成、thread生成、その他のsyscallです。header、tail canary、alignmentで増えたbytesも、使用中とpeakを別に数えます。分類状態はCPUではなくthread slotに置くため、待機中に別threadへ切り替わっても分類が混ざりません。

heapの内訳はperformance ABI v2の末尾へ追加しました。続くv3には、物理frameの要求数、free listからの再利用、新規領域からの確保、メモリマップを調べた回数、連続確保、失敗理由、zeroingした量と所要cycleを入れています。v1の先頭1,200 bytesとv2の先頭1,896 bytesは動かしていません。

物理frameの新規確保は、以前は要求のたびにメモリマップの先頭から調べていました。現在は最後に使ったregionを覚え、そこから再開します。通常buildの配布用kernelは875,424 bytesのままです。v3の計測buildは896,256 bytesで、v2の計測buildより16 bytes小さくなりました。[物理frame計測の確認結果](baselines/mnu-kernel-frame-observability-2026-09-01.json)に、ABIとビルドの確認範囲を残しています。

frame allocatorのlock待ちは、計測buildで回数、p50、p95、p99、最大cycleを取得できます。未使用pageの総数、free listに戻ったpage数、現在確保できる最大の連続page数も同じsnapshotへ加えました。これだけではper-CPU cacheが必要か判断できないため、通常buildの処理は変えていません。

物理pageのzeroingは`frame::zero_frame`へ集約しました。heap拡張、page table、ユーザーsegment、共有page、per-CPU領域で別々に行っていた初期化を一つの経路へ通し、失敗時の扱いと計測位置をそろえています。共有pageでは物理アドレスの計算に失敗してもmappingを続ける経路があったため、現在は確保済みpageを戻してエラーにします。

performance ABI v5では、確保したpage数をCPU別と処理元別に記録します。zeroingの呼び出し回数とcycleも処理元別に分かるため、競合や初期化費用が偏る箇所を実測できます。計測コードを除いた通常buildは875,480 bytes、LOAD segmentのメモリ量は1,269,962 bytesです。共通化前と比べて`.text`は1,568 bytes、LOAD segmentのメモリ量は4,916 bytes減りました。[frame活動の計測結果](baselines/mnu-kernel-frame-activity-2026-09-02.json)にrevisionと確認条件を保存しています。

実機の処理別page数とlock待ち分布はまだ採れていません。現在の起動試験は既知の`display.driver`終了でシェルへ到達しないためです。計測値がないままper-CPU cache、reserve pool、allocatorの全面置換には進みません。次は割り当て失敗がpanic、syscall error、process終了のどれになるかを追い、OOM時にも部分的なmappingや所有権の取り残しが起きないことを確かめます。

OOM経路の最初の監査では、ユーザーページテーブルの生成途中で失敗すると、呼び出し側から到達できないtable frameが残ることを確認しました。生成中のL4をguardで管理し、L3以下を公開前にその配下へ接続することで、途中の失敗時も既存の破棄処理から回収できます。L2を複製してすぐ置き換えていた処理もなくし、通常の起動構成ではプロセスごとに少なくとも1 pageを使わなくなりました。

frameの確保とzeroingは`allocate_zeroed_frame`へまとめました。zeroingに失敗したframeはその場で返し、page table、ユーザーsegment、cext segmentの各経路で同じ所有権規則を使います。cext segmentはpage全体を初期化するため、segmentの前後に残ったbytesから以前の内容を読めません。

execに必要なpage table、ELF segment、stack、heap、TLSの確保失敗は`ENOMEM`を返します。無効なELFやmapping条件は従来どおり`EINVAL`です。変更後の通常kernelは871,384 bytesで、直前の875,480 bytesから4,096 bytes減りました。[ページテーブルOOM監査の結果](baselines/mnu-kernel-page-table-oom-2026-09-02.json)に、サイズと起動確認を保存しています。

cextの読込も失敗時に元へ戻します。各segmentはRW+NXで内容を書き、PTEを残したまま最終属性へ変更します。現在のpage tableに対する変更ではTLBもflushします。ELFの後半、relocation、symbol検索、module initで失敗した場合は、それまでに割り当てたsegmentを解放します。

このcext rollbackを含む通常kernelは875,488 bytesです。直前から4,104 bytes増えましたが、読込失敗のたびにkernel pageが残る状態と、一時的なRWX mappingをなくすため変更は残します。計測buildは逆に24 bytes減っているため、section配置の差を含めて[cext rollback後の計測結果](baselines/mnu-kernel-cext-rollback-2026-09-02.json)へ記録しました。

cextのDMA確保は、pageを1枚ずつ`Vec`へ積んでから連続性を検査する処理をやめました。frame allocatorの連続確保を1回呼び、得られた範囲をpageごとにzeroingします。途中で初期化に失敗した場合は範囲全体を返します。最大64回あったallocator lockと、free listの順序による不要な失敗がなくなりました。

この変更後の通常kernelは830,216 bytesで、直前から45,272 bytes減りました。`.text`は41,952 bytes減っています。QEMUではDMAを使う`disk.cext`と、その上の`ext2.cext`が読み込まれました。[連続DMA確保後の計測結果](baselines/mnu-kernel-contiguous-dma-2026-09-02.json)に通常buildと計測buildの差を残しています。

同じ監査で、DMA syscallが確保直後の物理pageをzeroingせず、ユーザープロセスへmapしていることも見つかりました。現在は連続範囲をHHDMから全てclearし、完了後にだけprocessへ登録してmapします。アドレス計算や後続処理に失敗した場合は範囲全体を返します。zeroingのbytesとcycleは既存のframe計測へ含めます。

DMA zeroingを加えた通常kernelは859,072 bytesです。直前より28,856 bytes増えましたが、連続確保へ変える前の875,488 bytesより16,416 bytes小さい状態です。以前の所有者の内容をユーザーへ見せないため、このzeroingは削りません。[DMA zeroing後の計測結果](baselines/mnu-kernel-dma-zero-2026-09-02.json)に両方の比較を保存しています。

共有pageの確保では、物理アドレス一覧を返さない通常の呼び出しでも、page数と同じ長さの`Vec<u64>`を作っていました。上限では2 MiBの一時allocationになります。現在はpageをzeroingしてmapした後、失敗時にはmap済み範囲からframeを回収します。物理アドレスが必要な場合だけ、全pageのmap成功後にpage tableから値を読み直して返すため、一時配列は必要ありません。

通常kernelは834,448 bytesになり、DMA zeroing直後から24,624 bytes減りました。LOAD segmentのメモリ量は28,520 bytes減っています。計測buildは反対に4,072 bytes増えているため、通常buildの差にはlink時の4 KiB境界も含まれます。[共有page確保後の計測結果](baselines/mnu-kernel-shared-page-allocation-2026-09-02.json)にsectionごとの差と起動確認の範囲を残しています。

per-CPU syscall領域の2 pageも、共通のzeroing済みframe確保へ揃えました。stack pageの確保に失敗した場合は、先に確保したstate pageを返してから起動を止めます。通常kernelの`.text`は224 bytes、計測buildは8,264 bytes減りました。[per-CPU frame確保後の計測結果](baselines/mnu-kernel-percpu-frame-allocation-2026-09-02.json)には、section配置によりLOAD時メモリが4,008 bytes増えたことも含めています。

BootloaderReclaimableはまだ通常RAMへ加えていません。UEFI loaderの`BootInfo`、メモリマップ、initfsが同じ種類の領域に置かれるため、一括回収すると起動中のデータを再割り当てしかねません。loaderが所有範囲を渡し、mnuが必要な内容を退避してから回収します。

解放後のzeroing、ユーザー空間へ渡すpageの初期化、W^Xは削りません。速さのために前の所有者のデータが見える状態を作らないことが前提です。

### schedulerとtimerの無駄な仕事を減らす

run queueの走査範囲、不要なlock、起床後に実行されるまでの遅れを測ります。固定周期のtickで仕事がないCPUを起こしている場合は、deadlineに基づくtimerへ段階的に移します。lock-free化は回収とメモリ順序まで説明できる箇所に限ります。

### VFS、mmap、execのコピーと確保を減らす

pathの再解析、短命な`String`や`Vec`、同じmetadataの再取得を追います。cloneの数ではなく、実際にコピーしたbytesとallocation数で判断します。execでは署名検証とW^Xを維持しつつ、read、parse、mapping、relocationを分けて測ります。

### 大きなIPCを共有pageへ移す

画像、ファイル、ネットワークpacketなどは、小さいIPCの本文へコピーしません。Grantと共有ringを使い、mnuはpageの共有範囲と所有権だけを管理します。失敗、相手の終了、timeoutのどの場合もpageが回収される状態機械を先に固めます。

### 起動とI/Oを待ち時間中心に組み直す

起動順に依存しない初期化は並行化し、最初の操作に不要な仕事は遅延させます。ただし、後回しにした仕事で最初のアプリ起動だけが遅くならないよう、起動全体と初回操作を一緒に測ります。

block、network、consoleは要求をまとめられる箇所を探し、割り込みとDomain間通知の回数を減らします。バッチを大きくして入力遅延を増やさないよう、throughputとtail latencyを両方確認します。

### 最後にlinkとビルド設定を詰める

未使用sectionの除去、LTO、codegen unit、panic経路、symbolとrelocationを比較します。これは固定領域と実行時allocationを直した後に行います。stripだけで1 MiBへ見せる変更や、速度を測らず`opt-level = "z"`へ寄せる変更はしません。

## 変更を採用する条件

各変更は、対象の数値が改善し、既存テストと追加した回帰テストが通り、計測用featureを切った通常ビルドに不要な費用を残さない場合に採用します。サイズが減ってp99が悪化した場合や、平均だけ改善して最大待ち時間が伸びた場合は、その差を記録して判断します。

Capability、zeroing、W^X、署名検証を外した結果は最適化として扱いません。速くても境界が壊れていれば、OSとしての性能改善にはならないためです。
