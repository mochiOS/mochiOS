# canonical入力の行末容量

端末の設定値を調べる過程で、canonical入力だけで64KiBの入力queueを埋めると、
改行も格納できず、slaveには未完了の行をreadできない状態になる問題を確認しました。
EOFやeraseなら回復できましたが、普通に改行で行を完了する入力が詰まっていました。

`pty::CANONICAL_LINE_CAPACITY`は行末を含む64KiBです。canonical行のデータは
その1バイト手前までとし、行末用の領域を残します。上限後に来た通常データと
引用データは、echoせず消費・破棄します。引用した改行はデータなので、この領域を使いません。
quoteは破棄した1バイトで完了し、その後の本来の改行を受け付けます。

NL・入力変換後のNL・有効なVEOL/VEOL2は行末として格納・commitします。
erase・kill・EOF・signal・flow controlは従来通り行データの容量検査より前に扱います。
raw入力は従来通り全64KiBを使い、容量を超える分はbackpressureになります。

破棄するのは、未完了のcanonical行自体がデータ上限へ達した場合だけです。
すでに確定した未読行がqueueを占めるために短い行が進めない場合は、従来通り
EAGAIN/部分書き込みとし、slaveのread後に同じデータを再送できます。
echo出力やEOF recordのmetadataにも既存の容量制限を維持します。

[POSIXのcanonical input](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap11.html)
は、定めた行上限を超えたときの処理を実装定義としています。mochiOSでは上記の
行末領域確保と余剰データの破棄を方針とします。これはサービス内の端末処理であり、
mnuへ端末やPOSIXの意味論を追加するものではありません。

## 検証（176）

`pty_canonical_capacity.rs`は実際のPtyManagerで、過長行からの改行、EOL/EOL2とCR変換、
引用改行、eraseとsignal、未読の確定行によるbackpressure、raw入力を検証します。
`pty_input_capacity.rs`のEOF試験も、行末領域を維持する方針へ合わせて更新しています。
PTY・UTF8・metadata容量とecho・引用入力を合わせた対象61件が成功しました。
引用入力の試験はIEXTENを明示的に有効にしており、初回fixtureの不足を修正しています。
大きなechoの試験も、実際に保持する最大行データ量を使い、出力queueを超える再表示・
word erase、flow control、signal flush、peer close後のdrainを維持しています。
入力queue全体が満杯の引用試験は、確定済みの未読行と短い未完了行を合わせ、
prefixがEAGAINになったあとに同じ入力を再送できることを引き続き確認します。

今回はホスト上で実行できるサービス内の状態処理だけを変更しています。
wire protocol・カーネルABI・newlibには変更がありません。
mochiOS向けposix-serviceのビルドも成功しましたが、通常イメージへはまだ配置していません。
VMは起動せず、SDK・Rust std・通常イメージは更新していません。
`_PC_MAX_CANON`などの問い合わせ接続は、まだこの変更に含めていません。

全体655ターゲット・3,166件が成功し、失敗0件でした。
ログは`out/checks/mochios-posix-host-audit-176-final.log`と
`out/checks/mochios-canonical-capacity-176-focused-final-2.log`へ保存しています。
