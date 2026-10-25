# 現在スレッドのlocal-base設定（段階227）

compiler TLSをユーザー空間で初期化するため、native ThreadLocalBaseSet（704）を
追加しました。既存ThreadCreateContext.thread_local_baseは新規スレッド用であり、
ThreadContextRestoreはTLSを変更しない契約です。新APIは起動済みの呼び出し元だけの
CPU local-base registerと保存コンテキストを設定します。ターゲットPID / Handleは
受け付けません。

引数はbaseとreserved zeroです。baseは0またはcanonical user addressです。
レジスタの値として検証し、指すメモリを読んだり、mappingの作成・アクセス権の付与は
行いません。TCB、ELF、TLS template、POSIX、サービス名、資格情報の解釈はありません。
失敗はnative status::Errorです。reserved不正、非canonical / kernel address、
呼び出し元の不在を区別し、失敗時に保存値やlive registerを変更しません。

interruptを抑止した区間で、現在スレッドの保存値とCPUのFS baseを一緒に更新します。
保存値だけ更新してcontext switchで旧CPU値に巻き戻ることを防ぎます。
CPUへの書き込みは既存cpu::write_fs_baseを使います。このhelperはCPU対応に応じて
WRFSBASE / IA32_FS_BASE MSRを選択します。user側のset_base wrapperはnative errorを
維持し、未知の非ゼロ成功値をBackingFailureとして拒否します。

ホストでは6 target / 12件が成功しました。実operationを使った保存値・live値の同時
更新、拒否時の無変更、reserved、canonical address境界、native error伝播を確認しました。
既存のthread creation、TLS selector cache、CPU / RAM観測の検証も含みます。
記録はout/checks/mochios-thread-base-227-host-final.logです。

KVMではmain threadのbaseを一時的なprivate blockへ変更し、16回のyieldとnative
identity syscallを通してbaseとmarkerが維持されることを確認しました。不正なbase /
reservedの拒否、base 0でのnative syscall、元のbaseへの復帰も成功しています。
従来の実RAM / Handle、passwd、rusage、C++ thread / exception / filesystem、auxiliary、
memory adviceの試験も成功しました。mochios-thread-base-227-native.logに保存しています。

これはcompiler TLSの配置まで完了したという意味ではありません。PT_TLS segment、
メインスレッドとpthreadのtemplate初期化・独立領域・解放は引き続き実装が必要です。
また、現在のmap_elf_imageはAT_PHDR用にload_base + e_phoffを返すだけです。
実際のC ELFでは最初のLOADがoffset 0x1000 / vaddr 0x400000のため、その計算結果
0x3ff040はLOAD範囲にありません。TLS templateをprogram headerから安全に読む前に、
汎用ELF metadataを実際に読める配置へ直す必要があります。
完全なPOSIX環境とclang / ld.lldの実行は未完了です。


診断用ELFを削除して通常managerを再ビルドし、新カーネルのイメージを再生成しました。
KVMでinput READY、account endpoint handoff、Binder ready、デスクトップとDockを
確認し、mochios-thread-base-227-normal.log / normal.pngへ保存しました。VMは停止しています。
配布SDKとRust std overlayは再生成していません。NICのないrunnerのnetwork Failed(-6)と
USB driver errno 22は従来どおり残ります。
