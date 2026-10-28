# IPv4 TCP受動接続と受信経路（phase271–272）

POSIXのインターネットsocketはUDPまでで、network-stackのTCPも能動接続だけでした。
`TcpConnectionTable`へlisten、SYN受理、接続成立のキュー、accept、listenerのcloseを追加しました。
SYN-RECEIVEDの接続は既存の再送・送受信buffer・切断処理を使います。
受信時に必要なbufferは失敗可能な確保を使い、確保できなければ接続を公開しません。
これはユーザー空間のTCP transport実装で、mnuにTCPやPOSIX socketの状態を追加していません。

受動接続の状態遷移とACK確認は[RFC 9293の受信処理](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.10.7.2)を参照しています。
今回の実装では、重複SYNが同じ接続と初期sequenceを使い、余分な待ち枠を消費しません。
SYN/ACKの再送に加え、最終ACKが失われたときは既存の能動接続側が再ACKします。
受理したSYN内データもsequence / ACKへ反映しますが、成立前にはアプリへ渡しません。
最終ACKで同じ先頭データが繰り返された場合も、二重に配送しません。

## 所有と容量

listenerと接続は一つのtable容量を共有し、未成立・成立済み未acceptの接続もbacklogへ数えます。
成立した順にacceptし、成立待ちの先頭接続が後続の成立済み接続を止めない構造です。
失敗した未accept接続は再送期限切れやRSTで回収し、待ち枠を戻します。
listenerのcloseは未accept接続を破棄しますが、accept済みの接続は残します。
未accept接続の整理では、accept済み接続の終端状態を巻き込んで破棄しません。

ownerと接続IDは既存transport table内のラベルです。PID、POSIX FD、Kernel Handleではなく、
それ自体をIPCの認証やCapabilityとして使うものではありません。acceptとcloseは記録された
ownerを照合します。実際の外部権限確認・Kernel Handle発行は次のProvider接続側の責務です。
初期sequenceはlibraryが固定値を選ばず、呼び出すserviceから渡します。

## 検証と接続範囲

`services/posix/tests/tcp_passive.rs`は本番libraryのpacket encode/decodeとチェックサムを使い、
能動側と受動側のハンドシェイク、双方向のデータ、再送、半切断、SYN内データ、ACK/sequence/RSTの
拒否、sequenceのwrap、zero window、owner不一致、容量回復、成立順のaccept、buffer確保失敗を確認しています。
既存の能動接続の回帰試験も同targetで実行し、UDP関連を合わせて4 targets / 45 testsが成功しました。
network.serviceのmochiOS向けbuildも成功しました。

## 受信経路の接続（phase272）

`TcpConnectionTable::receive_packet`を追加し、network.serviceの実際のTCP受信処理から使うようにしました。
既存接続の更新と新規SYNの受理を同じ入口へまとめ、成立済み接続がaccept待ちキューへ入るようにしています。
未接続ポートへのRST、listener宛ての予期しないACKへのRST、状態に応じた不正ACKへの応答、
受信範囲内の非一致RSTへのchallenge ACKを返します。範囲外RSTやlistener宛てのRSTには応答しません。
これは[RFC 9293の受信処理](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.10.7)を参照したものです。
接続をresetした場合は、未送信データだけでなく受信済みデータも破棄します。

新規SYNの待ち枠・table容量・SYN内データの受信容量を確認してから、serviceの`RandomFill`経由で初期sequenceを選びます。
乱数の取得に失敗した場合は接続を公開しません。重複SYNや満杯のbacklogでも余分な乱数を取得しません。
transportは未指定・multicast・limited broadcastのアドレスを拒否し、物理interfaceの入口では
loopback送信元と自身のsubnetのdirected broadcastも拒否します。/31のpeerはbroadcastと扱いません。

新しい`services/posix/tests/tcp_dispatch.rs`は、同じtableに能動・受動接続を置き、実際の
TCP encode/decodeを使うpacket pumpで接続拒否、再送、双方向通信、受信window、半切断、
sequenceのwrap、zero window、SYN内データ、容量回復、保持中の接続IDへの再接続の付け替え防止を確認します。
さらに`network_routing_policy.rs`は本番のnetwork.service受信処理を読み込み、デバイスI/O・時刻・
乱数源だけを試験用に置き換え、IPv4入力からEthernet出力までのSYN/ACK、accept、データ、
チェックサム拒否、不正送信元、乱数取得失敗を確認します。UDP関連を合わせて6 targets / 70 testsが成功しました。
network.serviceのmochiOS向けbuildも成功しました。

権限付きTCP Provider、NICなし環境で使うservice内TCP loopback backendへの接続は未完了です。
packet pumpはホスト試験用であり、mochiOS上のloopback socketの実装完了を意味しません。
POSIXのAF_INET/SOCK_STREAM、bind/listen/accept、非blocking接続、FDのdup/fork/exec、
readiness、timeout、shutdownを実OSで使えるようにする作業は残っています。
すべてのTCP機能やRFCへの適合を証明した段階でもありません。
kernel ABI、IPC wire、SDK、OSイメージは変更せず、KVMも起動していません。

phase271の証跡は`out/checks/mochios-tcp-passive-271-host-syn-data.log`、`-network-syn-data.log`、
`-artifacts.json`です。phase272は`out/checks/mochios-tcp-dispatch-272-host-admission.log`、
`-network-admission.log`、`-artifacts.json`です。完全なPOSIX互換環境のgoalは継続中です。
