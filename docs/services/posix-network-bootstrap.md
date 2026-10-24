# Network endpointのboot handoff（段階212）

[UDP共有description](posix-inet-datagram-description.md)をsocket workerへ接続する前提として、
信頼するnetwork endpointをPOSIX層へ渡す起動経路を追加しました。
クライアントが任意のendpointを指定する構造にはしていません。

## 起動経路

network.serviceは既存service-readyのHandle通知を使い、自分のcontrol endpointを
SEND / DUPLICATE / TRANSFERに縮小してservice-managerへ送ります。
NIC不在でも同じendpointがloopbackとauthority要求を処理するため、endpointは渡します。
通知statusは従来どおり-6で、物理networkのREADYへ読み替えません。
endpoint初期化そのものに失敗した場合は、Handleを付けず失敗だけを通知します。

service-managerはnetwork通知が他サービスの待機中に届いても参照を保持します。
READY tokenだけでidentityを判断せず、native IPCから得た送信元process IDを保持し、
起動済みnetwork Process Handleのstatusから得たIDと照合してから採用します。
一致しない場合は受信参照をcloseします。重複通知・不正rights・正のstatusも拒否します。
これはユーザー空間の起動policyです。kernelにservice名や特別なnetwork interfaceは追加しません。

次にservice-managerが既存のPOSIX endpointへOP_NETWORK_ENDPOINT_INSTALL（128）を呼び、
受け取ったnetwork endpointをnative IPCで明示的に添付します。
payloadは空で、PID・FD・path・service名・生のHandle値は入れません。
送信側は一度だけ呼び、成功・失敗に関わらず自分の受信参照をcloseします。
クライアント側は異常応答に含まれた参照を回収し、借用した元のendpointはcloseしません。

POSIX側の登録権限は、起動時に継承したbootstrap Process Handleが指すprocessに限ります。
元から登録されているsessionのeffective UIDも0であることを確認します。
別のuid 0 processや自己申告のPeerIdentityのUID値では登録できません。
登録は一度だけで、再登録によるendpointの差し替えは拒否します。
正常時だけ受信Handleを保持し、失敗時は共通の受信参照cleanupへ戻します。

POSIX層のnetwork_endpoint()はservice lifetimeの借用参照です。
endpointを持つことと、ソケット用factoryを持つことは別です。
クライアントのnet.connectを代わりに取得したり、network Capabilityを付与したりはしません。

## 検証

関連9 targets・48 testsが成功しました。実際のinstall handlerとclient codecを使い、
bootstrap process限定の認証、別のroot processの拒否、二重登録、不正添付、失敗時のcleanup、
READY tokenが正しくてもnative送信元が異なる通知の拒否、early到着とFailed(-6)の保持を確認しました。

posix.service、network.service、service-managerのmochiOS向けtarget buildが成功しました。
SDK / Rust std overlayは再生成していません。
最初の起動では古いrootfsにkeychain.serviceがなく、現在のservice-managerが起動を中断しました。
既存Keychain依存のbinary・manifestを配置し、現在のCapability定義へ揃えるために
既存capability.serviceとkernelを再ビルドしました。kernelのソース変更や新規ABI追加はありません。
KVM（NICなし）でnetwork → service-manager → POSIXのnative Handle転送を確認しました。
service-managerの成功応答ログ、従来どおりのnetwork Failed(-6)、input READY、Binder起動、
通常デスクトップとDockの表示を確認しました。既存USB driver errno 22も残っているため、
全サービス正常とは扱いません。診断用のアプリ・manifest差し替えはなく、VMは停止しました。

記録はout/checks/mochios-network-bootstrap-212-{final-host,posix-build,network-build,manager-build}.log、
-native.log、-normal.png、-artifacts.json、-evidence.jsonです。

署名付き実装コミットはuser cbff8f3、services 4843e9eです。
再ビルドで生成されたcoreのlock更新も00c2277へ保存しています。

## 残る実装

クライアント側へのnetwork endpoint配布とfactory取得、認証済みsocket作成要求へのfactory添付、
socket workerへのUDP description登録、Provider control / record transport / readiness中継、
libc AF_INETの接続と実統合試験は引き続き必要です。
この変更だけでAF_INET socketが動くとは扱いません。POSIX全体のgoalは継続中です。
