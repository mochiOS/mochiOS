# とぅーどぅー

## セキュリティてきなとぅーどぅー
#### 最優先
- [ ] サービス監視・自動復旧を実装
  - 対象: `src/services/core/src/main.rs`, `src/core/kernel.rs`, `src/services/index.toml`
  - 内容:
    - `core.service` に子サービスの生死監視（`wait`/状態監視）を実装
    - クラッシュ時の自動再起動（指数バックオフ、再起動上限、無限ループ防止）
    - 重要サービス（`disk.service`, `fs.service`）の復旧ポリシーを明示
  - 完了条件:
    - 重要サービスを強制終了しても自動復旧し、システム全体が停止しない

- [ ] カーネル側にもウォッチドッグを追加する（core.service障害時の最終防衛）
  - 対象: `src/core/kernel.rs`, `src/core/task/scheduler.rs`
  - 内容:
    - カーネルスレッドで heartbeat 監視
    - `core.service`無応答時の再起動/縮退モード移行
  - 完了条件:
    - `core.service`が停止しても監視機構が生き残り、復旧動作が走る

- [ ] ブート信頼連鎖（Secure Boot相当）を作る
  - 対象: `src/boot/loader.rs`, `build.rs`, `builders/fs_image.rs`
  - 内容:
    - kernelとinitfsの署名検証（少なくともハッシュ+署名）
    - リリース成果物の改ざん検出、ロールバック防止
  - 完了条件:
    - 改ざんしたイメージ/バイナリを起動前に拒否できる
    - ブートローダーは信頼しよう。じゃないとどうにもならねぇ。

- [ ] 実ユーザー権限モデル（UID/GID）を作る
  - 対象: `src/core/syscall/pgroup.rs`, `src/core/syscall/fs.rs`, `src/core/task/process.rs`
  - 内容:
    - `getuid/getgid/geteuid/getegid` の常時 `0` を廃止
    - プロセスに資格情報を保持し、FSアクセス判定に反映
  - 完了条件:
    - 非特権プロセスが root 専用操作へ到達できない

- [ ] DoS耐性のためプロセスごとのリソース上限を作る
  - 対象: `src/core/task/process.rs`, `src/core/syscall/process.rs`, `src/core/syscall/pgroup.rs`
  - 内容:
    - メモリ・スレッド・FD の上限を導入
    - `getrlimit/prlimit`無制限スタブを実装に置換
  - 完了条件:
    - 悪性/暴走プロセスが単独でシステム全体を枯渇させられない

#### 高優先
- [ ] シグナル復帰アドレスを安全にする
  - 対象: `src/core/syscall/signal.rs`
  - 内容:
    - `sa_restorer` を無検証で信頼しない
    - カーネル生成の固定 `sigreturn` 経路へ寄せる
  - 完了条件:
    - 任意 `restorer` 指定で制御フローを奪えない

- [ ] ELFロード後の W^X を徹底する
  - 対象: `src/core/task/elf.rs`, `src/core/syscall/exec.rs`, `src/core/mem/paging.rs`
  - 内容:
    - ロード中のみRW、実行時は`PF_W`に応じて最終保護へ変更
    - 実行可能ページの不要な書き込み権限を除去
  - 完了条件:
    - 実行セグメントが常時writableにならない

- [ ] 例外経路のKPTI/SMAP適用を統一する
  - 対象: `src/core/interrupt/idt.rs`, `src/core/syscall/syscall_entry.rs`, `src/core/syscall/mod.rs`
  - 内容:
    - SYSCALL経路と同等に、例外/IRQでもCR3切替とユーザメモリアクセス制御を統一
  - 完了条件:
    - エントリ種別（syscall/exception/irq）で保護レベル差が残らない

- [ ] **IPCのアクセス制御を強化する**
  - 対象: `src/core/syscall/ipc.rs`, `src/services/fs/src/main.rs`, `src/services/disk/src/main.rs`
  - 内容:
    - サービス別 ACL（送信元権限/送信元PID/操作種別）
    - 重要操作はcapabilityトークンを要求
  - 完了条件:
    - 非許可主体からのIPCリクエストが拒否される

- [ ] `mprotect` を実装し、メモリ保護変更を本物にする
  - 対象: `src/core/syscall/pgroup.rs`, `src/core/mem/paging.rs`
  - 内容:
    - 現在のスタブ (`SUCCESS`返却中心) を廃止
    - ユーザ空間ページに対する保護更新と検証を追加
  - 完了条件:
    - `mprotect`が実際にページ属性を変える

#### 中優先
- [ ] クラッシュテレメトリの永続化
  - 対象: `src/core/panic.rs`, `src/core/interrupt/idt.rs`, `src/services/fs/src/main.rs`
  - 内容:
    - クラッシュ情報をリングバッファ化し、再起動後に回収可能にする
  - 完了条件:
    - 再起動後に直前クラッシュ情報を取得できる

- [ ] 監査ログ/セキュリティイベントログを整備する
  - 対象: `src/core/util/log.rs`, `src/core/syscall/*`, `src/services/*`
  - 内容:
    - 認可失敗、異常IPC、再起動理由、例外統計を記録
    - 今ぐっちゃぐちゃすぎるんだよﾜﾚｪ
  - 完了条件:
    - 重大イベントの追跡が可能

- [ ] 安全な更新/ロールバック基盤を設計して作る
  - 対象: `build.rs`, `builders/*`, `fs/`, `ramfs/`
  - 内容:
    - A/B的な更新、検証失敗時の既知良好版復帰を用意
  - 完了条件:
    - 不正/破損更新で起動不能にならない

#### 今の実装
- `core.service`は監視ループのみで、再起動監督機構は未実装
- UID/GID系syscallは現状すべて`0`を返す
- カーネル panic は停止（`hlt` ループ）で、自己復旧経路は未実装