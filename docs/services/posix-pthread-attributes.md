# pthread属性のメモリ境界（phase268）

newlib runtimeのpthread属性APIを、既存のnative `MemoryCopy`を使う
`user_copy`へ接続しました。Cから渡された属性、sched_param、出力ポインタを
直接dereferenceしたり、Rustの参照へ変換したりしません。
SDKのopaque storage契約（40 bytes、alignment 8）と既定値は維持します。

初期化は全storageを明示的なbyte列として書き込みます。通常のaccessorは初期化flagと
選択されたfieldだけを読み、未使用fieldやpaddingを推測して読みません。
stack addressとsizeを同時に返す場合は、両方を捕捉してから出力するため、
出力先が入力storageと重なっても途中の書き込みで次の返却値が変わりません。
無効なmappingはEFAULT、短いnative copyは共通user_copyのEIOを返します。
native copyが一部を書いた後で失敗した場合、そのprefixのrollbackは保証しません。
特に複数出力の後半が失敗しても、成功として返すことはありません。

`pthread_create`は属性を一度だけlocal storageへ捕捉し、初期化flag、detach state、
最小stack sizeを確認します。signal登録、stack確保、native thread作成より前に
完了するため、属性の読み取り失敗でこれらの資源を確保しません。
その後の作成判断は元のポインタを再参照しません。NULL属性は従来の既定値を使い、
属性を読むnative callも行いません。

これはユーザー空間runtimeの修正です。kernel ABI、Capability、POSIX service protocol、
SDKは変更していません。realtime schedulingの追加や、pthread_createの最終thread ID出力など
他のポインタ境界全体の修正まで完了したものとしては扱いません。

## 検証

`services/posix/tests/pthread_guard_attribute.rs`の7 testsで実際の本番関数を実行しました。
ホストのMemoryCopy境界は既存の自プロセス`process_vm_readv`を使い、無効ポインタ、
PROT_NONE、読み取り専用storage、ページ境界、非整列storageと出力、入力と出力の重複、
複数出力の後半失敗、属性捕捉後の元storage変更を確認しています。
既存のtimer callback、native thread作成のエラー処理、VM admission、spawn属性、
thread stack、cancellation制御を合わせて7 targets / 29 testsが成功しました。

SDKを再生成せず、standalone newlib runtimeのmochiOS向けrelease buildも成功しました。
今回の変更は既存MemoryCopyの呼び出し側に限り、native mechanismの境界を変更していないため、
KVMは起動していません。イメージも更新しておらず、この版で実OS統合試験を行ったとは扱いません。
rust-analyzer等の一時停止も行っていません。

証跡は`out/checks/mochios-pthread-attribute-268-host-final.log`、
`-runtime.log`、`-artifacts.json`です。完全なPOSIX互換環境のgoalは継続中です。
