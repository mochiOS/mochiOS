# ViewKit icon rebuild plan

ViewKitの旧アイコン資産は削除し、Figmaを正として新しい標準アイコンセットを再構築する。
再構築が完了するまで`IconName`のAPI互換性は維持するが、未作成アイコンは描画しない。

## 最優先: 現在のコードが要求する未作成アイコン

以下は`IconName`に公開済み、または標準コンポーネントと標準アプリから参照されている。
すべてFigmaで再設計し、ViewKitの新規資産として実装する必要がある。

- [ ] `search` - 検索フィールド、Filesツールバー
- [ ] `plus` - 追加操作
- [ ] `minus` - 削除・縮小操作
- [ ] `check` - Checkbox、完了状態
- [ ] `x` - 閉じる、取消、入力消去
- [ ] `settings` - Settings、仮想化警告
- [ ] `chevron-left` - 戻る、前項目
- [ ] `chevron-right` - 進む、次項目、ログイン
- [ ] `chevron-down` - Picker
- [ ] `arrow-up` - 親ディレクトリ、アップロード系操作
- [ ] `house` - Home、Account
- [ ] `app-window` - Applications、Input
- [ ] `download` - ダウンロード
- [ ] `hard-drive` - Storage、Network
- [ ] `folder` - 汎用フォルダ
- [ ] `folder-open` - 開く、ファイル選択
- [ ] `folder-plus` - 新規フォルダ
- [ ] `file` - 汎用ファイル
- [ ] `file-text` - 文書、Security
- [ ] `file-image` - 画像ファイル
- [ ] `file-archive` - アーカイブ
- [ ] `external-link` - 外部接続、Network prompt
- [ ] `layout-list` - Filesリスト表示
- [ ] `layout-grid` - Filesグリッド表示、Applications
- [ ] `columns-3` - カラム表示
- [ ] `eye` - Appearance、表示
- [ ] `volume-2` - 音量

## 標準UIとして追加すべき未作成アイコン

### Navigation and window

- [ ] `arrow-left`
- [ ] `arrow-right`
- [ ] `arrow-down`
- [ ] `menu`
- [ ] `more-horizontal`
- [ ] `more-vertical`
- [ ] `refresh`
- [ ] `sidebar`
- [ ] `panel-left`
- [ ] `maximize`
- [ ] `minimize`

### Common actions

- [ ] `edit`
- [ ] `trash`
- [ ] `copy`
- [ ] `cut`
- [ ] `paste`
- [ ] `upload`
- [ ] `share`
- [ ] `save`
- [ ] `undo`
- [ ] `redo`
- [ ] `filter`
- [ ] `sort-ascending`
- [ ] `sort-descending`

### Form and feedback

- [ ] `eye-off`
- [ ] `info`
- [ ] `warning`
- [ ] `error`
- [ ] `help`
- [ ] `calendar`
- [ ] `clock`
- [ ] `lock`
- [ ] `unlock`

### System and account

- [ ] `user`
- [ ] `users`
- [ ] `power`
- [ ] `wifi`
- [ ] `wifi-off`
- [ ] `network`
- [ ] `bluetooth`
- [ ] `volume-x`
- [ ] `volume-1`
- [ ] `sun`
- [ ] `moon`
- [ ] `display`
- [ ] `keyboard`
- [ ] `mouse`
- [ ] `shield`

## 実装条件

- Figma上のコンポーネントを唯一の視覚仕様とする。
- 全アイコンでview box、基準線、stroke、corner、optical sizeの規則を統一する。
- 色をSVGへ固定せず、ViewKitのThemeと状態色からtintできる構造にする。
- Light / Darkで別資産を要求しない単色アイコンを基本とする。
- Small / Medium / Largeの各control sizeで視認性を確認する。
- hover、pressed、focus、disabledで形状を差し替えず、状態色とopacityをViewKitが管理する。
- 意味を持つアイコンにはアクセシビリティラベルを付け、装飾アイコンは読み上げ対象外にする。
- Figma名、`IconName` variant、asset file名の対応を1対1にする。
- 新規資産の導入時にSVG decode testと全`IconName`の登録漏れtestを復元する。

## 導入順序

1. Figmaでgrid、stroke、corner、optical sizeを確定する。
2. 最優先27個を作成し、ViewKitのasset registryへ登録する。
3. Checkbox、Picker、Button、TextField、Toolbarで状態別表示を確認する。
4. Settings、Files、Secure UIで意味とアクセシビリティラベルを確認する。
5. 追加候補を標準コンポーネントの実装順に導入する。
