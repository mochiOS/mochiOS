# ARM64対応方針

更新日: 2026-11-19

mochiOSをARMタブレットで動かすための移植計画と進捗。以下の実装状況は現行ソースを確認した結果であり、未着手の構成・作業項目は提案である。現在はCPUプリミティブ、EL1継続、例外入口、EL0遷移、Stage-1 MMUを実装し、共有VMへの接続とARM向けC/C++ SDKの配布まで進めた。QEMU Cortex-A53上でmnuの初期化と実EL0のcore.service、Linux Hardware Domainの実PID 1まで実行できた。両Domain間のGrant／EventとmDriver制御接続も通った。実データ入りSystemとDataを接続し、通常のSecureUIログイン、User、Keychain、Workspace、Binderまで起動した。実画面で壁紙、上部バー、時計、Dockと実Terminalアイコンを確認した。物理入力、製品ARM起動媒体、対象タブレットでの起動は未完了。

## 2026-11-17の結合実行

製品mBootライブラリと実際のARM64 System ELF／core.service／Linux 6.12.98を使い、QEMUの単一Cortex-A53で確認した。

- mnuがフレームアロケータ、Stage-1 MMU、ヒープ、仮想タイマー、スケジューラを初期化し、実EL0のcore.serviceへ入った。exec側に残っていたx86限定のELF machine検査をnative定数へ変更した。
- EL2専有のCNTHPタイマーと物理GICのIARトークンを管理し、再設定後に完全なトークンでEOIする。ゲストにはGICVと独立したソフトウェア状態だけを渡す。LinuxのGICDはエミュレーションし、物理Distributor／GICHは公開しない。
- mDriverの制御・block・GPU・inputバックエンドを共有側へ移し、CPU別処理を検出・輸送・PCI IRQ等へ分離した。ARMはHVCと生成DTBで起動し、既存の実initがPID 1として動作した。ARMの物理PCI割り当ては未対応として拒否する。
- 両Domainを保持して既存のGrantTable／EventChannelTable／schedulerを利用した。Ready、範囲を確認したGrant共有、制御ネゴシエーションとセッション確立まで通った。core.serviceから通常のCapabilityチェック付きStorageControlで遅延初期化を要求する。

制御のみのログは`out/arm/mdriver/native-control/run.log`。続いて、Linuxの実`/dev/ram0`を明示的な一時ブロックデバイスとして接続した。initが実データをコピーし、Linux側で保護MBR・両GPT・CRC・パーティション境界を検証した後に公開する。ソフトウェアデバイスの機能ビットを分け、物理PCI requesterやDMA割り当てを装わない。Systemは読み取り専用、Dataは書き込み可能な既存のユーザー空間Filesystemへ接続する。この診断用RAMディスクの容量は、実機のインストール先容量を制限しない。

`out/arm/mdriver/native-services-diagnostics/run-compositor-user-failure.log`で、core.serviceからLogger、Capability、Signature、Package、Service Manager、Filesystem、POSIX、Drivers、Input、Display、Compositorまでの実起動を確認した。コードとマニフェストは実際のARM ELFから既存の`msign`で結び付け直し、System／Dataをマウントする。後続サービスもCapability内の共通ローダーで準備・測定し、停止状態のProcess Handleを返す。Service Managerが既存のPOSIX登録と失敗時の後始末を行ってから再開する。任意PID／FDや送信者のHandle番号を起動要求へ載せず、IPCの明示的な添付だけを子へ権限縮小して委譲する。

参照機に限ってQEMUの実RAMFBをEL2が設定し、fw_cfgのDMA制御はどちらのDomainにも公開しない。Systemには所有済みの描画RAMと既存のファームウェア表示APIを渡す。mnuは保持中のバッファをページごとにGPAへ変換してからHVCへ渡す。DisplayのReady、Compositorの入力購読／表示所有権取得／最初のpresentと、実画面のもちもちくん・スピナーを確認した（`actual-arm-scanout.png`）。これは物理GPU・タッチの実機確認ではない。

Data上のアカウントDB読み取りで発生していた一時エラーは、読み取り後のatime更新に必要なUTCが未初期化だったためである。参照frontendは実PL031を読み取り専用でEL2に保持し、存在を確認したときだけ汎用の時刻機能を宣言する。mnuはボード固有のレジスタを知らず、その機能を使って既存のUTC時計を初期化する。時刻やファイル読み取り成功を偽装せず、実RTC接続後にUser／Keychain／Workspace／Binderの起動を確認した（`run-rtc-binder-ready.log`）。

最初のBinder画面は資産不足で壁紙とアイコンが欠けていた。既存のSDKアプリ梱包処理へARM ELFの検証を追加し、宣言された資産・マニフェストを配置した。製品のSystem/Data skeletonを使った次の起動では、SecureUIが壁紙・文字・認証フォームを表示した（`actual-arm-login.png`）。開発用ログイン設定はホストのumaskに依存せず0644で生成し、権限チェックを維持する。通常の自動ログインとセッション開始まで実行した。

