# 静的ELFのコンパイラTLS

`newlib-runtime`がELFの`PT_TLS`を読み、初期スレッドとpthreadごとに
独立したTLS領域を確保する。カーネルへELF TLSの配置規則やPOSIXの
スレッド意味論を追加しない。

## 配置と所有権

- `posix-client::static_tls`はELF64のプログラムヘッダーとx86-64 Variant IIの
  配置を扱う。ヘッダーのstride、重複したTLSセグメント、整数overflow、
  alignment、初期化データを含むPT_LOADとのファイルoffsetの対応を検証する。
- `AT_PHDR`、`AT_PHENT`、`AT_PHNUM`をfaultable copyで読み出す。
  TLSがないELFには空のTCBを用意する。不正なメタデータを空のTLSとして扱わない。
- static PIEでは元PHDRの仮想アドレスにユーザー空間専用の`AT_IMAGE_LOAD_BIAS`を
  加え、初期値のロード先を求める。`AT_BASE`はinterpreter用の意味を維持する。
  キーが存在しない固定アドレスELFはbias 0、読取りエラーは失敗として扱う。
- 配置はlldの`getTlsTpOffset`と合わせる。TLS開始アドレスがalignmentの倍数で
  ない場合も`p_vaddr`の剰余を保持する。FSの先頭wordには自身のアドレスを置く。
- 新しい匿名mappingをRLIMIT_ASに従って確保し、`.tdata`をコピーする。
  `.tbss`は匿名mappingのゼロ初期化を利用する。完了前のコピー失敗ではmappingを戻す。
- 初期スレッドでは汎用`ThreadLocalBaseSet`、pthreadでは既存の
  `ThreadCreateContext.thread_local_base`を使用する。
- native threadの終了を確認したjoin/detach回収でTLSを解放する。
  pthread作成失敗でも回収する。forkの呼び出し元はCOWでTLS値を継承する。
  fork時に生存していた他スレッドのprivate memoryは既存のForkLive方針に従って保持し、
  コピーされたフレームが参照していた領域を解放しない。

`user/runtime/linker.ld`は`.tdata`と`.tbss`をPT_TLSにも割り当てる。
SDK自体の再生成は行っていない。SDK提供側は配布時にこのscriptの変更を取り込む必要がある。

## C++ランタイム

LLVM 18のlibc++abiは通常、UNIX/Fuchsia向けのビルドだけに
`cxa_thread_atexit.cpp`を含める。mochiOSのGeneric cross buildでもpthread APIを
明示した場合はこの既存実装を含めるpatchを適用する。
存在しない`__cxa_thread_atexit_impl`の検出をOFFにし、既存のpthread keyによる
destructor実装を使用する。thread無効化やTLS変数の置換は行わない。

## 検証

ホストの6対象・26テストが成功した。

- `static_tls`: ヘッダー、readerのfault、offset、alignment、剰余、overflow。
- `static_tls_memory`: 実際のruntime allocatorを使い、初期値コピー、ゼロ初期化、
  個別mapping、全10コピー位置の失敗時解放、AS admission失敗、base設定失敗を確認。
- `auxiliary_runtime`、`thread_stack`、`thread_lookup_cache`、`thread_lifecycle`。

KVMでは実際にコンパイラが生成したTLSアクセスをroot/euid 501の両方で実行した。
`native_static_tls.cpp`が初期値、ゼロ初期化、8KiB alignment、同時実行するpthreadの
独立性、yield後の値、再作成、C++ destructor、forkの継承とCOW分離を確認する。
実LLVMSupportのファイルbuffer、offset mmap、rename、子プロセスexec、redirect、
wait4の統計も両identityで成功した。PHDRの独立mappingとPT_LOAD内配置の既存試験も成功した。

証跡は`out/checks/mochios-static-tls-229-{host-results,native}.log`。
診断ELFを取り除いた通常イメージでも、input READY、Binder readyとDockの表示を
ログと画面で確認した。VMは検証後に停止した。
通常起動の証跡は`out/checks/mochios-static-tls-229-normal.{log,png}`。
network readinessのtimeoutは診断起動・通常起動の両方で記録されている。
直前の`mochios-elf-metadata-228-normal.log`にも同じ失敗があり、今回未修正。

これは静的ELFのTLS対応であり、動的loaderのTLS module追加や`dlopen`対応の完了を
意味しない。clang/ld.lld本体と生成ELFのネイティブ実行は後続の
[コンパイラ統合試験](posix-native-clang-pipeline.md)で確認済み。
static PIEの追加検証とPIC SDKの制限は[PIE image準備](posix-pie-preparation.md)を参照。
