# 暗号学的乱数

## entropyとDRBG

kernelは起動時に次の独立した材料をSHA-256で混合し、ChaCha20 DRBGを初期化します。

1. UEFI RNG Protocolから得た256-bit seed（全0は無効）
2. CPUが対応する場合の8個のRDRAND sample
3. RDTSC、RTCを含むboot-varying stateとkernel内address state

RDRAND sampleが連続して同値ならhealth failureです。UEFI RNGが使えない場合は8 sampleすべての
RDRAND取得を必須とし、どちらも成立しなければCSPRNGを未初期化のままにします。起動時刻だけ、固定seed、
非暗号PRNGをseedにしません。入力seedはChaCha20Rngへ渡した直後にzeroizeします。

UEFI RNGが使える場合もRDRANDを取得できれば追加混合しますが、RDRANDの出力をそのままDRBG seedには
しません。内部state、seed、生成した秘密乱数をlogまたはuserspaceへ公開しません。

## userspace API

`random_fill(&mut [u8])`は`RandomFill` syscallを使用し、kernel CSPRNGから指定bufferだけを埋めます。
このsyscallには`system.random.read` Capabilityが必要です。内部poolやDRBG stateを取得するAPIは
ありません。network.serviceはTLS handshake、ephemeral key、connection handleにこのAPIを使います。

CSPRNG未初期化時は`EAGAIN`です。部分成功として扱わず、TLSは`RandomUnavailable`でfail closedします。
kernelはuserspaceへのcopyを256-byte chunkで行い、一時bufferを完了時と失敗時に0で上書きします。

既存POSIX互換`getrandom` syscallも同じCSPRNGを使用します。TLS本番経路はCapability制御された
`RandomFill`だけを使用します。

## 検証と制限

共有`mochios-csprng` crateの単体テストは未初期化拒否、生成成功、stream進行、seed zeroizeを確認します。
QEMU runnerはUEFI RNGに加えて`virtio-rng-pci`を接続し、boot logでUEFI seed取得を確認できます。

現行実装は起動後の外部entropy再注入と定期reseedを行いません。ChaCha20 DRBGのstateはkernel内部だけで
進行します。将来の実機対応ではplatform hardware RNGとreseed policyを追加監査する必要があります。