その後の停止点は、Binderが壁紙を同期読み込みする間に最初の画面を提示できず、Ready期限を超える問題だった。期限を延ばさず、[背景の読み込み](applications/binder/src/platform/mochios/background.rs)を最初の画面提示後に開始する単一ワーカーへ移した。既存のViewKit `UiSubscription`で結果をUIスレッドへ返し、CompositorオブジェクトはUIスレッドだけが保持する。再読み込みは一件まで集約し、古い設定の結果を反映しない。実ソースを使ったホストの競合確認と既存dispatchの計5件が通った。

`out/arm/mdriver/native-services-diagnostics/run-binder-background.log`で、通常ログインからBinder PID 21のReadyまで確認した。実QMP画像`actual-arm-binder-background.png`では、壁紙・上部バー・時計・Dock・Terminalの実アイコンを確認した。最初のフレームでは上部バーの出現アニメーションと壁紙読込が未完了なので、その画像だけを完成判定に使わない。この参照イメージに含めたアプリはBinderとTerminalであり、標準アプリ全体のARM実行確認ではない。入力デバイスは未接続のため、タッチ・マウス操作やアプリ起動操作も未確認である。mboot-agentの製品ライフサイクル制御、Developerのtrust DB、通常MDP経路のARM製品イメージ生成も未完了である。

x86専用のPCIポート探索とサービス用リンカスクリプトをCPU別の入口で選択する。mnuへユーザー空間のサービス名やパスは追加していない。Binder、Terminal、mshを含む21本のARM版をビルド済みだが、ビルド成功を実行・GUI確認に数えない。初期サービスは既存の共通ローダーでRELRO／TLS等を準備し、作成したProcess Handleに結び付けて実出力を照合してから再開する。4 KiBを超えるローダーメタデータは既存の読み取り専用マッピングへ移す。起動引数・メタデータ・信頼済みbrokerの関連39件に加え、停止プロセス起動・失敗時の所有権・ロールの検証43件とプロトコル20件が通った。

ローダー／DTB生成を含むホスト168件、ARM Linux Imageのリンク、共有バックエンドのx86オブジェクトビルド、Linuxパッチのfuzz=0再適用と全15ファイルの一致を確認した。これはTCGによる機能確認で、性能測定や実機起動の証拠ではない。実機のEL2・GICH／GICV・実RAM map・SMMUの確認は、指示どおりADB不要の作業から分離して保留する。

## 着手済みの範囲

