# ARM64対応方針

更新日: 2026-11-15

mochiOSをARMタブレットで動かすための移植計画と進捗。以下の実装状況は現行ソースを確認した結果であり、未着手の構成・作業項目は提案である。現在はCPUプリミティブ、EL1継続、例外入口、EL0遷移、Stage-1 MMUを実装し、共有VMへの接続とARM向けC/C++ SDKの配布まで進めた。ARM64版mnu全体の起動には至っていない。

## 着手済みの範囲

- [ARM vCPU](mboot/src/arch/aarch64/vcpu.rs)にEL2からの繰り返し実行・再開を追加した。GPR・全32本のSIMD・FP制御・TLS・EL1翻訳／例外状態・仮想タイマーを保持し、物理CPUとStage-2のroot／VMIDへ継続を固定する。別アドレス空間・非activeルート・HVCやMRSの二重完了を拒否する。Cortex-A57で2つの実ゲストを交互に再開し、製品System Domain ELFでも実MRSトラップから再開してReadyへ到達した。現時点では拡張レジスタバンクを持つCPUを保守的に拒否し、QEMU maxは従来の一度限りの入口診断だけを実行する。Domain間の投機実行対策と製品ディスパッチャへの接続は未完了であり、この機構単独で安全な製品VMMの完成とはしない。
- [GICv2仮想化](mboot/src/arch/aarch64/vgic.rs)にvCPU固有のVMCR・APR・全LR・HCR状態を追加した。GICHはEL2だけが保持し、ゲストには割り当て済みGICVのみを公開する。ソフトウェア起源のcoalesced edgeを注入し、満杯や競合SGI送信元は保持元へエラーを返す。HW LRによる物理IRQのdeactivation権限はまだ公開しない。実GICH/GICVでactiveなIRQの途中切替、優先度分離、再開後EOI、SGI送信元付きtoken、キュー満杯と物理GICへのStage-2拒否を確認した。ログは`out/arm/native-vgic-final-check/run.log`。物理IRQ・仮想タイマーIRQ・実Domain資源との配送接続は引き続き必要。

- [ARM System Domainの起動入口](boot/src/arch/aarch64/entry.S)と[専用リンク配置](boot/domain-aarch64.ld)を追加した。実ELFの低位入口で4 KiBのStage-1を構築し、カーネル・スタック・起動情報をTTBR1の上位領域へ保持する。textはRO/X、rodataはRO/NX、その他のRAMはRW/NXとし、入口の上位別名もRO/NXにする。現在の初期RAM上限はDomainあたり1 GiB。製品ELFを実際のmBootローダーとStage-2で実行し、Cortex-A57とmaxで上位EL1のReady HVCまで到達した。ログは`out/arm/system-domain-stage2-check/run.log`と`out/arm/system-domain-stage2-a57-check/run.log`。診断はReadyで停止し、mnu初期化・サービス起動を通ったことにはしない。x86 System Domainのリンクと既存mBootの145試験も通った。

