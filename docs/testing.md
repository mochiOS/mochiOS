# mochiOS の統合テスト

`mmake test` は通常のOSイメージをKVMで起動し、ゲスト内で登録済みの必須試験を実行します。
`mmake host-test` はVMを起動せず、ホストで共通ライブラリ・プロトコル・認可処理などを確認する簡易版です。
リリース前の品質確認には、対象成果物と対応するKVM試験結果を使用します。

## 実行方法

```sh
mmake host-test
mmake test
```

詳細な標準出力も表示する場合は `mmake -v host-test` / `mmake -v test` を使います。
Python 3.11以降と、Rust/Cargoの開発環境が必要です。依存crateは通常の開発環境準備で
取得しておきます。ホスト試験のCargo実行はofflineです。
KVM試験には `/dev/kvm`、QEMU、OVMF、VirGLを使用できるDRMデバイスとデスクトップ環境が必要です。
KVMや描画環境が利用できない場合はエラーになり、TCGや描画無効への自動切り替えは行いません。
画面を表示しないホストでは、OpenGLを利用できる環境で次のように実行します。

```sh
MOCHIOS_TEST_DISPLAY=egl-headless mmake test
```

既定のUEFI環境は開発用鍵が登録されたOVMFです。別の署名環境を確認する場合は
`MOCHIOS_TEST_OVMF_CODE` と `MOCHIOS_TEST_OVMF_VARS` で対象ファームウェアと登録済み変数ファイルを指定します。
変数ファイルとOSディスクは実行ごとにコピー／overlayを作成するため、入力成果物は書き換えません。

既存の配布候補をビルドし直さず確認する場合は、同じ実行器に対象を渡します。

```sh
python3 scripts/test.py kvm --image /absolute/path/to/disk.img
python3 scripts/test.py host --list
python3 scripts/test.py kvm --list
```

## 登録と共通ライブラリ

試験一覧は [`tests/catalog.toml`](../tests/catalog.toml) に置きます。ホスト試験はコマンドと制限時間、
ゲスト試験は必須IDと制限時間を登録します。ゲストが返す一覧はホストの登録内容と完全一致する必要があります。
古いゲストバイナリや試験の削除によって、実行件数が減ったまま成功することを防ぎます。

[`scripts/testing/`](../scripts/testing/) がプロセスの終了・制限時間・QMP操作・結果検査・集計を共有します。
[`mochios-test-support`](../user/crates/test-support/) はlibtestに依存しないRustの試験ライブラリです。
試験関数は `Result<(), String>` を返し、各ケースを新しい通常プロセスに分離して実行します。
ケースが停止した場合はその子プロセスを終了させ、失敗を報告します。

ゲストの入口は `/bin/os-test` です。通常のTerminalとmshから起動し、通常のCapability検査を通ります。
結果は実行IDを付けたJSONとして、既存のネイティブ診断出力からシリアルへ送ります。
試験用の送信権限やネットワーク接続は不要です。通常のログと区別するframeを付け、
全ケース・最終集計・終了値の完了frameが一致するまで成功にしません。
カーネルやサービスに試験専用の権限付与・承認APIは追加しません。

最初のKVM試験群は、ネイティブ実行識別、Handleの権限縮小とIPC転送、ファイルのrename/unlink、
FDのoffset共有、子プロセスの終了値、スレッド、旧起動認可経路の拒否、署名改変の拒否を対象とします。
GUIはデスクトップとTerminalを通した通常起動を確認します。
更新・復旧・全GUI操作・完全なPOSIX互換性は、それぞれの必要なケースを追加して確認します。

## 結果と品質確認

結果は `out/tests/host/<実行ID>/` または `out/tests/kvm/<実行ID>/` に保存します。
`out/tests/host/latest` と `out/tests/kvm/latest` は、それぞれ最後に完了した実行を指します。

- `result.json`：ケースごとの結果、対象commitと作業差分のhash、試験一覧のhash、経過時間。
- `junit.xml`：CIで集計できる結果。
- KVMではイメージのSHA-256、実際のQEMU引数とKVM／DRM確認結果、シリアルログ、ゲストの結果、画面も保存します。

全必須ケースの成功と最終結果の受信を確認した場合だけ終了コード0になります。
`mmake test` は同じ依存グラフ内で `image` を生成してから試験を実行します。
イメージ生成に失敗した場合はmmakeが非0で終了し、KVM試験は開始しません。
ビルド失敗もJSONに保存する必要がある場合は、外側のmmakeを使わず
`python3 scripts/test.py kvm --build` を実行します。
失敗・未実行・タイムアウト・不正な結果・通信切断は非0です。skipを成功に数えません。
イメージが実行中に変わった場合もエラーになります。

`host-test` の成功はKVM試験の代わりにはなりません。品質承認では `mode = kvm` の結果と
承認対象のイメージhashを照合し、登録された試験範囲を確認します。作業中の変更は結果に記録されます。
直近のコマンドの成功も必須です。ビルド失敗後に、以前の `latest` を今回の成功結果として扱いません。
リリースチェックリスト全体の承認は、必要な試験・機種・人による確認を含む各項目と対応付けます。
