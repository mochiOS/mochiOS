# UDP packet Handle backend（段階210）

network.serviceに、明示的に渡されたfactory HandleからUDP packet Handleを作る処理を追加しました。
[段階209のauthority](posix-network-authority.md)を拡張するものです。
POSIX AF_INETのAPI接続はまだ行っていません。kernelのABI・policyは変更していません。

## 権限と寿命

作成要求はversion 1 / opcode 2、IPv4 addressとportの16 bytesです。
factoryのCONTROLだけをnative IPCへ添付し、network.serviceがidentify_provider_objectで
自分のendpointが発行した生存中のfactoryか確認します。request内にPID、FD、object IDはありません。
net.connectをdropした後も、明示的に保持・委譲されたfactoryは利用できます。
port 0はephemeral、明示的な1024未満のportは禁止です。

作成したpacket HandleのrightsはREAD / WRITE / CONTROL / WAIT / DUPLICATE / TRANSFERです。
SEEK / MAPは付けません。packetはfactoryから独立して存続し、最後の参照のcloseで
Provider releaseを受け、bindingと送信queueを回収します。IDは再利用しません。
作成・初期signal・返信の失敗でもbindingを回収します。raw IPCのProvider偽造は拒否します。

## データ経路

送信はnative HandleWrite、受信はHandleRead、peekはoffset 0のHandleReadAtを使います。
wireはnet-device-protocol/datagramに置きます。

- endpoint: address 4 bytes、port little-endian u16、reserved 2 bytes（計8）。
- send: endpointの後にpayload。address/portが全て0なら設定済みpeerを使います。
- receive: source address/port、元のpayload長u16、truncated flag u32（計12）の後にpayload。
- 空のdatagramも12 bytesの受信結果となり、データ未着の-11と区別できます。
- 短い受信bufferでは残りのrecordを捨てます。peekは捨てません。
- header未満の受信bufferはrecordを消費せず-22です。
- payload上限は1472 bytes。fragmentationは実装していません。

CONTROLでlocal / peer照会、peer設定・解除、未処理送信エラーの取得・消去を提供します。
peer設定による受信filterは既存UdpSocketTableを使い、将来到着するsource addressとportを
照合します。すでに受信済みのrecordは維持します。POSIX connectの追加policyはfrontendの責務です。

送信queueはpacketごとに最大8 recordsです。送信先はqueueへ受け入れる時に確定し、
後からpeerを変更しても受け入れ済みrecordの送信先は変えません。
transportの-11ではrecordを保持し、queue満杯ならWRITABLEを落とします。
送信成功はlocal queueへの受け入れであり、UDP配送の保証ではありません。
後続のtransportエラーはREADABLEで通知し、次のread/writeまたはTAKE_STATUSで一度だけ取り出せます。
最後のcloseでは未送信recordを破棄します。

NICありでは既存NetworkStackのIPv4送受信を使います。NICなしでも同じBackend経路の
loopback通信を提供します。127/8とwildcardをbindでき、それ以外の未設定addressは-99、
NICなしの外部宛送信は-6です。loopback受信側のqueue満杯・未bindはUDPのdropとして扱います。
NICなしで既存TCP/TLS/HTTP要求を成功扱いにする変更はありません。

## 検証と制限

ホストの関連10 targets、48 testsが成功しました。実装本体を使って認証、権限、
wire検証、peek、record境界、空record、quota、発行失敗時の回収、送信backpressure、
readiness、非同期送信エラーの一度限りの消費を確認しています。

KVM（NICなし）のnative試験は既存7 groupsと追加3 groups、計10 groupsが成功しました。
実Handle転送、権限縮小、READABLE待機、peek、record切り捨て、空record、default peer、
factoryをcloseした後の通信、同一portの32回再bind、外部宛の-6を確認しました。
実NICとの通信は未検証です。
通常構成へ復元後、input READY、Binder起動、デスクトップとDockの描画も確認しました。
診断用のmanager / compositor / manifest / verificationは復元し、VMは停止しました。
NIC不在のnetwork Failed(-6)と既存USB driver errno 22が残っているため、全サービス正常とは扱いません。
SDK / Rust std overlayは再生成していません。

ログとartifact hashはout/checks/mochios-datagram-provider-210-{final-host,native,normal}.log、
-normal.png、-artifacts.json、-evidence.jsonに保存しています。
署名付き実装コミットはuser a5c4008、services 39e62cdです。

残る作業はPOSIX socket / bind / connect / sendto / recvfrom / getsockopt等の接続、
LaunchContextによるendpoint配布、実NICでの送受信試験です。
TCP socket Handle、IPv6、IP fragmentation、ICMP error、multicast、socket options全般は
この段階の実装範囲に含めていません。POSIX互換環境全体のgoalは継続中です。

共有description policyとnativeアダプタは[段階211](posix-inet-datagram-description.md)で追加しました。
libc FD経路への登録はまだで、両者の配線が次の作業です。
