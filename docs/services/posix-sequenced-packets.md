# UNIX sequenced-packet sockets

`AF_UNIX / SOCK_SEQPACKET`をposix.serviceの接続管理とrecord queueに追加しました。
カーネルは引き続き汎用Provider Handle・IPC・Eventの境界だけを提供します。
ソケットの種類・接続・メッセージ境界はユーザー空間の状態です。

## 意味論

stream・datagram・sequenced packetをdescriptionの明示的なKindとして保持します。
sequenced packetはlisten / connect / acceptを使い、一つのsend / writeが一つのrecordです。
同じKindのlistenerだけに接続できます。datagramのsendto routeやdisconnectによって
接続先・受信filterを書き換えることはできません。

受信は一度に一つのrecordを返し、短いbufferでは残りを破棄します。
MSG_PEEKはrecordを残し、MSG_WAITALLで次のrecordを連結しません。
空recordもqueueの一件として数え、常にEAGAINになる空queueと区別します。
送信はrecord全体が入るまで成功せず、既存のbyte上限とrecord件数上限を適用します。

相手のSHUT_WRやclose後も、残っているrecordは先に受信できます。
その後はEOFになり、receive waiterとpollの読取通知も発生します。
相手が未読recordを持ったままcloseした場合は、残る入力を返した後にECONNRESETを報告します。
SHUT_WR後の送信はEPIPEで、SIGPIPE / MSG_NOSIGNALも扱います。

acceptは接続先Kindと共有容量・timeoutを引き継ぎ、FDのNONBLOCK / CLOEXECは
既存のaccept規則に従います。失敗したacceptの再試行時は既存のID退役処理を使います。

libcは専用のprocess-local FD Kindを使ってrecord pathを選びます。
dup / forkは同じProvider descriptionを共有し、execとSCM_RIGHTSはuserspace FD metadataで
型を保持します。FD番号をカーネルのソケット操作へ持ち込むAPIはありません。

## Record endの通知

SCM_RIGHTSを含むrecvmsg responseに、record endのbooleanを追加しました。
wireの22 byte目のbit 0だけを使い、残るbitとreserved byteは拒否します。
source-address lengthの領域とは別です。
実際のrecordではtrue、byte streamとEOFではfalseです。
libcはsequenced packetでtrueを受け取ったときにMSG_EORを返します。
空recordのMSG_EORと、データが尽きた後のEOFを区別できます。
送信のMSG_EORは一回の送信が作るrecord境界を明示するものとして扱います。

接続型のrecordとMSG_EORの基準は[POSIX socket](https://pubs.opengroup.org/onlinepubs/9699919799/functions/socket.html)、
一回の受信と切り捨ては[POSIX recvmsg](https://pubs.opengroup.org/onlinepubs/9699919799.2013edition/functions/recvmsg.html)を参照しています。

## 検証と残る範囲

テストはservices/posix/testsに配置します。
socket_seqpacket.rsは実装本体のpair・接続・accept・切断・異なるKindの拒否・
record end wireとFD metadataを検証します。
native_socket_seqpacket.cはlibcから実際のIPCを使い、record・recvmsg・SCM_RIGHTS・
dup / fork / exec・filesystem上のbind / listen / connect / acceptを確認します。

Internet domain socketやTLSの完成を示すものではありません。
配布SDKとRust std overlayの再生成は行わず、native試験には単独ビルドしたruntimeを使います。
POSIX互換環境全体のgoalは継続中です。

最終ホスト全体回帰は683 targets、3,318件成功・失敗0件・8 ignoredでした。
KVMでnative probeの4 groupsが成功し、試験用置き換えを戻した通常構成でも
input READY、Binder、デスクトップとDockを確認しました。VMは停止済みです。
NICを付けないrunnerのnetwork Failed(-6)と既存USB driverの起動失敗は残っており、
全サービスが成功したという扱いにはしていません。

記録は`out/checks/mochios-seqpacket-206-final-host.log`、`-native.log`、
`-normal.log`と`.png`、`-artifacts.json`、`-evidence.json`です。
