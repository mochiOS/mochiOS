# ファイルシステムイメージのsymlink（320）

`scripts/mmake/sync-ext2.py`はstageのsymlinkを参照先へ展開せず、リンク先の文字列と
symlink inodeとして記録します。初回のmke2fsによる生成と差分更新で同じ状態を保ち、
相対・絶対・dangling・長いリンク、リンク先の変更と削除に対応します。
通常ファイル・ディレクトリ・symlinkの置換では、古い子を先に除去します。
ディレクトリへのリンクはstageの走査対象を増やしません。

debugfsの引数はshell展開せず、libssの二重引用符を二つ重ねる形式で表現します。
リンク先の空白・引用符・backslashはそのまま保持します。行区切りやNULを含む引数は
コマンドを実行する前に拒否します。stage内のファイル名の空白制限は従来どおりです。

debugfsはコマンド失敗でも終了値0を返すことがあるため、stderrの診断も検査します。
変更開始前に古いstate cacheを無効化し、全コマンド成功後だけ新しいstateを保存します。
中断・部分失敗後の次の通常ビルドは、欠落したcacheを根拠にstage全体から再生成します。
失敗したイメージを成功として公開する処理や、毎回イメージ全体を複製する処理は追加しません。

`services/posix/tests/filesystem_image_symlink.py`の6件が成功しました。
実ext2 / ext4の初回生成・差分更新・型の置換・所有者・stage外を走査しないこと・
無変更時のbyte一致・削除を検査し、e2fsckでも確認しています。終了値0のエラーを注入し、
stateの無効化と通常のmake-ext2-image.shによる再生成も確認しました。
記録は`out/checks/mochios-filesystem-image-320-host.log`です。

これはイメージ生成の対応であり、コンパイラの製品配置や完全POSIX互換性の完了ではありません。
