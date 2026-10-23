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
