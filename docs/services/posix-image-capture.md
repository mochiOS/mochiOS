# 大きいネイティブ実行イメージの取得

clang 18.1.8の静的実行ファイルは117,205,312 bytesであり、従来の64 MiBの
取得上限と32 MiBのカーネルヒープ上限では起動できない。
strip後も割り当て対象sectionだけで約88 MiBあるため、debug情報の除去では解決しない。

`mnu_abi::exec::MAX_CAPTURED_IMAGE_BYTES`を256 MiBとし、ProcessCreate、
ProcessImageReplace、認可サービスのイメージ準備、ユーザー空間の署名対象読み取りと
static PIE準備で共有する。カーネルへアプリ名、サービス名、パス、POSIXの規則を
持ち込まない。権限確認、入力の固定、認可、失敗時の解放は既存経路を維持する。

カーネルヒープの仮想的な成長上限は512 MiBとした。初期割り当ては従来通り
256 KiBであり、512 MiBを起動時に物理確保する変更ではない。
イメージ確保はfallibleなままとし、資源不足を成功や権限不足へ置き換えない。

## 検証

`services/posix/tests/`の実コードを利用するホスト試験30件が成功した。
既存のbootstrap試験1件は従来のignore指定のまま。
記録: `out/checks/mochios-image-bound-233-host-results-final.log`。

- 64 MiB超と新上限ちょうどのサイズは、入力上限では拒否せず確保処理まで進む。
- 新上限超はpayloadを読まずに拒否する。
- allocation failure、取得権限、認可失敗、参照解放、FD経由execの検査を維持する。
- ヒープ成長時の割り込み抑制とマッピングロックの順序を維持する。

KVMの段階233では実際のclangでCをコンパイルし、実際のld.lldでリンクした
ELFが終了コード37を返すことまで到達した。ただし試験側の出力ファイル再利用に
より最終stdout比較が失敗したため、統合試験全体の成功とは数えない。
