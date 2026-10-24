# 起動引数ポリシーとネイティブ生成契約の分離

## 境界の整理（181）

`mnu-abi::exec`に残っていた、環境変数とHandle・値の継承を文字列で指定する
3つのprefix定数を`posix-client::launch_arguments`へ移しました。
これらは起動元のユーザー空間エンコーダだけが解釈する規則であり、カーネルABIではありません。
未使用の`SECURITY_IDENTITY_PREFIX`は削除し、互換exportやaliasは置いていません。

core.service、drivers.service、capability.service、service-manager.serviceと対応する試験の
importを更新しています。既存のposix-client依存を使い、新しい依存は追加していません。
カーネルで実際に検証する実行認可のidentity・digest・権限の契約は、この移動の対象では
ありません。カーネルのentry番号やバイナリのprocess_create形式も変更していません。

prefixの文字列値とエンコーダの処理は維持しています。ユーザー空間でargv/envpのスタックを
構成し、Handle継承はnative HandleGrant、値の継承はopaque key/valueに変換して渡します。
カーネルはsource Handleの所有、TRANSFER、権限縮小を検査し、文字列の起動規則や
POSIXのFD番号・環境変数名を解釈しません。

任意のアプリから届いた文字列が、自プロセス内のHandle番号や実行認可を選べるようにしては
いけません。capability側のアプリ起動は、受信済み・認証済みのendpointだけを継承し、
その境界を越える起動文字列を拒否する既存処理を維持しています。

## 調査したネイティブ境界

この整理ではProcessCreate、ProcessImageReplace、ProcessExitとVMの入口を確認しました。
ProcessCreateは既存のimage Handleとserialized startup image/contextを受け取り、
ProcessExitは64ビットの不透明な終了値を保持します。MemoryMap・MemoryProtectは
nativeのaccess/optionsを使い、POSIXの引数やflagsの変換はposix-client/newlib側にあります。
カーネルのプロセス別上限はthreads・IPC・mapping等の汎用機構の上限であり、POSIXの
RLIMIT_*を保持するものとしては扱いません。

これは全カーネルABI・実装の意味論監査を完了したという意味ではありません。
今回の具体的な撤去対象は、カーネル側で使用されていなかった起動文字列ポリシーの公開定義です。
POSIX互換性全体や、Vim・WebKitの実行可否を示す変更でもありません。

## 検証

既存の実エンコーダ試験で、argv/env、binary HandleGrantと権限、opaque値、重複・過長入力、
認可情報と通常引数の分離を確認します。アプリ起動の試験も、リモート入力からのHandle番号や
認証情報の偽造を引き続き拒否します。対象5ターゲット・21件は成功、失敗0件です。
`manifest_launch_retirement.rs`ではカーネル実装だけでなく共有ABIにも、これらの文字列規則の
定義が残っていないことを確認します。カーネル全体のtarget向けcargo checkも成功しました。

既存SDKとRust std overlayを消費する独立ビルドで、利用側4サービスのネイティブビルドも
成功しています。新しいsyscall、Handle転送方式、startupのバイト配置は導入していないため、
この定数配置とimportの変更の確認にVMは起動していません。通常イメージへの配置や、
この4つの新しい実行体の起動試験は行っていません。
kernel checkによるCargo.lockのpatch記録の並べ替えも、巻き戻さず変更へ含めます。

全体のホスト検証は656ターゲット・3,182件成功、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-181-final.log`、
`out/checks/mochios-launch-policy-181-focused.log`、
`out/checks/mochios-launch-policy-181-kernel-check.log`、
`out/checks/mochios-launch-policy-181-services-build.log`です。

## image 置換許可と権限取り消し（198）

プログラムの置換許可に保持した権限は、許可発行後に呼び出し側が取り消した権限を
復元してはいけません。mnu の汎用 image 置換で、保存した Capability と native authority が
現在の権限の部分集合であることを二度確認します。

最初の確認は、identity と呼び出し thread が一致する一回限りの許可を消費するときです。
権限が不足していた許可も消費し、後から権限を付与しても再利用できなくします。
identity / thread の不一致では、別の呼び出し用の許可を消費しません。
既存の信頼された broker による ProcessCreate の許可は変更していません。

image の読み込みと VM の準備中には、同じプロセスの別 thread が権限を取り消せます。
そのため、既存 thread を静止させた後、元の image / LaunchContext / Handle に変更を加える前に
再確認します。この拒否経路では準備した VM と静止 guard を破棄し、元の image を維持して
thread を再開します。カーネルに POSIX の exec、FD、サービス名、ディレクトリ配置は追加していません。

ホスト試験 `exec_authority_revocation.rs` は、本体の許可消費と再検証処理を使って
通常権限・native authority の組み合わせ 4,096 通り、一回限りの消費、別 identity / thread、
取り消し後の権限再付与、許可消費後の取り消しを確認します。
`native_exec_authority.rs` は mochiOS 専用の実行試験です。無効 Handle と有効な ELF の両方で
取り消し済み許可を拒否し、同一 ELF を供給する Provider の最初の要求で権限を取り消すことで、
許可消費後の取り消しも決定的に再現します。拒否後も元のプロセスと取得済み FileHandle が
利用できることを確認します。ホストでこの native 試験を起動した場合は SKIP と表示します。

この修正だけで fexecve、未登録のコンパイル済み ELF の起動、Vim / WebKit の実行が
完成したとは扱いません。明示的な FileHandle による実行許可と、POSIX 側の実行権限判定は
引き続き実装対象です。

198 の検証結果はホスト 3,279 件成功・失敗 0 件（674 ターゲット、ignored 8 件）、
KVM の取り消し試験 3 グループ成功です。通常構成でも Binder の READY と Dock / トップバーの
表示を確認しました。最初の全体ホスト試験では sandbox が `process_vm_readv` を拒否しましたが、
自己メモリ読み取りを許可したホスト実行で全件成功しています。試験を弱める代替処理は追加していません。
SDK / Rust std / runtime は再生成せず、既存のものを消費しています。

記録は `out/checks/mochios-exec-authority-198-host-final.log`、
`out/checks/mochios-exec-authority-198-native.log`、
`out/checks/mochios-exec-authority-198-normal.log`、
`out/checks/mochios-exec-authority-198-normal.png` です。
最終の整形後ビルドでも通常起動を再確認しました。記録は
`out/checks/mochios-exec-authority-198-final-normal.log` と
`out/checks/mochios-exec-authority-198-final-normal.png` です。検証 VM は停止済みです。
