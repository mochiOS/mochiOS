# ARM64対応方針

更新日: 2026-11-15

mochiOSをARMタブレットで動かすための移植計画と進捗。以下の実装状況は現行ソースを確認した結果であり、未着手の構成・作業項目は提案である。現在はCPUプリミティブ、EL1継続、例外入口、EL0遷移、Stage-1 MMUを実装し、共有VMへの接続を進める段階で、ARM64版mnu全体の起動には至っていない。

## 着手済みの範囲

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
- CランタイムへARMのsetjmp保存領域（224 bytes）、jumpアセンブリ、整列したpthread入口、TPIDR_EL0参照、TLS descriptor resolverを追加した。ここはクロスビルド・Cヘッダー配置確認までで、OS上のsignal／longjmp動作確認はまだ行っていない。TLS本体のVariant I配置、ARM動的再配置、crt0、SDKの生成・梱包・配布対応は未完了である。
- x86_64のkernel producerを新しいMDP経路でビルドした。OS全体の起動確認は別途必要。

診断は[check-cpu-primitives.py](core/scripts/check-cpu-primitives.py)から実行する。実装本体を直接読み込み、架空のIRQ状態や保存・復元の代替実装で置き換えない。QEMU 8.2以降のARM/x86バイナリ、OVMFの4 MiB版、指定Rustツールチェーンの`aarch64-unknown-none`と`x86_64-unknown-uefi`ターゲットが必要。

```sh
python3 core/scripts/check-cpu-primitives.py --toolchain nightly-2026-05-14
```

ARM診断の参照機は`virt-8.2`、`cortex-a57`、GICv2、CPU 1個、RAM 128 MiB。EL1の例外入口は製品側の実装を使い、UART・割り込みコントローラ設定と診断用の例外ディスパッチだけをfixtureに置く。mnuの起動・例外処理方針との統合やmBootの移植完了とは扱わない。診断は命令動作を確認するTCG実行であり、既存OSのKVM起動設定を変更しない。成果物とログは既定で`core/out/mdp/cpu-primitives/`に保存する。`--out`、`--qemu-aarch64`、`--qemu-x86-64`で検証環境を指定できる。

ARM64のコンテキストは、ベースの[AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst)とFP/SIMDを対象にする。SVE/SMEを有効にしたビルドは拒否し、起動側でもそれらを有効にしない前提。FP/SIMDアクセスの製品側初期化、タスクごとのTLS切替、アドレス空間切替、SMPでの受け渡しは、この継続切替だけで完成したとは扱わない。既存のユーザーupcall・syscallレジスタABIにはx86依存が残る。

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

**今回の追加指示により、ARM向けSDK・libc・devkitのビルド対応とリリースも対象に含める。** 公開の許可は今回のARM対応に限る。ARM64のサービス・Binder・アプリを実際にビルドするためのランタイム・Rust標準ライブラリ・ターゲット情報も揃える。SDKの公開はまだ行っていない。

既存のdevkitリリース`sdk-27.0-dp.1-shared.10`の本文を確認済み。変更履歴は同じ書式（太字のprerelease注意文、`mochiOS SDK ...`の見出し、変更の箇条書き、末尾の太字`Full Changelog`と比較リンク）に従う。ターゲット名を変えただけのアーカイブや、実行確認をしていない機能を動作済みとして公開しない。

## 現在の実装

ARM用のターゲットを追加するだけでは起動できない。CPU依存処理は複数の層に存在する。

| 対象 | 確認した現状 | 主な移植箇所 |
|---|---|---|
| mnuのビルド・入口 | [Cargo.toml](core/Cargo.toml)の`x86_64`依存をCPU条件付きに分離。[ターゲット定義](core/x86_64-mochios.json)はx86_64。[入口](core/src/entry.rs)のSP参照はarch層へ移したが、`sysv64`とx86シリアル出力は残る | AArch64用のカーネル配置、呼び出し規約、初期スタック、初期ログ |
| CPU・割り込み | IRQマスクとARMのベクタ・保存フレームはarch層。製品側の[cpu.rs](core/src/cpu.rs)、[interrupt](core/src/interrupt/)、[percpu.rs](core/src/percpu.rs)、[smp.rs](core/src/smp.rs)にはx86依存が残る | ARM例外のカーネル配送との接続、CPUローカル状態、タイマー、二次CPU |
| メモリ | [mem](core/src/mem/)にx86のページテーブル、GDT/TSS、TLB制御がある | Stage-1 MMU、ページ属性、アドレス空間切替、TLB・キャッシュ制御 |
| タスク | `arch::context`がCPUごとの保存フレームと切替を選択。ARMの初回EL0遷移も追加済み。[task/context.rs](core/src/task/context.rs)と[usermode.rs](core/src/task/usermode.rs)の製品経路にはx86依存が残る | upcall・保持ユーザー状態、タスクと例外の接続、TLS、アドレス空間切替 |
| ELF | [loader.rs](core/src/elf/loader.rs)は`EM_X86_64`だけを許可 | 実行中のアーキテクチャに対応したELF検証とロード |
| 起動ABI | [BootInfo / SmpHandoff](core/crates/abi/src/boot.rs)にAPIC IDや`kernel_cr3`がある | 共通情報とアーキテクチャ固有情報の分離 |
| syscall・実行ABI | [syscallラッパー](user/crates/syscall/src/lib.rs)は`syscall`命令とx86レジスタを直接使用 | ARM64の入口・復帰・引数受け渡しと、実行コンテキストの表現 |
| mBoot | [arch](mboot/src/arch/)はx86_64のみ。[仮想化バックエンド](mboot/src/lib.rs)はIntel VMXとAMD SVMを実装 | EL2、Stage-2変換、vCPU、割り込み仮想化、ハイパーコール |
| System Domainの入口 | [system_domain.rs](boot/src/system_domain.rs)、[domain_entry.rs](boot/src/domain_entry.rs)が`sysv64`と既存バックエンドに依存 | ARM64のDomain起動・引き渡し |
| mDriver | [現行仕様](docs/mboot/mdriver.md)はLinuxのPVH起動、PCIデバイス割り当て、`vmcall` / `vmmcall`を使用 | ARM64 Linuxの起動、Domain接続、対象SoCのデバイス割り当て |
| OSビルド | [workspace設定](mdp.workspace.toml)、[producerのtoolchain設定](tools/devkit/build-support/toolchain.mk)などにx86_64向けの指定がある | 内部ビルド・成果物・配置のアーキテクチャ選択 |

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

現在は共有VMとARMの型・ページングを接続している。続いてCPU初期化・CPUローカル状態・スレッド復帰、起動ABI、mBoot EL2、ARM用ランタイム・SDK、サービスとGUIを実際につなぐ。診断の成功をARM版OS全体の起動完了と扱わず、空実装でビルド成功を装わない。
