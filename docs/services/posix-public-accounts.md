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

## 通常版passwd APIとC ABIの修正（340）

Vim 9.2のリンクで必要になったgetpwuid / getpwnamを、既存の公開account照会へ接続しました。
情報源は引き続きuser.serviceのUserDatabaseです。非reentrant版はprocess-localな所有bufferを
共有し、次の照会で上書きされ得ます。RPCはSTATEをlockする前に完了し、結果だけを既存の
RuntimeStateへ保存します。別の継承mutexを増やさず、既存のfork時の状態保護を使います。
入力が直前に返したpw_nameを参照している場合も、上書き前にcaptureします。

newlibのx86_64-elfヘッダーではuid_t / gid_tはどちらもunsigned shortです。
従来はRust側をu32としていたため、全体sizeとpw_uidのoffsetが一致していてもpw_gidの場所が
ずれていました。公開型をu16、pw_gidをoffset 18へ修正し、wireのu32からの変換が範囲を超える
場合はEOVERFLOWとします。桁を落として別のUID/GIDとして公開しません。

関連4 targetsのhost試験19件が成功しました。実際のaccount RPC codecとruntime実装を使い、
UID/GID配置、独立したcaller buffer、範囲外IDでの出力保持、process-local cacheの再利用、
不在record、errno、サービス失敗と不正pointerを確認しています。
hostのMemoryCopy adapterは自processのprocess_vm_readvを使うため、sandboxのEPERM後に
同じ試験を許可された環境で再実行しました。fault検査の代替や省略はしていません。

mochiOS向けの独立runtime archiveのbuildと、強化したnative_passwd.cのC ABI compileも
成功しました。native実行はVimの統合試験と合わせて続けます。SDKとRust std overlayは
読み取り入力のまま再生成していません。Vimはグループ照会の不足が残り、まだリンク・起動の
成功としては扱いません。記録はout/checks/mochios-passwd-340-host.logとnative-build.logです。

## 永続グループ情報とC API（341〜343）

getgrgid / getgrnamとそれぞれの_r版を、posix-client → posix.service → 委譲済みの
user.service endpointへ接続しました。GIDや名前から返却値を捏造せず、user.serviceが
所有するgroups.dbを照会します。読み取り専用の公開wireにグループ名・GID・明示的な
追加メンバーだけを載せ、秘密情報・資格変更・native Capabilityの付与は行いません。

groups.dbは/var/lib/accounts以下の0600の永続ファイルです。初回だけ実アカウントの
UID=GIDのprivate groupを作成・保存してから公開します。既存の共有グループとメンバーは
権威ある設定として保持し、壊れたDBやI/Oエラーで初期データを再生成しません。
原子的保存と回復候補の扱いはusers.dbと共有します。ユーザー追加時はprivate groupを
先に永続化するため、後続のユーザー保存に失敗しても未割り当てのグループ情報だけが残ります。
グループ保存が失敗した場合は照会・追加を停止し、不確かな状態を成功として公開しません。

新wireは最大64メンバー、名前32byte、レコード2216byteに制限します。重複ID・名前・メンバー、
過長データ、request ID不一致、壊れた返信は拒否します。Cのstruct groupはnewlibの
32byte ABIに合わせ、gid_tはu16、gr_gidはoffset16、gr_memはoffset24です。
呼び出し元のbufferに文字列と整列したNULL終端のメンバー配列を作り、MemoryCopyを通して
公開します。サービスエラー、EFAULT、ERANGE、EOVERFLOWを不在recordへ置き換えません。
通常版のcacheはpasswdとは別で、既存のRuntimeStateとfork保護を使います。

最初の実機試験で追加opcode136と既存read_scopeの衝突を検出しました。未使用の144へ修正し、
同じPOSIX endpointの全モジュールで番号重複を検出する回帰試験を追加しました。
ホスト5 targets / 28件が成功しました。KVMでも実際のC APIからUID・GID・名前照会、
独立buffer、cache、errno、不在record、範囲不足、不正pointerを確認しています。
実Vim 9.2のUTF-8編集・保存・再読み込み試験も成功しました。
記録はout/checks/mochios-groups-343-host.logとmochios-vim-343-batch.pngです。

グループの列挙API、initgroups、グループ管理RPCは未実装です。追加メンバー一覧の照会は
POSIX processのsupplementary groupsを自動変更しません。SDKとRust std overlayは再生成せず、
今回の互換APIを含む独立runtimeとサービスだけをビルドしています。