- [GICv2のfirmware binding](mboot/src/fdt/gic.rs)を追加した。rootの`interrupt-parent`とphandleから主controllerを解決し、祖先busの`ranges`とcell幅を検証して、Distributor・CPU interface・GICH・GICVの物理範囲へ変換する。重複phandle、無効な祖先、重複／範囲外／overflow、未対応の割り込み変換や非banked構成を拒否する。[GIC bindingの定義](https://github.com/torvalds/linux/blob/v6.6/Documentation/devicetree/bindings/interrupt-controller/arm%2Cgic.yaml)に沿って、宣言のない仮想化bankを生成しない。取得済みの実機DTBでは物理側の2領域だけを保持し、GICH／GICVは未確認のまま扱う。既存の仮想GIC診断からQEMU固有のMMIOアドレスを除き、実DTBの資源でA53／A57のIRQ配送・active状態保持・EOIと実System ELF Readyを確認した（`out/arm/gic-binding-final-a53/run.log`、`gic-binding-final-a57/run.log`）。ホスト164件とx86 UEFIプログラムの型検証も通った。記述の解決だけではMMIO所有権・物理IRQ配送・実機対応を与えない。
- [起動RAMの所有元](mboot/src/pages.rs)を追加し、firmwareの予約計画を実際の確保へ接続した。信頼済みの起動元だけが排他的にRAMを取得し、複製できない`Pages`でゼロ初期化した領域を保持する。物理アドレスとCPU側のアクセス先を区別し、共通の`Domain<Pages>`と[ARM専用ページ表](mboot/src/arch/aarch64/memory.rs)へ別々に渡す。Dropだけでは回収せず、全vCPU・DMA・walkerの停止と参照解除・無効化後に明示的に返す。回収失敗時は所有ハンドルを保持する。ホスト160件と製品ライブラリを使ったQEMU A53／A57で、他DomainのRAM・EL2専用ページ表へのアクセス拒否、確保途中の失敗、ページ表のゼロ再初期化、切り離し後の回収を確認した（`out/arm/owned-pages-final-a53/run.log`、`owned-pages-final-a57/run.log`）。実System ELFもこのRAMからReadyまで到達した。`max`ではページ表と実ELF入口を確認したが、未対応の拡張レジスタ群があるためnative vCPU経路は引き続き拒否する。製品frontendへの接続とDMA隔離は未完了。
- [firmware RAMの解釈と予約計画](mboot/src/fdt/memory.rs)をmBootへ追加した。64-bitのRAM範囲、DTB予約表、固定carveout、`size`／`alignment`／`alloc-ranges`による動的予約を検証する。起動元が本体・スタック・DTB等を除外した後、宣言順のfirst fitで配置を計画し、未解決の予約がある間は空きページを返さない。`no-map`と`reusable`を保持し、再利用可能なプールも所有者の回収実装ができるまでは通常RAMから除外する。範囲外・overflow・RAM重複・予約失敗を拒否し、失敗時に元の計画を変更しない。157件のホスト試験が通り、QEMU A53の実DTBでも本体とDTBを空きページへ含めないことを確認した（`out/arm/firmware-memory-native-a53/run.log`）。これは物理RAMの所有権取得やDTB書き換えを行うAPIではなく、製品frontendの確保処理への接続は引き続き必要。
- **ARM64版mBootライブラリの実ビルドが通った。** [Domain](mboot/src/domain.rs)は保持するメモリ所有型を汎用化し、Grant・Event Channel・マニフェスト・ELF検証を共有する。VMX/SVMのAPIはx86側に維持し、x86専用のPCI／APIC／IOMMUはARMへ混入させない。ARM版のPCI・SMMUやUEFI起動プログラムはまだ未実装で、ライブラリ生成を製品VMMの起動完了とはしない。既存x86 UEFIプログラムの型検証と145件のホスト試験も通った。`check-stage2.py --production-library`で製品クレートをリンクし、Cortex-A57の継続・仮想GIC・製品System Domain Readyが通った（`out/arm/linked-mboot-native-final/run.log`）。
- [Device Treeリーダー](mboot/src/fdt.rs)は割り当てやMMIOアクセスを行わず、借用したbytesの範囲・ブロック重複・構造・深さ・予約領域の加算overflowを検証する。[ARM topology](mboot/src/fdt/arm64.rs)はCPUの`reg`から64ビットaffinityを保持し、PSCI 0.2／1.0のSMC／HVCを区別する。重複CPU・不正なcell数／予約ビット・起動CPUの欠落・無効CPU・重複propertyを拒否する。QEMU実DTBと不正入力を含む153件が通った。標準AArch64 Image形式の入口でも実際に渡されたDTBと物理MPIDRを照合し、製品ライブラリからSMC bindingを解決した（`out/arm/native-boot-dtb-final-a53/run.log`）。この解釈だけでfirmware呼び出し権限やSMP対応を付与せず、GIC・UART・RAMのbindingと資源確保への接続は引き続き必要。
- [firmwareのSMC経路](mboot/src/arch/aarch64/firmware.rs)は、信頼済みの起動元が指定したconduitを物理CPUへ固定し、PSCI versionとWORKAROUND_1／3だけを扱う。QEMUで実PSCI 1.1と緩和策の未対応応答`-1`を確認し、そのまま保持・伝播する。任意のguest functionやポインターをfirmwareへ転送しない。製品Domainディスパッチャへの接続は未完了。
- 実機と同じCortex-A53のQEMUモデルでも、製品mBootライブラリのStage-2・vCPU再開・仮想GIC・実System Domain ELFの上位EL1 Readyを確認した（`out/arm/native-boot-dtb-final-a53/run.log`）。40-bit PAのCPUへ44-bit geometryを要求せず、未対応範囲の拒否も確認する。CPU admissionはMIDRのimplementer／architecture／partを照合し、既知の非影響Cortex-A53に不要なCSV3・WORKAROUND_1／3を要求しない。他CPUの保守的な判定は維持し、不明なfirmware応答を成功に変換しない。CPU基本処理と判定のログは`out/arm/cortex-a53-cpu-check/aarch64-run.log`。ARM版mnu・System Domain ELFの再ビルド、A57／max／x86のCPU診断、MDPによる既存x86カーネルの再ビルドも通った。製品mnu初期化全体や実機の起動完了を意味しない。

- [ARM vCPU](mboot/src/arch/aarch64/vcpu.rs)にEL2からの繰り返し実行・再開を追加した。GPR・全32本のSIMD・FP制御・TLS・EL1翻訳／例外状態・仮想タイマーを保持し、物理CPUとStage-2のroot／VMIDへ継続を固定する。別アドレス空間・非activeルート・HVCやMRSの二重完了を拒否する。Cortex-A57で2つの実ゲストを交互に再開し、製品System Domain ELFでも実MRSトラップから再開してReadyへ到達した。現時点では拡張レジスタバンクを持つCPUを保守的に拒否し、QEMU maxは従来の一度限りの入口診断だけを実行する。Domain間の投機実行対策と製品ディスパッチャへの接続は未完了であり、この機構単独で安全な製品VMMの完成とはしない。
- [GICv2仮想化](mboot/src/arch/aarch64/vgic.rs)にvCPU固有のVMCR・APR・全LR・HCR状態を追加した。GICHはEL2だけが保持し、ゲストには割り当て済みGICVのみを公開する。ソフトウェア起源のcoalesced edgeを注入し、満杯や競合SGI送信元は保持元へエラーを返す。HW LRによる物理IRQのdeactivation権限はまだ公開しない。実GICH/GICVでactiveなIRQの途中切替、優先度分離、再開後EOI、SGI送信元付きtoken、キュー満杯と物理GICへのStage-2拒否を確認した。ログは`out/arm/native-vgic-final-check/run.log`。物理IRQ・仮想タイマーIRQ・実Domain資源との配送接続は引き続き必要。

- [ARM System Domainの起動入口](boot/src/arch/aarch64/entry.S)と[専用リンク配置](boot/domain-aarch64.ld)を追加した。実ELFの低位入口で4 KiBのStage-1を構築し、カーネル・スタック・起動情報をTTBR1の上位領域へ保持する。textはRO/X、rodataはRO/NX、その他のRAMはRW/NXとし、入口の上位別名もRO/NXにする。現在の初期RAM上限はDomainあたり1 GiB。製品ELFを実際のmBootローダーとStage-2で実行し、Cortex-A57とmaxで上位EL1のReady HVCまで到達した。ログは`out/arm/system-domain-stage2-check/run.log`と`out/arm/system-domain-stage2-a57-check/run.log`。診断はReadyで停止し、mnu初期化・サービス起動を通ったことにはしない。x86 System Domainのリンクと既存mBootの145試験も通った。

- [System DomainのGrant RAMアクセス](boot/src/mdriver.rs)でGPAとカーネルVAを分けた。Hypercallと描画面の共有情報にはGPAを渡し、CPUからリング・ストレージ・metadata・描画バッファへアクセスするときだけ、検証済みGrant範囲とカーネルlinear mapを使う。範囲外・加算overflow・不正な仮想アドレスを拒否する。非identityの実バッファでコピーと読み取り専用Grantを確認し、既存の起動データ5試験とx86 System Domainの実リンクが通った。ARMの実I/O確認はmBoot／mDriver結合後に行う。
- **ARM64版mnuライブラリ全体の型チェックと実コード生成が通った。** `aarch64-unknown-none`でコンパイルし、従来の部分的なCPU診断だけでなく、共有VM・syscall・IPC・Capability・スケジューラを含む実ライブラリを生成した。ログは`out/arm/native-mnu-aarch64-build.log`。後続変更でARMブートELFのリンクとReadyまでの入口実行も通ったが、OS起動は未完了。既存x86カーネルも再ビルドできた。
- [ネイティブCPU起動の分離](core/src/smp.rs)により、APIC／CR3／16-bit trampolineをx86側へ移した。ARMは現在1vCPUだけを受け付け、実際に走っているCPUのみをTLB同期の参加者へ登録する。複数vCPUの起動とリモートIPIは未対応として拒否する。診断では1CPUの受け入れ、未完成SMP・x86ハンドオフの拒否を確認した。ARM SMPを完成扱いにはしない。
- [ARMタイマー](core/src/arch/aarch64/timer.rs)と[製品IRQハンドラ](core/src/arch/aarch64/interrupt.rs)を共有の時刻・sleep・スケジューラへ接続した。割り当て済みCPUインターフェースだけを上位のDevice／RW／XN領域へマップし、既存マッピングや非presentの所有済みPTEを上書きしない。タイマーは前回deadlineから再設定し、時刻は共有counter epochから算出してCPU数やIRQ遅延による増速を避ける。EOI完了後にスケジュールし、EL0フレーム・TLS・upcallを保持する。実タイマーIRQ、counterの端数・wrap・オーバーフロー、MMUの空き判定、x86カーネルビルドが通った。製品IRQハンドラ全体をOS上で実行する確認はmBoot結合後に行う。既存タイマー／CPU時間の回帰5件も、実フレームと現在の所有権表現で通った。
- [CPU初期化](core/src/arch/aarch64/cpu.rs)をx86から分離し、EL1のFP/AdvSIMD、CPUごとのPAN/RNDR検出、入れ子のユーザーデータアクセス範囲を追加した。PANを開く間はIRQとCPU移動を制限し、PXNは解除しない。Cortex-A53・A57と`max`で保護状態の復元・未対応RNDRの拒否を確認した。既知の非影響Cortex-A53以外はCSV3とEL2経由SMCCCのWORKAROUND_1／3を要求し、未対応を成功扱いにしない。**製品ディスパッチャのfirmware経路への接続と、CPU admission全体の実行確認は未完了。** ARMの早期乱数は有効なfirmware seedまたは健全なRNDRを必須とし、RTC・時刻・固定値を代替エントロピーにしない。
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

初期対象はAArch64（ARM64、little endian）とし、ARM32は含めない。実機は**NEC PC-TE510HAW、Snapdragon 625／MSM8953系、Cortex-A53・8コア**を対象とする。[NEC公式仕様](https://lenovo-nec.jp/navigate/products/pc/183q/10/lavietab/lte/spec/index02.html)ではSoCはAPQ8053、RAMは3 GB、画面は1920×1200。ブートローダーは解除済み。提供された`~/Downloads/boot.img`を読み取り、以下の構成を確認した。

[SoC共通のLinux Device Tree](https://github.com/torvalds/linux/blob/master/arch/arm64/boot/dts/qcom/msm8953.dtsi)にはCortex-A53、PSCI SMC、`qcom,msm-qgic2`のGICv2、Qualcomm固有IOMMUが記述されている。CPU affinityは`0..3`と`0x100..0x103`で、クラスタ情報を切り捨てない。これはSoC側の参考情報であり、NECの実DTB・EL2引き渡し・GICH/GICV利用可否・secure firmwareによるIOMMU所有を確認したものではない。GICv3／SMMUv3やUEFIの存在を前提にしない。

Cortex-A53のMeltdown非影響判定は[LinuxのKPTI判定](https://github.com/torvalds/linux/blob/v6.6/arch/arm64/kernel/cpufeature.c)、Spectre-v2は[Armの資料](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Security%20update%2018%20June%2018/Arm_Trusted_Firmware_Speculative_Vulnerabilties_v1.6.pdf)、Spectre-BHBは[LinuxのCPU判定](https://github.com/torvalds/linux/blob/master/arch/arm64/kernel/proton-pack.c)を確認した。これらの非影響判定を、他のCPUや他の攻撃にも一律に適用しない。

### 提供されたAndroid起動イメージ

入力は23,526,696 bytes、SHA-256は`9708d3ed2dc87eccad009e51316379fe95d433ebf4d2118dfe1c3997bb9d7fba`。元ファイルは変更していない。展開物と調査結果は`out/arm/pc-te510haw-stock/`に置き、配布物やソースへ取り込まない。

| 項目 | ファイルから確認できた内容 |
|---|---|
| Android形式 | boot header v0、page size 2048、Android 7.1.1／patch level 2017-08 |
| kernel payload | gzipのAArch64 Imageと、直後に連結された55個のDTB。kernel payload全体は21,026,587 bytes |
| kernel | Linux 3.18.31-perf、AArch64／4 KiBページ／39-bit VA。展開したファイルは22,303,232 bytes |
| Image header | `text_offset=0x80000`、`image_size=0x17a8000`、`flags=0` |
| Android側の配置情報 | `kernel_addr=0x80008000`、`ramdisk_addr=0x81000000`、`tags_addr=0x80000100`。Image headerのtext offsetと同一の情報ではないため、実ブートローダーの配置処理を確認する |
| ramdisk | gzip／CPIO、圧縮2,495,857 bytes、展開5,565,184 bytes。Androidの起動用でありmochiOSのinitfsとして再利用しない |
| DTBの選択 | MSM8953・APQ8053に加えて別SoC／評価ボードも含む。APQ8053 MTPは`dtb-048.dtb`だが、実機で選択された証拠はまだない |

実際の55個すべてを製品[mBoot FDTリーダー](mboot/src/fdt.rs)と[CPU／PSCI解釈](mboot/src/fdt/arm64.rs)へ渡し、構造・CPU affinity・conduitの解決が通った（`out/arm/pc-te510haw-stock/mboot-dtb-check.log`）。単にマジックを見つけた件数ではなく、各DTBの全構造を検証した結果である。

APQ8053／MSM8953候補には、8 CPUのaffinity `0..3, 0x100..0x103`、PSCI 1.0 SMC、GICv2 `qcom,msm-qgic2`がある。GICの`reg`はdistributorとCPU interfaceの2組だけで、GICH／GICV利用可否は確定できない。Qualcomm SMMU v2／MMU-500とsecure IDを持つcontextもあるが、EL2からそれらを所有・設定できることを意味しない。

候補DTBの`/memory/reg`は全cellがゼロであり、ブートローダーによる実行時の補正が必要。固定の予約メモリに加え、`size`／`alignment`で後から配置されるsecure・GPU等の領域もある。公称3 GBをそのまま空きRAMとして扱わず、実行時DTBと予約領域を取得する。

stock kernelは`CONFIG_VIRTUALIZATION`が無効。これはLinuxにKVMを組み込んでいないことを示すだけで、EL1／EL2のどちらで入ったかや、別firmwareがEL2を保持しているかは証明しない。

ADBで起動中のLineageOSから実行時DTBを取得した。Androidの報告名はLenovo TB-X704F、DTBのモデルはMSM8953 + PMI8950 QRD SKU3である。RAMは計3 GiBで、DTBに固定・動的な予約領域がある。`/soc/timer` のアクティブLow PPIと19.2 MHzの宣言は、[共通のタイマー解決](mboot/src/fdt/timer.rs)で扱う。起動時はその周波数をCPUの`CNTFRQ_EL0`と照合する。GICH／GICVは実行時DTBにも記載がなく、利用可否は未確認である。取得物はローカルの`out/arm/pc-te510haw-live/`に保持する。

2026-11-21に、この実機のLineageOSイメージへカーネル入口で`CurrentEL`を読む計測コードを追加し、`fastboot boot`で一時起動した。Androidの起動完了後に`ro.boot.mochios_el=1`を確認した。この起動経路の引き渡しはEL1で、現行mBootのStage-2初期化にはEL2が必要である。TB-X704FでEL2を渡せる公開の起動経路は確認できていない。実機ログはローカルの`out/arm/pc-te510haw-probe/lineage-boot-run-20261121.log`に保持する。

[EL1直接起動の入口](boot/src/arch/aarch64/native.rs)を開発ビルド `mochios-native` として追加した。AArch64 Imageのブート規約で確保された範囲をDevice TreeのRAM・予約領域と照合し、既存のStage-1初期化とmnuの通常の入口に接続する。EL2を無効化したQEMU Cortex-A53で、ファームウェアから新しい乱数シードを受け取るとmnuのカーネルヒープが初期化され、シードがなければ停止することを確認した。実機DTBを使ったQEMU試験でも、試験用シードをその場で渡した場合は同じ地点まで進んだ。既存のEL2経路でもSystem DomainのReadyを再確認した。各ログはローカルの`out/arm/native-entry/`に保持する。

EL1側は、Device Treeから物理GICv2と仮想カウンターのPPIを解決し、タイマー割り込みをmnuのスケジューラへ届ける。EL2が供給するイベントや管理IRQはEL2経路で扱う。`MOCHIOS_ARM64_BOOT_MODULE`で既存の起動モジュールを読み取り専用で組み込める。EL2を無効化したQEMU Cortex-A53で、タイマーの進行、ARM64の`core.service`のPID 2での起動、EL0からのSVCを確認した。`core.service`はSystem上のLoggerのマニフェスト取得で止まった。QEMUの検証ログはローカルの`out/arm/native-timer/`に保持する。

取得した実機DTBには`/chosen/rng-seed`がなく、MSM8953の`qcom,msm-rng`には`iface_clk`が指定されている。[開発用Fastbootイメージの生成](boot/scripts/pack-android-v0-dev.py)では、元のAndroid v0イメージのDTBに一時起動ごとにホストOSで生成した乱数を渡す。実機でこの引き渡しを検証し、通常起動用には端末内の乱数源を接続する。

実機の最初の確認は、Fastbootでの一時起動とGICv2／PPI 20の割り込み配送、EL0に入ったことを観測できる状態までを含む。通常のOS起動には、System／Dataを供給するストレージが必要である。実行時DTBには`qcom,sdhci-msm`が2系統、表示にQualcomm MDSS／DSI、タッチにGoodix GT9xxが記述されている。各機能を物理デバイスで検証する。

開発用EL1イメージは`MOCHIOS_ARM64_BOOT_DISK`に渡した単一のGPTディスクからSystem／Dataを供給できる。既存のストレージAPIを通し、検証したDataの範囲内だけを書き込み可能にする。ディスクは起動中のRAMにあり、Dataへの変更はその起動中に限って保持される。QEMUでは実PL031からUTCを初期化し、既存のARM64 System／DataでFilesystemとPOSIXのReadyを確認した。検証中の80件の書き込みはすべてData内だった。ログは`out/arm/native-disk/`に保持する。TB-X704FのeMMC接続とFastbootでの実機検証は引き続き必要である。

[EL1のフレームバッファ引き継ぎ](boot/src/arch/aarch64/boot_framebuffer.rs)は、ファームウェアが`/chosen`で明示したXRGB8888の`simple-framebuffer`を検証し、既存の表示ドライバーへ渡す。QEMUの`ramfb`を使い、`display.driver`とコンポジタの初回描画まで確認した。同じ起動では`secure-ui.service`のプロセス作成が`NotFound`を返し、Binderには到達していない。ログは`out/arm/native-display/`に保持する。

TB-X704Fの実行時DTBに`simple-framebuffer`と`qcom,cont-splash-enabled`はない。予約された`splash_region`だけでは、現在パネルへ送信されている画素の位置や形式は分からない。実機でこの表示経路を使うには、選択されたNT51021BのDSIパネルを初期化し、確保した描画用メモリをMDSSでスキャンアウトするためのボード対応が必要になる。

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
| mnuのビルド・入口 | 実ライブラリ全体のARM型チェック・コード生成と既存x86カーネルビルドが通った。ARM System Domainの実ELFが上位EL1のReadyまで実行された | Ready後のmnu初期化、サービス・ユーザープロセスの実行 |
| CPU・割り込み | CPU初期化・保護設定、割り当て済みGIC・タイマーIRQからのスケジューラ接続を実装。ARMは1vCPUのみ受け付ける | firmware mitigation供給、実vCPU上のプリエンプション・例外復帰、SMPとリモートIPI |
| メモリ | Stage-1 MMUと共有VMアダプタ、TTBR0／TTBR1制御、ARMの上位ヒープ配置を実装した。GDT/TSSとCR4初期化はx86側だけで行う | ARMの初期配置・マッピング、SMPでのTLB整合 |
| タスク | CPU別のEL1継続、EL0復帰、初期レジスタ・upcall・FP・TLSを実装した。実mnuから最初のcore.serviceをEL0で実行した。複数ユーザープロセス間の結合実行は未確認 | 上位カーネル配置・例外スタックと実スケジューラを接続し、IPC・Handle転送を実行する |
| ELF | [mnuローダー](core/src/elf/loader.rs)はnative CPU種別を検査する。ユーザー側のARM再配置は実Clang/LLDのDSOで確認済み。[mBootローダー](mboot/src/image.rs)もCPUを検査し、ARMの上位VMAと物理ロード先を区別する | 通常サービス群の実行と共有オブジェクトの結合確認 |
| 起動ABI | BootInfo v4・DomainBootInfo v14にCPU種別、64ビットID、明示的なARM IRQ資源を定義した。既存のx86 SMPハンドオフはARMで拒否する | ARMの実vCPU起動・SMP状態と資源供給側の実装 |
| syscall・実行ABI | [ARMラッパー](user/crates/syscall/src/arch/)のSVC、カーネル側のSVC配送、native保存状態・継続ABIを実装した | OS上でのブロック・再開・例外復帰と、既存IPC・Capability処理の確認 |
| mBoot | ARMのStage-2とEL1からのHVC受け渡しをCPU診断で確認済み。ARMのvCPU再開とGICv2状態保持を実CPU命令で確認済み。ARMライブラリと共有Domain状態の生成も確認済み。UEFI起動・実行プログラムは引き続きx86向け | ネイティブ起動・実行経路への接続、CPU拡張バンク、実機IRQ、投機対策、DMA隔離 |
| System Domainの入口 | [system_domain.rs](boot/src/system_domain.rs)はIRQ資源を渡し、I/OのGPAもlinear map経由でアクセスする。ARMは低位入口でStage-1を作り、実ELFからmnu初期化とcore.serviceのEL0実行を確認済み | 画面・入力と通常デスクトップの起動 |
| mDriver | x86のPVHに加え、ARM64 Image／DTB／HVCでLinuxとPID 1を実行した。Grant／Event制御と実RAMブロックI/Oが通った | 物理ストレージ・表示・入力の接続、対象SoCの安全な割り当て |
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

Androidの起動形式とブートローダーからの引き渡しは、mBootのARM起動frontend（配置予定は`mboot/src/platform/android/`）で扱う。機種ごとに必要な起動設定は`mboot/config/boards/<機種>.toml`に置き、実行時DTBから取得できるアドレス・IRQは共通パーサーで解決する。画面、タッチ、ストレージ等のLinux設定やDTオーバーレイはmDriverの既存の`mboot/mdriver/board/`配下に置く。物理資源の割り当てとDMA隔離はmBootが検証し、mnuには既存の起動ABIで確定した資源を渡す。

mDriver側にLinuxドライバが存在しても、mBootのARM仮想化、LinuxのDomain起動、デバイス資源の割り当てまで自動的に成立するわけではない。特にSoC内蔵デバイスはPCI前提で扱わず、MMIO・IRQ・DMA・関連資源の所有関係を確認する。

## アーキテクチャ依存処理の分離

mnuでは、次の配置でCPUプリミティブとカーネル継続を分離した。`arch/mod.rs`がビルド時に実装を選び、共有処理へ同名のAPIを公開する。MMU・CPUローカル状態・例外配送・ユーザー復帰もARM側へ接続済みだが、製品OS上での結合実行は未完了。

```text
core/src/arch/
  mod.rs
  x86_64/
  aarch64/
```

分離対象は、初期入口、例外フレーム、割り込み制御、MMU・TLB、CPUコンテキスト、ユーザーモード遷移、TLS、タイマー、CPUローカル状態、SMP、必要なバリア・キャッシュ操作。

| 層 | 共通部分とCPU別部分 | 現在の限界 |
|---|---|---|
| mnu | IPC・Capability・スケジューラ・VMの共通処理を維持し、CPU命令とレジスタ配置を`core/src/arch/`から選択 | ARMは1 vCPU。参照構成でOSの結合起動を確認。SMPと製品起動経路は未完了 |
| mBootの機構 | `Domain<Memory>`、Grant・Event・scheduler・マニフェストを共有。ELFは`ImageMemory`、ARMのページ表は`TableMemory`を通じて所有元へアクセスする | ARMは型付きの専用vCPU／Stage-2／仮想GIC API。共通の製品ディスパッチャへの接続は未完了 |
| firmware／ボード | DTBのCPU・PSCI・RAM・予約領域・主GICv2をmBootで解釈。metadataの解析・配置計画は、物理資源の所有やデバイス権限を付与しない | 参照機でGIC／EL2タイマー所有を接続済み。実機資源の確認、UART等のbinding、製品frontend、SMMU・DMA隔離が必要 |
| System Domainの入口 | `boot/src/arch/aarch64/`とCPU別linker scriptで上位EL1へ移り、共通BootInfoをmnuへ渡す | 参照構成で通常サービス群、ログイン、Binderデスクトップの描画まで実行した。製品frontendとの接続は未完了 |
| デバイス接続 | カーネル側の`PlatformOps`へ、boot側がmDriverとの接続を供給する。デバイス探索や輸送の実装はカーネル外 | 実RAM block、firmware表示、HVC／Grant／Eventを確認。物理ストレージ・GPU・入力とDMA隔離は未完了 |
| ユーザー実行ABI | syscall命令、保存状態、TLS、ELF再配置をCPU別実装へ分離。上位のサービス・アプリの機能は共有 | Rust標準ライブラリを使う複数サービスとBinderの実EL0実行・IPC・ファイルI/Oを確認。一般アプリ全体の実行確認は未完了 |
| MDP／製品起動 | producerとイメージ組み立ての責務は既存構成を維持 | kernel／boot／mBootの製品recipeや共通toolchain定義にはx86指定が残る。ARMの診断ビルドと製品イメージ生成を区別する |

`mboot/src/main.rs`の製品frontendは現在もx86 UEFI・VMX／SVM向けである。ARMライブラリがビルドできることを、この起動プログラムがARM対応済みという意味にはしない。CPUの移植はarch層へ集約し、firmware起動・Domain資源割り当て・ユーザーABI・ビルド設定はそれぞれの層で接続する。

RAMの情報と所有権は次の順に扱う。`MemoryMap`は複製可能な配置情報であり、所有権を与えない。信頼済みの起動元が生存中の本体・スタック・DTB・イメージ・firmware／デバイス予約を除外した後、`PagePool::claim`で排他的な所有を宣言する。確保した`Pages`をDomain RAMとStage-2ページ表へ分ける。ページ表にはDomainへ公開しない独立した所有元を使う。このプールはCPU間で共有できず、今回の単一CPU起動経路に限定する。

回収は、すべての実行主体を停止してから、選択中の正確なroot／VMIDを`Mapper::deactivate`で切り離し、他CPU・DMA・IOMMUも含む無効化を完了したことを呼び出し元が保証して`Tables::retire`へ渡す。以後はmapperとして使えない`RetiredTables`から回収する。失敗したハンドルを失わず、Drop時も未回収ページを再配布しない。CPUのStage-2保護をDMA隔離の代わりとはしない。

スケジューリング方針、Handle表、IPCの権限判定、Capability、VM objectの所有権管理などは共有する。選択はアーキテクチャ層の入口で集約し、各サービスやUIへ`target_arch`判定を広げない。全体の大規模な書き換えや不要な動的ディスパッチは行わない。

ARM側では次を実装・確認する。

- mnuのEL1例外ベクタと、EL0からのsyscall・フォールトの処理、検証済み状態への復帰。
- 汎用レジスタ、SP、復帰先・状態レジスタ、FP/SIMD、TLSの保存・復元。
- Stage-1ページテーブル、アクセス属性、実行禁止、ASID、TLB無効化、命令キャッシュとの整合。
- CPUローカル状態、仮想タイマー、プリエンプション、割り込みの配送。
- ARMのメモリ順序を踏まえたロック、共有キュー、Grant、MMIO・DMAの同期。

初期実装のページ粒度は4 KiB。起動時にもCPUの対応granuleを検査し、共有メモリ・アライメントの前提を検証する。x86のPTEビットやメモリ順序をARMへそのまま移さない。

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

初期の参照環境はQEMU ARM64 `virt-8.2`・GICv2・Cortex-A53とする。Cortex-A57と`max`も対応範囲の診断に使用する。実機固有の周辺回路を再現するボードではないため、機種別対応とは分けて扱う。[QEMUの公式仕様](https://www.qemu.org/docs/master/system/arm/virt.html)で、AArch64 CPUの明示指定と、mBootに必要な仮想化拡張の有効化を確認する。仮想化拡張は既定では無効。

最初はCPU 1個・シリアル出力を使い、参照するCPUモデル、GIC、RAM、firmware、QEMU版を固定する。ARM64ホスト上でmBoot自体をKVMゲストとして検証する場合は、ネスト仮想化とゲストEL2の利用条件を確認する。ARM64ホストであることだけを根拠に、この構成を利用可能とは扱わない。x86_64ホストでARM64をTCG実行した結果を実機性能の指標にはしない。既存のx86_64 KVM起動設定は変更しない。

対象タブレットの元boot.img／DTBと実際の起動ログで、次を調べる。

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

共有VM、CPUローカル状態、継続復帰、起動契約、ARM SDKまで段階的に実装・検証している。mnuの初期化とcore.serviceのEL0実行、Linux Hardware Domainと制御経路の接続を確認済み。OS全体の起動は未完了である。Filesystem／POSIXのReadyまで到達し、通常サービスの認証・表示・入力・製品イメージへ進む。診断の成功をARM版OS全体の起動完了と扱わず、空実装でビルド成功を装わない。
