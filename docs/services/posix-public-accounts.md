# 公開アカウント情報とreentrant passwd API（段階226）

getpwuid_r / getpwnam_rを追加します。LLVMの実物のLLVMSupport.aをリンクした際、
Path.cppからこの二つへの未定義参照が出たことが実装の直接の契機です。
環境変数からUIDを捏造したり、rootの情報を固定で返したりしません。

user.serviceが保有するUserDatabaseを唯一の情報源とします。新しいpublic lookupの
wireにはUID / GID / name / display name / home / shellだけを載せます。
password_hashやlocked状態には対応するfieldがなく、Cのpw_passwdはxを返します。
既存のsnapshot、ユーザー変更、認証要求のCapability検査は維持しています。
公開lookupのみは秘密情報へのread Capabilityを要求しません。

service-managerがuser.serviceのREADYに添付されたSEND / DUPLICATE / TRANSFERの
Endpoint Handleを受け取り、保持したProcess Handleのidentityと送信者を照合します。
これを、LaunchContextで指定されたbootstrap processだけがposix.serviceへ登録できます。
別のroot process、自己申告のPID / UID、未知のHandle権限による差し替えは拒否します。
サービス名検索、private users.dbの直接読み込み、カーネルへのaccount policy追加はありません。

通常のクライアントはposix-clientのlookup_accountでUIDまたはnameを検索します。
POSIXサービスは公開lookupだけをuser.serviceへ転送し、request ID、wireの長さ、UTF-8、
field上限、NUL、error replyを検査します。lookup keyのUIDは検索データであり、
認証identityでも資格変更でもありません。書き込み要求や任意IPC opcodeは転送しません。

C APIはnewlibの56 byteのstruct passwdと呼び出し元bufferを使います。
pw_comment / pw_gecosは同じdisplay nameを参照します。所有する静的bufferはありません。
MemoryCopyで入力nameと出力先にアクセスし、未整列pointerをRust参照に変換しません。
容量不足はERANGE、pointer faultはEFAULT、未登録accountは0とNULL resultです。
サービスの停止やエラーを「該当ユーザーなし」に置き換えません。
返却規約は[POSIX getpwnam_r](https://pubs.opengroup.org/onlinepubs/009695099/functions/getpwnam.html)を参照してください。

ホストでは8 target / 513件が成功しました。公開wireの破損、秘密情報の非露出、
登録権限、endpoint所有者の照合、返信失敗時のHandle回収、Cの未整列出力とfault、
二つの独立bufferへの取得、サービス失敗の伝播を含みます。
記録はout/checks/mochios-passwd-226-host-final.logとmochios-passwd-226-owner.logです。

KVMでは通常の資格情報とseteuid(501)後のexecの両方で、Cから実際のUserDatabaseを
検索できました。UID / 名前による取得、private caller storage、ERANGE、EFAULT、
不在record、errno保持を確認しました。従来のRAM / rusage / C++ / auxiliary /
memory advice / wait試験も成功しています。mochios-passwd-226-native.logへ記録しました。

非reentrantのgetpwnam / getpwuidとアカウント列挙はこの段階には
含まれません。clang本体の起動・コンパイル成功、完全なPOSIX環境は未完了です。


診断用ELFを削除し、POSIX / accountサービスと通常managerを最終ソースで再ビルドして
イメージを再生成しました。通常KVM起動の最初の確認はIME初期化中にinput READYが
タイムアウトしました。ログをnormal-attempt1.logへ保存し、自分のLLVMビルドだけを
一時停止して同じイメージで再試行しました。再試行ではinput READY、account endpoint
handoff、Binder ready、デスクトップとDockを確認しました。
mochios-passwd-226-normal.log / normal.pngに保存し、VMを停止して同じLLVM jobを再開しました。

カーネルは段階224のままです。配布SDKとRust std overlayは再生成していません。
NICのないrunnerのnetwork Failed(-6)とUSB driver errno 22は従来どおり残ります。