- [System DomainのGrant RAMアクセス](boot/src/mdriver.rs)でGPAとカーネルVAを分けた。Hypercallと描画面の共有情報にはGPAを渡し、CPUからリング・ストレージ・metadata・描画バッファへアクセスするときだけ、検証済みGrant範囲とカーネルlinear mapを使う。範囲外・加算overflow・不正な仮想アドレスを拒否する。非identityの実バッファでコピーと読み取り専用Grantを確認し、既存の起動データ5試験とx86 System Domainの実リンクが通った。ARMの実I/O確認はmBoot／mDriver結合後に行う。
- **ARM64版mnuライブラリ全体の型チェックと実コード生成が通った。** `aarch64-unknown-none`でコンパイルし、従来の部分的なCPU診断だけでなく、共有VM・syscall・IPC・Capability・スケジューラを含む実ライブラリを生成した。ログは`out/arm/native-mnu-aarch64-build.log`。後続変更でARMブートELFのリンクとReadyまでの入口実行も通ったが、OS起動は未完了。既存x86カーネルも再ビルドできた。
- [ネイティブCPU起動の分離](core/src/smp.rs)により、APIC／CR3／16-bit trampolineをx86側へ移した。ARMは現在1vCPUだけを受け付け、実際に走っているCPUのみをTLB同期の参加者へ登録する。複数vCPUの起動とリモートIPIは未対応として拒否する。診断では1CPUの受け入れ、未完成SMP・x86ハンドオフの拒否を確認した。ARM SMPを完成扱いにはしない。
- [ARMタイマー](core/src/arch/aarch64/timer.rs)と[製品IRQハンドラ](core/src/arch/aarch64/interrupt.rs)を共有の時刻・sleep・スケジューラへ接続した。割り当て済みCPUインターフェースだけを上位のDevice／RW／XN領域へマップし、既存マッピングや非presentの所有済みPTEを上書きしない。タイマーは前回deadlineから再設定し、時刻は共有counter epochから算出してCPU数やIRQ遅延による増速を避ける。EOI完了後にスケジュールし、EL0フレーム・TLS・upcallを保持する。実タイマーIRQ、counterの端数・wrap・オーバーフロー、MMUの空き判定、x86カーネルビルドが通った。製品IRQハンドラ全体をOS上で実行する確認はmBoot結合後に行う。既存タイマー／CPU時間の回帰5件も、実フレームと現在の所有権表現で通った。
- [CPU初期化](core/src/arch/aarch64/cpu.rs)をx86から分離し、EL1のFP/AdvSIMD、CPUごとのPAN/RNDR検出、入れ子のユーザーデータアクセス範囲を追加した。PANを開く間はIRQとCPU移動を制限し、PXNは解除しない。Cortex-A57と`max`で保護状態の復元・未対応RNDRの拒否を確認し、x86カーネルビルドも通った。実運用のCPU admissionはCSV3とEL2経由SMCCCのWORKAROUND_1／3を要求し、未対応を成功扱いにしない。**このfirmware経路はまだmBootへ接続されておらず、製品側のCPU admission全体の実行確認は未完了。** ARMの早期乱数は有効なfirmware seedまたは健全なRNDRを必須とし、RTC・時刻・固定値を代替エントロピーにしない。
- [スレッド切替の例外スタック設定](core/src/task/context.rs)をnative入口へまとめた。x86では従来のTSSとSYSCALLスタックを更新し、ARMでは所有済みEL1スタックを保持して最終EL0遷移で使用する。ARMからGDT/TSSとCR4の初期化を除き、x86カーネルの再ビルドが通った。この段階で残っていたIRQ・タイマー・SMPによるコンパイルエラーは、上記の後続変更で解消した。
- [mBootのELFローダー](mboot/src/image.rs)を所有済みDomain RAMのインターフェースへ分離し、ARMの低位ブート入口＋上位カーネルVMAを別のGPAへロードできるようにした。CPU種別を明示し、x86の通常経路は外国CPUのELFを拒否する。入口は実行可能なidentityセグメント内に限定し、入力・配置・RAM範囲をすべて検証してからコピーする。SDKの実Clang／LLDで作った[再現用ELF](mboot/scripts/elf-layout-probe.ld)でtext・data・BSSの物理配置を確認し、mBootの145試験が通った。後続の診断で製品ELFのReadyまで実行したが、後続でvCPUの繰り返し再開も確認したが、本体のディスパッチャとmnu初期化への接続は引き続き必要。
- [カーネルヒープの配置](core/src/arch/aarch64/memory/mod.rs)をCPU別にした。ARMではTTBR1の上位領域を使い、プロセスのTTBR0を交換してもヒープ配置を失わない。実MMUで下位ルートの交換と上位ヒープの保持・交換を確認した。x86は既存アドレスを維持し、カーネルビルドと既存のヒープ拡張ロック試験が通った。後続変更でSystem Domainの上位配置とGrant RAMのGPA変換も接続した。実I/Oとプロセス切替はOS結合後に確認する。
- [GICv2 CPUインターフェース](core/src/arch/aarch64/gic.rs)に、割り当て済みレジスタ領域の有効化・IRQ受信・完了通知を追加した。CPUとIRQマスク状態を確認し、SGI送信元を含むIAR全体をEOIへ戻す。spurious IRQへEOIを送らず、完了前のCPU移動を認めない。実際のタイマーIRQと範囲・マスク状態の拒否を確認した。診断はGICCを使っており、mBootの仮想GIC状態保存・物理IRQ配送・Domain間隔離の確認はまだ行っていない。
- [ARM割り込み資源の起動契約](core/crates/abi/src/arm64_interrupts.rs)を追加した。mBootが割り当てたGIC CPUインターフェースのGPA・範囲、timer／event／management／TLB／wakeのIRQ役割を明示する。未対応の形式・予約ビット・範囲外・役割の重複・別CPU向けの資源・framebufferとの重複を拒否する。Domain起動ABIはv14、mnu起動ABIはv4へ更新し、既存の送信側も更新した。ABIの50単体＋5結合試験、両CPU診断、mBootの143試験、x86カーネル・UEFIローダー・System Domainのビルドが通った。資源の既定値は空で、ARMのGIC仮想化やIRQ配送を実装済みとは宣言しない。次にこの契約へ実コントローラとタイマーを接続する。
- [診断コンソール](core/src/util/console.rs)から既存COM1実装をx86側へ分離した。ARMはmBootの既存コンソールへ、ATによる現在のStage-1変換で得たゲスト物理アドレスを渡し、ページ境界で分割する。実MMUで上位アドレス・ルート切替・unmap後の変換を確認した。1024 bytesを超える文字列と送信失敗も既存のホスト試験で確認し、x86カーネルビルドも通った。ボードUARTのアドレスはmnuへ追加していない。
- [I/Oポート](core/src/arch/aarch64/port_io.rs)をCPU別に選択する。共通の範囲・Capability確認を維持し、ARMでは存在しないポート命令を成功扱いにせず`Unsupported`とする。デバイスは既存の所有済みMMIO・mDriver経路を使う。診断出力とI/O権限に関する既存の回帰試験を現行APIに合わせ、境界をまたぐ範囲・無権限・未対応CPUでハードウェア処理へ進まないことを確認した。
- [ARM同期例外の製品ハンドラ](core/src/arch/aarch64/interrupt.rs)を追加し、実フレームからSVC、COW、通常のページ解決、upcallへ接続した。外部エラー・無効FARなどを通常のページ不足として解決せず、未処理の特権例外・IRQも成功扱いにしない。スタック拡張とVM予約の確認はx86と共有する。実EL0でread／write／execute abortの分類、既存のVM retry・スタック予約・例外・診断の8試験とx86カーネルビルドが通った。後続変更でARM版mnuライブラリ全体のコード生成まで通ったが、この製品ハンドラのOS上での実行確認は未完了。
- [ARMのSVC入口](core/src/arch/aarch64/syscall_entry.rs)を追加し、x8の番号とx0〜x5の引数、x0の戻り値を既存のsyscall処理へ渡す。x86のSYSCALL／int 0x80入口はCPU別ファイルへ分け、スレッドのsyscall中状態・終了確認は共通化した。EL0レジスタとTLSを保持し、ブロック後に現在の所有済みTTBR0とTLSを選び直す。実ユーザー側ライブラリの0〜6引数呼び出し、非native SVCの拒否、x86カーネルビルド、既存のupcall・継続回帰9試験が通った。
- [スレッド初期状態](core/src/arch/aarch64/thread.rs)をCPU別に生成し、共有のプロセス作成・スレッド作成から呼ぶようにした。ARMはx0へ起動引数、x1〜x30へ明示された汎用レジスタを配置し、PSTATEとFP状態はカーネルが初期化する。両CPUの実特権診断、x86カーネルビルド、既存の初期レジスタ・継続回帰7試験が通った。作成時のCapability・VM・予約領域検証は共有側に維持している。
- upcallの[CPU別スナップショット変換とコールバックABI](core/src/arch/aarch64/upcall.rs)を追加し、権限確認・トークン・例外記録は共有側に維持した。ARMの800-byte snapshotを往復し、不正PSTATE／FPCRを拒否する。実EL0で7引数、16-byte SP整列、LR設定を確認し、元のFP状態付き継続へ復帰した。SVCとIRQは実際の同じARM例外フレームを使い、x86のIRET欄を仮定しない。例外コードはESRのECから取得する。x86カーネルビルドと、既存のupcall・例外配送・レジスタ変換・fault retryの関連12試験が通った。ARMの割り込み／syscall配送全体への接続は引き続き未完了。
- [タスクのEL0入口](core/src/task/usermode.rs)もCPU別に分けた。ARMは現在のプロセスが保持するTTBR0と、CPUローカルの所有済み例外スタックを選び、検証済みの継続復帰を呼ぶ。プロセス／ページテーブル／スタックが欠けた場合にゼロ値で起動しない。既存x86カーネルのビルドと、TLS・upcall・CPU参照・ユーザー復帰に関する既存回帰試験16件が通った。後続変更でmnuライブラリ全体のARMコード生成まで通ったが、このタスク入口のOS上での実行確認は未完了。
- 共有upcall状態のFP保存を[CPU別の実装](core/src/arch/aarch64/fp.rs)へ分離した。ARMは例外フレームからQレジスタ・FPCR・FPSRを保持し、ユーザー指定の不正な制御ビットを拒否してから復帰画像へ反映する。ライブのFPレジスタを意図的に変更した後でも、保持した状態から実EL0へ正しく復帰した。既存x86のMXCSR検証・SSE復元とカーネルビルドも通った。
- [ARMのEL0継続復帰](core/src/arch/aarch64/usermode.rs)で全汎用レジスタ・FP/SIMD・FP制御・TLSを復元し、次の例外用に所有済みカーネルスタックを設定する。PC／SP／PSTATE／FP制御／TLSが不正なら、CPU状態を書き換える前にエラーを返す。実EL0へ復帰し、レジスタ・TLS・FP状態と、その後のNX違反の処理まで確認した。ユーザー状態の内部レイアウトを800 bytesとして固定し、共有スレッド側はPC／SPアクセサを使う。
- CPUローカルの共有スレッド状態を残し、[GS／KPTIのx86実装](core/src/arch/x86_64/percpu.rs)と[TPIDR_EL1のARM実装](core/src/arch/aarch64/percpu.rs)を分離した。ARMは検証済みのハードウェアID一覧から論理スロットを選び、未登録CPUや不正なCPUローカルポインターを拒否する。実際のEL1へ32ビットを超える仮想MPIDRを与え、スロット選択・TLSとの分離・切替中のスレッド参照保持を確認した。x86カーネルのビルドも通った。複数CPUの同時起動・SMP全体は未完了。
- スレッド切替・TLS設定からCPU別の[ユーザーTLS操作](core/src/arch/aarch64/tls.rs)を呼ぶようにした。x86は既存のFS／MSR経路、ARMはTPIDR_EL0を使う。両CPUの特権診断で書き込み・読み戻し・元の値への復元が通り、ARMではTPIDR_EL1が変更されないことも確認した。x86のカーネルビルドも通った。
- [起動ABI](core/crates/abi/src/boot.rs)のバージョン3で、CPU種別と64ビットのハードウェアIDを追加した。ARMのMPIDR affinityをAPIC IDへ切り詰めず、論理スロットへ明示的に対応付ける。別CPU向け・旧版・重複ID・無効ビットを拒否し、既存のUEFIローダーとSystem Domainも同時に更新した。ARM向け型チェック、ABIテスト52件、x86のカーネル・UEFIローダー・System Domainのビルドが通った。ARM SMPのネイティブ起動状態は未実装で、x86用CR3ハンドオフをARMで受け付けない。
- [mBootのARM Stage-2](mboot/src/arch/aarch64/stage2.rs)に4 KiBページ、連結ルート、Normal／Device属性、読み書き・実行権限、VMID選択、TLB無効化、break-before-make、確保失敗時の回収を追加した。編集対象は明示的に選択したアドレス空間に限定し、別Domainへ暗黙に切り替えない。[EL2診断](mboot/scripts/check-stage2.py)は製品のマッパーを使い、実際のEL1ゲストでRO／XN違反、mBoot用ページへのアクセス拒否、unmap、VMID切替・再利用、L0／L1／L2開始の翻訳を確認した。QEMU Cortex-A57とmaxの両CPUモデルで通った。既存x86のmBoot単体テスト143件も通った。これはCPUのメモリ分離機構であり、vCPU実装・DMA隔離・ARM版OS全体の起動完了ではない。
- [arch層](core/src/arch/mod.rs)へ、ローカルIRQ制御、CPU待機、スタックポインタ参照を集約した。スピンロック、スケジューラ、メモリ管理などの高水準の呼び出しはこの入口を使う。
- [AArch64バックエンド](core/src/arch/aarch64/mod.rs)にDAIFのIRQマスク操作、WFI、SP参照を実装した。Debug・SError・FIQのマスクは変更しない。
- QEMU ARM64のEL1で、IRQ無効化、有効化、ネストした区間の状態復元、他のDAIFマスクの保持、マスク中／解除後の仮想タイマーによるWFIの復帰を確認した。
- x86_64の[保存フレームと切替命令](core/src/arch/x86_64/context.rs)を`arch/`へ分離した。スレッドの初期スタック・復帰先・IRQ状態は`Context::for_kernel_entry`で作成し、共有側はSP・PCのアクセサを使う。既存のスケジューリング方針と切替時の所有権管理は変更していない。
- [ARM64のEL1カーネルコンテキスト](core/src/arch/aarch64/context.rs)を追加した。x19〜x30、SP、継続先、NZCV、DAIF、全32本の128ビットFP/SIMDレジスタ、FPCR・FPSRを保存・復元する。旧スタックを離れた後に、切替完了マーカーをreleaseでクリアする。
- [継続の診断](core/scripts/context-probe.rs)で別スタックへ入り、異なる汎用レジスタ・SIMD値・FP制御を設定した2つの継続を16往復した。ARM64ではNZCVと継続ごとに異なるIRQ状態も確認した。x86_64でも同じ往復診断を確認した。
- x86_64の[例外・syscallフレーム](core/src/arch/x86_64/exception.rs)を`arch/`へ移した。`cpu::context`は既存呼び出し元のための再公開とし、構造体・レイアウト・変換内容は変えていない。
- [ARM64の例外フレーム](core/src/arch/aarch64/exception.rs)と[EL1ベクタ](core/src/arch/aarch64/exception.S)を追加した。全汎用レジスタ、SP_EL0、ELR/SPSR、ESR/FAR、FP/SIMDをカーネルスタックへ保存する。ELR/SPSRを保存してからハンドラを呼ぶため、ネストした例外からも復帰できる。
- ベクタは`mnu_aarch64_exception_dispatch`へ処理を渡す。サービスやデバイスを認識せず、受信の種類・発生元と保存フレームを渡すだけ。カーネル側のsyscall・フォールト・IRQ配送との本接続は今後の作業。必要なハンドラをリンクせずに成功する空実装は置かない。
- 復帰の特権状態と発生元はフレームの非公開情報とし、EL1hまたはEL0tの元のモードだけに復帰する。未対応のEL1t／AArch32、破損したモード、ハンドラが停止を指定した場合はERETせずCPUを停止する。
- [初回EL0遷移](core/src/arch/aarch64/usermode.rs)を追加した。明示された引数とTLSを設定し、他の汎用レジスタ、FP/SIMD、FP制御、読み取り専用TLSレジスタをクリアする。メモリマッピングとユーザーアドレスの検証は呼び出し側の責務。
- ARM診断は専用IRQ入口の重複を削除し、製品側ベクタを使用する。実際のタイマーIRQ、ネストしたBRK、[EL0診断](core/scripts/user-probe.rs)のSVCと戻り値・FP/SIMD復元、初期レジスタの消去、TLS設定、特権命令のトラップが通った。
- [Stage-1 MMU](core/src/arch/aarch64/paging.rs)に4 KiBページ、48ビットVA、CPUのPA幅検出、Normal／Device属性、W^X、ASID切替、TLB無効化、break-before-makeを実装した。途中のテーブル確保に失敗した場合は未公開のテーブルを回収し、部分経路を公開しない。
- [MMU診断](core/scripts/mmu-probe.rs)では実際にMMUを有効化している。ASIDごとに同じVAが別の物理ページへ翻訳され、ASIDの再利用前の無効化、権限更新、unmap、確保失敗の回収が通った。EL0からカーネルページを読む、実行コードを書き換える、スタック上のコードを実行する操作は、それぞれ実際のpermission faultになった。
- [ユーザー側syscall](user/crates/syscall/src/arch/)をCPUごとに分け、ARM64では`SVC #0`・番号x8・引数x0〜x5・戻り値x0とした。syscallの番号・意味は変えていない。実際のクレートをEL0へリンクする任意診断で、0〜6引数すべての関数と戻り値を確認した。
- 共有VMのアドレス・ページ型は`arch::memory`を参照する。[ARMの論理フラグ変換](core/src/arch/aarch64/memory/paging.rs)ではCOW・一時的なアクセス停止・常駐RAM・カーネル所有情報を、ARMの無視されるソフトウェアビットへ変換する。CPUのAP／XNビットへ既存のx86ビット列を直接流用しない。共有VMの接続全体は作業中。
- [VMアダプタ](core/src/arch/aarch64/memory/mapper.rs)から実際のマップ・翻訳・権限変更・unmapを実行した。既存のフレームアロケータへ接続し、公開前の確保失敗は同じアロケータへ返す。管理用フラグの更新だけで実行中の親テーブルを一時解除しない。
- [翻訳ルート制御](core/src/arch/aarch64/memory/control.rs)はTTBR0（プロセス）とTTBR1（上位カーネル）を分ける。同じVAの物理ページをルートごとに変え、TTBR0切替で上位の翻訳が変わらないことと、TTBR1交換後に古い翻訳が残らないことを実MMUで確認した。共通スケジューラ接続用APIはASID 0とローカル全無効化を使い、所有済みASIDのAPIとは区別する。
- [x86のKPTI初期化](core/src/mem/paging/x86_64.rs)を分離した。ARMのプロセスルートは空のTTBR0から作成し、GDT・IDT・syscallトランポリンのx86用マッピングを複製しない。ユーザー配置判定も、ARMの明示的supervisorマッピングをx86用の破棄可能なidentity aliasと解釈しない。
- [ネイティブレジスタABI](core/crates/abi/src/thread_registers/aarch64.rs)にARM用ThreadCreateContext（304 bytes）・ThreadUpcallSnapshot（800 bytes）を定義した。作成メッセージはCPU別タグで区別し、他CPUのレジスタ列を受け付けない。x86のレイアウトとタグは維持した。
- [EL0復帰の検証](core/src/arch/aarch64/exception.rs)はPC／SPの範囲と整列、NZCV以外のPSTATE、未対応FPCR／FPSRビットを検査してから一括反映する。不正な入力で保存状態が部分的に変わらず、有効な状態で実際にERETできることを確認した。VMマッピングの検証と所有権管理は引き続き共有VM側が担当する。
- [POSIXクライアント](user/crates/posix-client/src/snapshot_aarch64.S)へAAPCS64継続点保存、[signal用チェックポイント](user/crates/posix-client/src/signal_checkpoint_aarch64.S)、[別スタック切替](user/crates/posix-client/src/signal_stack_aarch64.S)を追加した。[任意CPU診断](core/scripts/posix-context-probe.rs)が実際のrlibをリンクし、EL1でレジスタ・FP/SIMD・復帰スタックを確認する。子継続の確認は同じアドレス空間内で行い、OSのfork実行確認とは扱わない。同期チェックポイントの最終分岐ではAAPCS64のIP0を使用する。
- [ARM向けRustターゲット](user/targets/aarch64-unknown-mochios.json)を追加し、libcの型チェックとCランタイムのライブラリ生成が通った。C／Rustのsignal context配置も照合した。libcの公開utsnameを既存Cヘッダーの1280 bytesへ揃えた。既存x86のsnapshot・signal・別スタック・C ABIの16テストが通った。
- CランタイムへARMのsetjmp保存領域（224 bytes）、jumpアセンブリ、整列したpthread入口、TPIDR_EL0参照、TLS descriptor resolverを追加した。ここはクロスビルド・Cヘッダー配置確認までで、OS上のsignal／longjmp動作確認はまだ行っていない。Variant-I TLS本体とARM動的再配置は以下の追加確認を行った。OS上のsignal／longjmp実行確認は引き続き未完了である。
- [Variant-I TLS](user/crates/posix-client/src/static_tls.rs)とDTVを実装し、TPの後ろへTCB・静的TLSを配置した。強い整列条件を持つ依存モジュールを追加しても、先頭モジュールのTPオフセットは変えない。実メモリで初期値・ゼロ領域・整列・DTV境界を確認し、既存x86のVariant-IIと実リンカを使うテスト14件も通った。
- [ネイティブELF再配置](user/crates/posix-client/src/pie/architecture.rs)へARMのRELATIVE・ABS64・GOT／PLT・静的TLS・TLSDESCなどの識別を追加した。Clang/LLDが生成したARM DSOを実際のローダーで処理し、加数・ポインタ・TPRELを確認した。別CPUのELFと未対応の再配置は拒否する。既存x86のELF／TLS／IFUNCテスト71件が通った。
- [crt0](user/runtime/crt0.S)と[リンカスクリプト](user/runtime/linker.ld)を両CPUでコンパイル・リンクした。[newlibビルダー](user/scripts/build-newlib.sh)は`--arch aarch64`を受け付け、別CPUの既存出力への混入を拒否する。generic newlibの`_gettimeofday`も、既存のユーザーポインタ検証・errno伝播へ接続した。
- [devkit](tools/devkit/sdk/build.py)がCPUごとのcompiler-rt builtins、newlib、mochiOSランタイム、libc++、libc++abi、libunwind、TLSアダプタを生成する。コンパイラ・LLDアダプタ・CMake・ELF検証は`share/native-target`を共有する。ARM64／既存x86 SDKのホスト検証はそれぞれ40件成功、任意の1件は対象外だった。ARMでのC/C++実行ファイル・DSOリンク、W^X・RELRO・NX、未解決import拒否、出力保全、SDKを実際に別のパスへコピーした再ビルドも通った。ホストのG++へ暗黙にフォールバックしない。
- ARM SDK [27.0 dp.1 shared.11](https://github.com/mochiOS/devkit/releases/tag/sdk-27.0-dp.1-shared.11)を公開した。固定ソース・配布済みホストLLVMのSHAインベントリ・ライセンスを確認し、同梱ソースを含む10,509ファイルのパス監査を通した。アーカイブ、manifest、SHA256SUMSを配布している。changelogは既存の警告・版名・箇条書き・Full Changelogの書式を維持した。SDKは`runtime_tested: false`であり、ARM版OSやアプリの起動完了を示さない。
- [CPUカウンタ](core/src/arch/counter.rs)をarch層へ分離し、ARMはCNTVCT_EL0とCNTFRQ_EL0を使用する。x86は既存のTSC／RDTSCPと校正処理を維持する。ARMの値をx86命令対応として報告せず、両CPUでカウンタの進行と従来のコンテキスト・EL0診断を確認した。
- 既存のmochiOS向けRust標準ライブラリ（`nightly-2026-07-16`のsysroot overlay）をARMでチェックし、thread・channel・current_dir・fs・timeを使うプログラムを公開SDKで実ELFへリンクした。`core.service`もARMでリンクし、CPU種別・W^X・RELRO・NXを検証した。core固有のx86固定リンカスクリプトは共通ランタイムと重複していたため除去し、x86の配布SDKと従来の基本SDK双方でもリンクを確認した。どれもOS上での実行確認とは区別する。
- [ハイパーコール輸送](core/src/arch/hypercall.rs)をarch層へ分離した。ARMはHVC #0でX0の番号／戻り値とX1〜X3の引数を渡す。QEMUでは別スタックを持つEL2 fixtureへ実際に遷移し、全幅のレジスタ受け渡しと他CPU方式の拒否を確認した。起動情報はARMタグを区別し、ARMのAPICフラグを拒否する。未実装の機能をコンストラクタから宣言しない。これはmBootのStage-2・vCPU・Domain分離の完成ではない。既存x86ビルドとABIの49テストが通った。
- x86_64のkernel producerを新しいMDP経路でビルドした。OS全体の起動確認は別途必要。

診断は[check-cpu-primitives.py](core/scripts/check-cpu-primitives.py)から実行する。実装本体を直接読み込み、架空のIRQ状態や保存・復元の代替実装で置き換えない。QEMU 8.2以降のARM/x86バイナリ、OVMFの4 MiB版、指定Rustツールチェーンの`aarch64-unknown-none`と`x86_64-unknown-uefi`ターゲットが必要。

```sh
python3 core/scripts/check-cpu-primitives.py --toolchain nightly-2026-05-14
```

ARM診断の参照機は`virt-8.2`、`cortex-a57`、GICv2、CPU 1個、RAM 128 MiB。EL1の例外入口は製品側の実装を使い、UART・割り込みコントローラ設定と診断用の例外ディスパッチだけをfixtureに置く。mnuの起動・例外処理方針との統合やmBootの移植完了とは扱わない。診断は命令動作を確認するTCG実行であり、既存OSのKVM起動設定を変更しない。成果物とログは既定で`core/out/mdp/cpu-primitives/`に保存する。`--out`、`--qemu-aarch64`、`--qemu-x86-64`で検証環境を指定できる。

ARM64のコンテキストは、ベースの[AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst)とFP/SIMDを対象にする。SVE/SMEを有効にしたビルドは拒否し、起動側でもそれらを有効にしない前提。FP/SIMDアクセスの製品側初期化、タスクごとのTLS切替、アドレス空間切替、SMPでの受け渡しは、この継続切替だけで完成したとは扱わない。ユーザーupcall・syscallレジスタABIのARM実装も追加済みだが、OS上での結合確認は引き続き必要。

`--native-syscall-rlib <同じターゲット・ツールチェーンのmochi-user-syscall.rlib>`と`--posix-client-rlib <mochios-posix-client.rlib>`で実クレートを追加検証できる。依存ABIを変更した後は、両rlibを再生成してから実行する。ARMのTLS配置は[LLD 18のTPオフセット計算](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/lld/ELF/InputSection.cpp#L573)と[AAELF64](https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst)を基準にする。

例外の配置・ELR/SPSR・ERETは[Armの例外モデル](https://developer.arm.com/documentation/102412/latest)を基準にする。今回のEL0診断はMMUを有効にした参照RAMで実行し、CPUによるアクセス権・書き込み禁止・実行禁止を確認した。mnu本体の複数プロセス、実際のsyscall配送・IPC、mBootのDomain分離との統合はまだ済んでいない。診断用のSVC即値やsyscall番号は製品ABIの追加ではない。

## 現行ビルド経路

[MDPとmdevの構成](docs/development/mdp-and-mdev.md)に従う。各リポジトリのproducerがMDPを生成し、ルートの`mmake build`は固定済みMDPのイメージ組み立てだけを行う。VMの起動・状態・ログはmdevが管理する。

```sh
mdp workspace lock
mdp workspace build --select kernel -j 2
```

この選択の前提producerは`context`だけで、SDK producerは実行しない。カーネル成果物は`core/out/mdp/`、OSイメージは`out/images/system/`、組み立ては`out/build/os/`、共有キャッシュは`out/cache/`、VMは`out/develop/`に分かれる。ARM調査の`out/arm/`と各producerの`out/`は移動しない。既存キャッシュ向けの互換シンボリックリンクを削除しない。

部分ビルドの`mdp.assembly.lock`には、入力・依存・成果物の同一性が現在も一致するproducerだけが残る。今回の出力構成変更後に実行したkernel単独ビルドでは、kernelだけが残った。これは完全なOSイメージを組み立てられる状態ではない。全OS用の成果物を揃える作業と、ARMのCPU診断の成功を区別する。

## 対象と最初の到達点

初期対象はAArch64（ARM64、little endian）とし、ARM32は含めない。対象タブレットの機種・SoC・firmwareは未確定のため、実機の対応可否はまだ判断しない。

最初の到達点は、QEMUのARM64環境で次を通すこと。

1. mBootが起動し、System Domainのmnuへ制御を渡す。
2. mnuが例外処理・メモリ保護・タイマー・スケジューラを初期化する。
3. 最小のユーザープロセスを起動し、syscallとIPCによる通信を実行する。
4. 不正なアクセスが該当プロセスの失敗として処理され、カーネルや別Domainを破壊しない。

その後、SMP、mDriverとのI/O、実機の起動、Binderの表示・タッチ入力へ進む。既存のx86_64環境は維持する。

**今回の追加指示により、ARM向けSDK・libc・devkitのビルド対応とリリースも対象に含める。** 公開の許可は今回のARM対応に限る。ARM64のサービス・Binder・アプリを実際にビルドするためのランタイム・Rust標準ライブラリ・ターゲット情報も揃える。C/C++ SDKは上記shared.11として公開済み。ARM64のRust標準ライブラリとcore.serviceはクロスリンクまで確認した。MDP producer・他サービス・Binderのビルドと、mnu／mBootとの結合は引き続き必要。

既存のdevkitリリース`sdk-27.0-dp.1-shared.10`の本文を確認済み。変更履歴は同じ書式（太字のprerelease注意文、`mochiOS SDK ...`の見出し、変更の箇条書き、末尾の太字`Full Changelog`と比較リンク）に従う。ターゲット名を変えただけのアーカイブや、実行確認をしていない機能を動作済みとして公開しない。

## 現在の実装

ARM用のターゲットを追加するだけでは起動できない。CPU依存処理は複数の層に存在する。

| 対象 | 確認した現状 | 主な移植箇所 |
|---|---|---|
| mnuのビルド・入口 | 実ライブラリ全体のARM型チェック・コード生成と既存x86カーネルビルドが通った。ARMブートELFのリンク・起動は未完了 | ARM用のイメージ配置、呼び出し規約、ブート入口・初期スタック |
| CPU・割り込み | CPU初期化・保護設定、割り当て済みGIC・タイマーIRQからのスケジューラ接続を実装。ARMは1vCPUのみ受け付ける | firmware mitigation供給、実vCPU上のプリエンプション・例外復帰、SMPとリモートIPI |
| メモリ | Stage-1 MMUと共有VMアダプタ、TTBR0／TTBR1制御、ARMの上位ヒープ配置を実装した。GDT/TSSとCR4初期化はx86側だけで行う | ARMの初期配置・マッピング、SMPでのTLB整合 |
| タスク | CPU別のEL1継続、EL0復帰、初期レジスタ・upcall・FP・TLSを実装した。製品のタスク入口は実コード生成まで通り、OS上の実行は未確認 | 上位カーネル配置・例外スタックと実スケジューラを接続し、IPC・Handle転送を実行する |
| ELF | [mnuローダー](core/src/elf/loader.rs)はnative CPU種別を検査する。ユーザー側のARM再配置は実Clang/LLDのDSOで確認済み。[mBootローダー](mboot/src/image.rs)もCPUを検査し、ARMの上位VMAと物理ロード先を区別する | 製品vCPUの繰り返し実行、mnu初期化とサービス実行 |
| 起動ABI | BootInfo v4・DomainBootInfo v14にCPU種別、64ビットID、明示的なARM IRQ資源を定義した。既存のx86 SMPハンドオフはARMで拒否する | ARMの実vCPU起動・SMP状態と資源供給側の実装 |
| syscall・実行ABI | [ARMラッパー](user/crates/syscall/src/arch/)のSVC、カーネル側のSVC配送、native保存状態・継続ABIを実装した | OS上でのブロック・再開・例外復帰と、既存IPC・Capability処理の確認 |
| mBoot | ARMのStage-2とEL1からのHVC受け渡しをCPU診断で確認済み。ARMのvCPU再開とGICv2状態保持を実CPU命令で確認済み。本体の起動・実行経路は引き続きIntel VMX／AMD SVM | ネイティブ実行経路への接続、CPU拡張バンク、物理IRQ・タイマー配送、投機対策、DMA隔離 |
| System Domainの入口 | [system_domain.rs](boot/src/system_domain.rs)はIRQ資源を渡し、I/OのGPAもlinear map経由でアクセスする。ARMは低位入口でStage-1を作り、実ELFで上位EL1のReadyまで実行確認済み | Ready後のmnu初期化、仮想GIC・firmware・mDriverとの結合 |
| mDriver | LinuxのPVH起動、PCIデバイス割り当て、`vmcall`／`vmmcall`を使用 | ARM64 Linuxの起動・FDT・Domain接続・対象SoCのデバイス割り当て |
| OSビルド | ARM SDKを公開しRust標準ライブラリ・core.serviceのクロスリンクを確認した。製品のMDPカーネル・ブート・イメージはx86向け | CPU選択、ARMブート成果物・Linux・ユーザーランドを使うOSイメージ生成 |

BinderのタブレットUIと`mmake ui-tablet`は、ARMの起動対応とは別の状態にある。ホスト上でUIを操作できることは、ARM実機のブートやタッチ入力が完成したことを意味しない。

## 責務を維持する

[既存のDomain構成](docs/mboot/about.md)を移植先でも維持する。

| 層 | ARM対応で担当すること |
|---|---|
| mBoot | 物理CPU・RAMの分離、EL2、Stage-2変換、物理割り込みの管理と仮想割り込み配送、DMA隔離、Domain・Grant・Event Channelの管理 |
| mnu | System Domain内の例外、Stage-1 MMU、タスク切替、syscall、IPC、Capability、仮想割り込み・タイマーなどのカーネル機構 |
| mDriver | 割り当てられた物理GPU・ストレージ・USB・タッチ・ネットワークなどのドライバとI/Oバックエンド |
| ユーザーランド | アプリ起動、権限承認、ファイルシステム、POSIX互換性、セッション、GUI、機種別のシステムポリシー |

mnuにサービス名、ユーザー空間のディレクトリ構成、アプリ名、機種別ドライバの知識を追加しない。必要なハードウェア情報は、検証済みの起動情報や汎用インターフェースで渡す。

mDriver側にLinuxドライバが存在しても、mBootのARM仮想化、LinuxのDomain起動、デバイス資源の割り当てまで自動的に成立するわけではない。特にSoC内蔵デバイスはPCI前提で扱わず、MMIO・IRQ・DMA・関連資源の所有関係を確認する。

## アーキテクチャ依存処理の分離

mnuでは、まず既存のx86_64実装を動作を変えずに整理する。次の配置でCPUプリミティブとカーネル継続を分離した。例外配送・ユーザー復帰を製品経路へ接続し、MMUやCPUローカル状態を分離する作業は引き続き必要。

```text
core/src/arch/
  mod.rs
  x86_64/
  aarch64/
```

分離対象は、初期入口、例外フレーム、割り込み制御、MMU・TLB、CPUコンテキスト、ユーザーモード遷移、TLS、タイマー、CPUローカル状態、SMP、必要なバリア・キャッシュ操作。

スケジューリング方針、Handle表、IPCの権限判定、Capability、VM objectの所有権管理などは共有する。選択はアーキテクチャ層の入口で集約し、各サービスやUIへ`target_arch`判定を広げない。全体の大規模な書き換えや不要な動的ディスパッチは行わない。

ARM側では次を実装・確認する。

- mnuのEL1例外ベクタと、EL0からのsyscall・フォールトの処理、検証済み状態への復帰。
- 汎用レジスタ、SP、復帰先・状態レジスタ、FP/SIMD、TLSの保存・復元。
- Stage-1ページテーブル、アクセス属性、実行禁止、ASID、TLB無効化、命令キャッシュとの整合。
- CPUローカル状態、仮想タイマー、プリエンプション、割り込みの配送。
- ARMのメモリ順序を踏まえたロック、共有キュー、Grant、MMIO・DMAの同期。

初期ページ粒度は既存実装との接続を評価しやすい4 KiBを候補とする。対象CPUの対応粒度と、共有メモリ・アライメントの前提を確認して確定する。x86のPTEビットやメモリ順序をARMへそのまま移さない。

## 起動・実行ABI

`BootInfo`、`DomainBootInfo`、SMP引き渡し情報、スレッド初期状態・upcallコンテキストを確認する。

- メモリ範囲、イメージ、共有領域、画面、エントロピーなどの共通情報は維持する。
- アーキテクチャ識別と、CPU識別・変換テーブル・割り込み・タイマーに必要な固有情報を明示する。
- APIC IDやCR3の欄へ、ARMの値を意味の異なるまま詰め込まない。
- 外部へ渡すレイアウトは、サイズ・バージョン・アライメント・予約領域を定義し、受信時に検証する。変更する場合は送信側と受信側を同じ段階で更新する。
- ELFのアーキテクチャ検証を維持する。ARM64ビルドでx86 ELFを実行可能として扱わない。
- 既存native syscallの意味、Handleの権利、エラー体系を共有し、命令とレジスタによる受け渡しをアーキテクチャ層で実装する。
- LinuxアプリケーションABIとの互換性は既存の互換層の責務とし、ARM移植を理由にPOSIX処理をmnuへ移さない。

mDriverのARM64 Linuxを起動する際は、[LinuxのARM64起動仕様](https://docs.kernel.org/arch/arm64/booting.html)に従う。mnuの起動ABIはmochiOS側で定義し、両者の呼び出し規約を混同しない。

## 進める順序と完了条件

各段階を小さいコミットに分ける。実装前の配置変更と、動作を追加する変更を分け、各段階で既存のx86_64経路も確認する。

| 段階 | 作業 | 完了条件 |
|---|---|---|
| 0 | 対象条件とQEMU構成を確定 | firmwareからの起動経路、EL2の利用、CPU・割り込み・メモリ構成を説明できる。実機依存の未確認点を残しておく |
| 1 | mnuのCPU依存処理を分離 | x86_64の起動、ユーザープロセス、IPCが従来どおり動き、共有側にx86固有レジスタ操作を残さない範囲を段階的に広げる |
| 2 | 起動ABIとDomain入口を整理 | x86_64を維持しつつARM64の引き渡し形式を定義でき、不正なアーキテクチャ・サイズ・バージョンを拒否できる |
| 3 | mBootの最小ARM64バックエンド | EL2で動作し、分離されたメモリ上の最小EL1ゲストを実行して、ハイパーコールと終了を処理できる |
| 4 | mnuの単一CPU起動 | System Domainのmnuが起動し、例外、Stage-1 MMU、メモリ確保、タイマーをシリアルで追跡できる |
| 5 | EL0・syscall・IPC・保護 | 複数の最小ユーザープロセスを切り替え、IPCと権限縮小付きHandle転送を実行できる。不正ポインタ、無権限操作、実行禁止違反を安全に処理できる |
| 6 | SMPと共有資源の同期 | 二次CPU起動、プリエンプション、アドレス空間切替、TLB shootdown、FP/SIMD・TLSの分離が成立する |
| 7 | mDriverとI/Oの接続 | ARM64 LinuxのHardware Domainが起動し、Grant・Event Channelを使って必要な仮想デバイスを利用できる。物理割り当てではDMA隔離も成立する |
| 8 | 実機とタブレットUI | 対象機種で起動し、別途用意したARM64ユーザーランドによりBinder・アプリ・実タッチ入力を操作できる |

段階3以降はmBootとmnuを往復して進める。mnuの依存分離だけでARMタブレットが起動する、とは扱わない。単独のCPU診断コードを使う場合も、製品のDomain分離を省略した状態を移植完了に数えない。

## 開発環境と実機で確認すること

初期の参照環境はQEMU ARM64 `virt`を候補とする。実機固有の周辺回路を再現するボードではないため、機種別対応とは分けて扱う。[QEMUの公式仕様](https://www.qemu.org/docs/master/system/arm/virt.html)で、AArch64 CPUの明示指定と、mBootに必要な仮想化拡張の有効化を確認する。仮想化拡張は既定では無効。

最初はCPU 1個・シリアル出力を使い、参照するCPUモデル、GIC、RAM、firmware、QEMU版を固定する。ARM64ホスト上でmBoot自体をKVMゲストとして検証する場合は、ネスト仮想化とゲストEL2の利用条件を確認する。ARM64ホストであることだけを根拠に、この構成を利用可能とは扱わない。x86_64ホストでARM64をTCG実行した結果を実機性能の指標にはしない。既存のx86_64 KVM起動設定は変更しない。

対象タブレットが決まったら、次を調べる。

- 機種名、SoC、CPU、対応するページ粒度、物理メモリ構成。
- ブートローダーの起動許可と、UEFI・Device Tree・ACPIなど利用できる情報。
- firmwareからmBootがEL2を利用できるか、既存のハイパーバイザーとの関係。
- 割り込みコントローラ、タイマー、二次CPUの起動方法。
- SMMUなどDMA隔離機構の有無と、デバイスごとの接続・隔離単位。
- Linux側のGPU、画面、タッチ、ストレージ、USB、Wi-Fiなどの対応と必要なfirmware。
- SoC内蔵デバイスのMMIO・IRQ・DMA・クロック・リセット等の依存関係。
- シリアルなど、GUIが出る前の段階を人間が追跡できる経路。

EL2や必要なDMA隔離が利用できない機種は、現在のDomain構成でそのまま対応可能と判断しない。起動やデバイス割り当てに必要な条件を明示し、保護を無効化して動作済み扱いにしない。

## 守る条件

- x86_64の既存動作を維持し、機能単位で移植する。
- カーネルに特定サービス・アプリ・ユーザー空間のパスを認識させない。
- Capability判定、コード同一性検証、Handle転送時の権限縮小を維持する。
- 実行状態の復帰時に、ユーザー入力から特権状態・カーネル側アドレスを設定できないようにする。
- W^X、ユーザー／カーネル分離、Domain間のメモリ分離、DMA隔離を維持する。
- 未実装の処理を成功として返さず、CPU機能や起動条件が不足する場合は明確に停止する。
- initfsへGUIや一般アプリを追加せず、既存の最小構成を維持する。
- ソースの変更、Lock更新、各リポジトリの参照とMWS記録を一組として管理し、Lockを巻き戻さない。

共有VM、CPUローカル状態、継続復帰、起動契約、ARM SDKまで段階的に実装・検証している。ARM版mnuライブラリ全体のビルド、System Domain ELFのリンク、Stage-2下の上位EL1入口は確認済み。OS起動は未完了である。続いてmBootのARM vCPU・firmware・仮想GIC・I/Oを接続し、mnu初期化・サービス起動・SMPへ進む。診断の成功をARM版OS全体の起動完了と扱わず、空実装でビルド成功を装わない。
