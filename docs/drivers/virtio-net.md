# virtio-net driver

## 1. 対象device

`virtio-net.driver`はPCI vendor `0x1af4`、modern virtio-net device `0x1041`を検出します。
QEMUは次の形式で起動します。

```text
-netdev user,id=net0
-device virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56
```

driver bundleは`org.mochios.network.virtio-net`で、`drivers.service`が
`/bin/drivers/network/virtio-net.driver`から起動します。必要Capabilityは
`device.net`、`dma.allocate`、`ipc.server`です。

## 2. feature negotiation

必須feature:

- `VIRTIO_F_VERSION_1`
- `VIRTIO_NET_F_MAC`

任意feature:

- `VIRTIO_NET_F_STATUS`

driverは上記以外をdriver featureへ書きません。checksum offload、guest checksum、GSO、TSO、
UFO、mergeable RX buffer、control virtqueue、multiple queueは未対応です。

modern virtio 1.0のframe先頭には12 bytesの`virtio_net_hdr_v1`を使用します。
`num_buffers`を含まないlegacy用10 bytes headerは使用しません。全offload fieldは0です。

## 3. PCIとDMA

PCI configuration spaceは`0xcf8`/`0xcfc`経由で読み、virtio vendor capabilityからcommon、
notify、ISR、device configuration領域を取得します。必要なBARだけをpage単位でmapし、
register accessは範囲検査後のvolatile accessに限定します。

virtqueue memoryとpacket bufferは`dma.allocate`で取得します。descriptorへ渡すaddressは
DMA allocationが返したphysical addressであり、processのvirtual addressではありません。
DMA allocationは所有objectのDropまで保持します。

## 4. virtqueue

- queue 0: RX
- queue 1: TX
- 使用queue size上限: 64 descriptors
- 事前投入RX buffer: 32
- 固定TX buffer: 32
- driver内受信queue: 最大64 frames
- 最大Ethernet frame: 1514 bytes（MTU 1500）

RXはdeviceが返したused lengthについて、12-byte virtio header、14-byte Ethernet header、
buffer上限を検証します。処理後は同じDMA bufferをRX queueへ再投入します。不明なhead、
used indexの不正な進行、短いframeはerrorとして扱います。

TX bufferはused descriptorを回収するまで再利用しません。空きbufferまたはdescriptorがない場合は
`EAGAIN`相当を返し、追加allocationを行いません。queue error時はdevice statusへFAILEDを設定し、
通常処理を停止します。初期化開始時にはdevice resetを実行します。

現行driverは割り込みをbindせずpollingします。これは現在のdriver event機構に合わせた実装で、
multiple queueやinterrupt moderationは未実装です。

## 5. 上位IPC

wire formatは`mochios-net-device-protocol` v1です。すべてlittle-endianで、24-byte headerに
magic `MNET`、version、opcode、message length、reserved、request IDを持ちます。
構造体の生送信は使用しません。

提供operation:

- `GetInterfaceInfo`
- `TransmitFrame`
- `ReceiveFrame`
- `GetStatistics`

interface情報はinterface ID、MAC、link、MTU、driver name、driver ID、PCI device identifierを
含みます。frame長は1514 bytes以下に制限し、受信frameがない場合は`EAGAIN`を返します。

## 6. テストと診断

driver固有testはfeature集合とmodern header境界を固定します。共有PlugKitのmock transport/
virtqueue testsはfeature拒否、queue設定、RX再投入、TX回収、queue full、不正used chain、resetを
検証します。実機相当の疎通はQEMU Smoke TestでDHCP、ARP、ICMPまで確認します。

pcap取得方法は[mochiOS Networking](../networking.md)を参照してください。
