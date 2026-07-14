# How to contribute to mochiOS

mochiOSはどんな人でも参加できるオープンソースプロジェクトです。

どんな小さな貢献でも大歓迎です。バグの報告、ドキュメントの改善、コードの修正、機能の提案など、あなたのスキルや興味に応じて貢献できます。

## 開発の始め方

mochiOSはrepoを使ったマルチリポジトリ構成です。
カーネル、ユーザーランド、サービス、アプリケーション、ライブラリ、ビルドツールなどが別々のGitリポジトリとして管理されています。
そのため、通常のGitリポジトリを1つ触る感覚とは少し違います。

### 必要なもの

開発には次のツールが必要です。

```sh
git
repo
make
perl
rustup
cargo
```

ビルドや実行には、環境によって追加のパッケージが必要です。
まずはこの文書の手順でワークスペースを作り、足りないものが出たらエラーに従って入れてください。

### ワークスペースを作る

まずmochiOS本体をcloneします。

```sh
git clone https://github.com/mochiOS/mochiOS.git
cd mochiOS
```

次にrepoを初期化します。

```sh
make repo-init
```

これでmanifestに書かれた各リポジトリがクローンされます。便利！

### mws をインストールする

mochiOSでは、複数リポジトリの状態を扱うために`mws`を使います。インストールには

```sh
make install
```

を実行して下さい。
このコマンドは`tools/mws`にあるmwsをCargoでインストールします。

インストール後、次のコマンドが使えることを確認してください。

```sh
mws --help
```

もし`mws`が見つからない場合は、Cargoのbinディレクトリが`PATH`に入っているか確認してください。（詳しくはググってみてください）

```sh
echo $PATH
```

でPATHを確認できます。
通常は次をshellの設定（.bashrcなど）に追加します。

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

### mwsを初期化する

ワークスペースを作ったら、最初にmwsを初期化します

```sh
mws init
```

これにより、manifestに含まれる各リポジトリへGit hookが設定されます。

以後、各リポジトリでcommitすると、mwsがワークスペース全体の状態をsnapshotとして記録します。

### 現在の状態を見る

```sh
mws status
```

このコマンドは最新のsnapshotと現在のワークスペースを比較します。

差分がある場合は、どのリポジトリのHEADが変わったか、どこに未コミット変更があるかが表示されます。

`modified`は、そのリポジトリのHEADが最新snapshotと違うことを表します。
`dirty`は、未コミットの変更があることを表します。

### ログを見る

```sh
mws log
```

mwsが記録したワークスペース履歴を表示します。
ここに出るcommitIDはmwsが生成したIDです。（Gitのcommitとは別物です）

### restore

復元には`mws restore`が使用できます。
ですが、いきなりrestoreすると危険なので、まずは`--dry-run`で表示だけしてみましょう。

```sh
mws restore latest --dry-run
```

`--dry-run` は表示だけです。checkout、switch、reset、cleanは実行されません。
これで問題がないことを確認してから、実際に復元しましょう。

```sh
mws restore latest
```

これは最新のsnapshotにワークスペース全体を戻します。
この復元は完全再現用なので、各リポジトリはdetached HEADになります。過去の状態を確認したい場合や、ビルドを再現したい場合に使います。
未コミット変更がある場合、デフォルトでは失敗します。

未コミット変更を破棄してでも戻したい場合だけ `--force` を付けます。

```sh
mws restore latest --force
```

また、restoreするsnapshotを指定することもできます。

```sh
mws restore <SnapshotID>
```

snapshotIDは、`mws log`で表示されるID（commit ID）です。

#### 作業用ブランチ付きでrestoreする

復元した状態から作業したい場合はdetached HEADだと不便なので、作業用ブランチを作ります。

```sh
mws restore latest --work fix-build
```

これにより、各リポジトリで `mws/fix-build` ブランチが作られます。

すでに同じwork branchがある場合は失敗します。作り直したい場合だけ `--force` を付けます。

```sh
mws restore latest --work fix-build --force
```

### work branchを見る

```sh
mws work list
```

現在のwork branchが表示されます。
`checked out`が付いているリポジトリは、現在そのwork branch上にいます。

### work branchを消す

```sh
mws work clean fix-build
```

`mws/` を付けても同じです。

```sh
mws work clean mws/fix-build
```

削除前に確認が表示されます。
Gitが未マージだと判断したブランチは、通常の削除では消えません。
その場合は、必要な変更が残っていないことを確認してから`-f`（または`--force`）を使います。

```sh
mws work clean fix-build -f
```

### 基本的な開発の仕方

通常の開発は次の流れです。

```sh
mws status
mws restore latest --work my-change
```

変更を加えて、各リポジトリでコミットします。

```sh
git -C core status
git -C core add .
git -C core commit -m "message"
```

コミットするとmwsのhookが動き、ワークスペース全体のsnapshotが保存されます。

では、作業状態を確認してみましょう。

```sh
mws status
mws log
```

作業用ブランチが不要になったら削除します。（かならずpushしたり、必要な変更が残っていないことを確認してから削除してください）

```sh
mws work clean my-change
```

### 注意

mwsは複数のGitリポジトリをまとめて扱います。`restore`や`work clean -f`は危険な操作です。

不安な場合は、先に次のコマンドを使ってください。

```sh
mws status
mws restore latest --dry-run
mws work list
```

`--force` は、変更を破棄してよいと判断した場合だけ使ってください。
