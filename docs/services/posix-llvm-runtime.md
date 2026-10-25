# LLVM向けC++ランタイム（段階215）

LLVM 18.1.8のlibc++、libc++abi、libunwindをmochiOS向けに静的ビルドし、
それらをリンクしたC++プログラムをKVM上で実行しました。clang本体の実行完了ではありません。

## 再現するビルド

公式配布のllvm-project-18.1.8.src.tar.xzを展開した作業用source treeを渡します。
今回取得したarchiveのSHA-256は
`0b58557a6d32ceee97c8d533a59b9212d87e0fc4d2833924eb6c611247db2f2a`です。

```sh
scripts/build-llvm-runtimes.sh out/llvm-port/src/llvm-project-18.1.8.src
```

入力SDKはMOCHIOS_SDK、compilerはCC / CXX、並列数はJOBSで変更できます。
出力は既定でout/llvm-port/build-runtimes/lib/です。SDKの生成・更新・installは行いません。
source treeには同梱のnewlib向けpatchを適用します。適用済みの場合は再適用せず、
対応しないsourceはエラーにします。

newlibのctype maskはsigned charです。libc++のregex class tableでblank / printのmaskが
符号拡張される問題を、newlibの場合だけunsigned charへ戻して修正しています。
thread、例外、RTTI、locale、filesystemは有効です。機能を無効化してビルドを通した結果ではありません。
現行runtimeが実装する公開Cヘッダーもuser/libc-port/includeから参照します。
ホストのCヘッダーやpthread / dl / rtライブラリはリンクしません。

## 静的ELFの起動と例外処理

user/runtime/crt0.Sにimage固有のhiddenな__dso_handleを追加しました。
linker.ldは.eh_frame / .eh_frame_hdrをread-onlyな独立sectionに保ち、
各sectionの開始・終端をhidden symbolで公開します。
libunwindは静的imageのsectionを直接参照する既存のbaremetal設定を使います。
これはカーネルへのPOSIX機能追加や、動的loaderの実装ではありません。

試験はこれらのsourceからcrt0.oとELFを別出力先へ作り、配布SDKの既存crt0.o / linker.ldは
書き換えていません。新しい起動コードとscriptの適用は、SDKを再生成する側でも必要です。

## 実行した検証

services/posix/tests/native_llvm_runtime.cppで次を確認しました。

- vector / stringの操作とsort
- regexの文字種判定とblank / printのmask
- std::thread、atomic、join
- std::runtime_errorのthrow / catchとunwind
- std::filesystem::current_path / space

通常のユーザー空間bootstrapを終えてから、診断managerがPOSIXのfork / execveで
試験ELFを子プロセスとして実行し、waitpidで終了status 0を確認しました。
初期managerを試験ELFで置き換える方法ではbootstrapが満たされないため、その試行は成功に数えていません。
並行ビルド中にBinder readinessがTimedOutになった試行も成功には数えていません。

成功記録はout/checks/mochios-llvm-runtime-215-native.log、入力・ELF・ライブラリのhashは
mochios-llvm-runtime-215-artifacts.jsonにあります。ビルドscriptの実行記録は
mochios-llvm-runtimes-215-script.log、リンク記録はmochios-llvm-runtime-215-link-final.logです。
関連ホスト試験は8件成功しました。診断managerと試験ELFを除去して通常構成へ戻し、
再起動後のinput READY、Binder ready、デスクトップとDockをログ・画面で確認しました。
記録はmochios-llvm-runtime-215-normal.logとnormal.pngです。VMは停止しています。
NICのないrunnerのnetwork Failed(-6)と、USB driverのerrno 22は別件として残ります。

## 続ける範囲

clang / ld.lld本体のビルド、起動、Cのコンパイルと生成物の実行は引き続き必要です。
動的リンク、共有ライブラリのunwind、dlopenやELF TLSの一般的な対応も、この試験では証明していません。
POSIX全体のgoalは継続します。
