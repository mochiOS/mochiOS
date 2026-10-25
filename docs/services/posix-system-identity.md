# POSIXのシステム情報とホスト名

`uname`、`gethostname`、`sethostname`は`posix-client`と`posix.service`で
実装する。カーネルにはシステム名、release manifest、ホスト名、POSIX namespaceの
知識を追加しない。

## 情報源と権限

- systemは`mochiOS`。releaseはルートの`version.toml`の`release`、versionは
  同じファイルのbuild番号・channel・build_dateからビルド時に生成する。
  machineはサービスの`CARGO_CFG_TARGET_ARCH`。ビルドホストのunameは使用しない。
- `services/posix/build.rs`はmanifestの欠落、必須値の欠落・重複、不正なbuild番号、
  長すぎる値をビルドエラーにする。manifestの現在の単純なquoted scalar形式を扱い、
  未対応のescapeや複雑な文字列を推測して解釈しない。
- サービスの初期ホスト名は`localhost`。ホスト名はサービスの共有namespaceの状態であり、
  設定はfork/execしたプロセスを含め同じendpointの利用者に反映される。
  このAPIの変更は永続化せず、サービスを再起動すると初期値へ戻る。
- 問い合わせには登録済みPOSIX環境が必要。変更は、IPCが保証した送信元process IDを
  サービスのsessionへ照合し、そのsessionの実効UIDが0の場合だけ許可する。
  requestやPeerIdentityの自己申告UIDは使用しない。
  hostnameの変更はPOSIX namespaceだけに作用し、native resourceの権限や
  ネットワークの認証identityを変更しない。

## ABI

- opcode 134はsystem identityの取得、135はホスト名の変更。
- 応答は長さ付きbyte stringを5つ連結する。Cの構造体やポインタはIPCへ載せない。
  各fieldは最大255bytes、内部NULと余分な末尾bytesは拒否する。
- Cの`struct utsname`は256bytesのchar配列5つ、合計1280bytes。
  headerとRust libcの定義を揃え、返却時にはNULと残りの領域をゼロにする。
- `gethostname`は不足したbufferを変更せず、ENAMETOOLONGを返す。
  `sethostname`は指定された長さだけをfaultable copyで読み、255bytesを超える値や
  内部NULをEINVALとする。空のホスト名は受け付ける。
- `_SC_HOST_NAME_MAX`は255。成功した操作はerrnoを変更しない。
  不正なpointer、サービス停止、不正応答を成功や固定文字列へ置換しない。

POSIXが定めるunameの5項目は
[unameの仕様](https://pubs.opengroup.org/onlinepubs/009604599/functions/uname.html)を参照。

## 検証

`services/posix/tests/system_identity.rs`、`system_identity_runtime.rs`と既存protocol試験の
計507テストがホストで成功した。release情報とmanifestの対応、境界長、欠落metadata、
資格情報の偽装、共有状態、C ABIの全field、fault、短いbuffer、不正なサービス応答を含む。
runtime試験では実際のclient・serviceを接続し、C ABIの出力を確認する。

`native_system_identity.c`はKVM上でroot/euid 501の両方からAPIを呼んだ。
実IPCでの名前変更、fork先への反映、非rootの変更拒否、最大長、pointer fault、
sysconfとC構造体を確認した。名前は試験前の値へ戻す。
TLS、実LLVMSupport、ELF PHDRの既存統合試験も成功した。

証跡は`out/checks/mochios-system-identity-230-{host-results,native}.log`。
SDKとRust std overlayは再生成していない。

診断ELFを外した通常起動でもinput READY、Binder ready、Dockの表示を確認し、
検証用VMを停止した。通常起動のログと画面は
`out/checks/mochios-system-identity-230-normal.{log,png}`。
既存のNetwork readiness失敗(-6)は引き続き発生している。今回の修正対象には含めていない。
