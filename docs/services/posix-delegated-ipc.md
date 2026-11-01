# 委譲EndpointによるIPC（328）

IpcCall、IpcCallObjectHandles、IpcSendObjectHandlesは、呼出元のHandle tableにある
有効なEndpointのSEND権限を、その宛先への送信権限として使います。
追加のipc.client / ipc.serverは要求しません。型、権限、Endpointのslotとgeneration、
宛先の寿命、添付HandleのTRANSFERと要求権限の包含関係は従来どおり検査します。

グローバルなEndpoint IDを使う同じ3操作は、ipc.client / ipc.serverを引き続き要求します。
この検査はグローバル表の検索より前に行います。IpcSendの既存の権限判定も維持します。
送信元はカーネルが確定し、同期応答は対象generationとcall IDへ結び付けます。
利用者がdelivery_flagsを指定することはできず、Provider由来の配送を偽装できません。

カーネルへPOSIX、サービス名、配置パス、FD番号の知識は追加していません。
POSIX endpointも他のサービスと同じHandle委譲を使います。名前付きCapabilityを
落としても、保持する個別参照の権限が消えるわけではありません。参照のclose、
宛先の終了・generation失効、通常のCapabilityの取り消しはそれぞれの規則に従います。

## 検証

変更前に、委譲SENDでの同期呼出し・添付送信の試験がAccessDeniedとなることを確認しました。
変更後は関連12 targets / 18 host testsが成功しています。実際のカーネル処理を使い、
グローバル宛先の拒否と検索前の判定、権限不足・異なる型・古いgeneration、添付の縮小、
配送情報の偽装拒否、応答の認証、queue pressureだけの再試行と失敗時の回収を検査します。
記録はout/checks/mochios-delegated-ipc-328-のbefore.log、before-calls.log、host-final.logです。

既存のshell receive試験は次の関数までを文字列で取り込んでいたため、新しく間に入った
別関数まで取り込んでコンパイルに失敗していました。対象関数の終端までに抽出を限定する
修正を独立コミットし、エラーを保持する既存の検査内容は維持しました。

native_delegated_ipcは、子から明示的に添付されたSEND / WAITだけのEndpoint参照を受け取り、
親がipc.client / ipc.serverを両方落としてから通信する試験です。親が保持するその他の
Capabilityを全て削除する試験や、通常execでの最終権限付与の完成を意味しません。
署名検証付き共通実行認可、保護された同意と最終grantの結合、完全POSIX環境のgoalは継続します。

更新カーネルで通常のBinder、Dock、Terminal / mshを起動し、この試験の全項目が成功しました。
通常の同期呼出し、READだけのRAM添付と応答、書込・複製・再転送の拒否、非同期送信、
IPC CapabilityなしのPOSIX wait、宛先終了後の失効、名前付きIPC権限が復活していないことを
確認しています。グローバル宛先、SENDなし、偽のnative配送情報も拒否しました。
PASSは同接頭辞のnative.log、msh promptへの復帰はresult.pngに記録しています。

所有VMのPIDと開始時刻を照合して停止し、一時停止の記録も復帰しました。
試験用プログラム1個だけを撤去し、検証済みカーネルを保持して通常イメージを再生成しています。
401ファイルと4つのcommand symlinkの読み戻し、probeの不在、6成果物のchecksumを確認しました。
SDKとRust std overlayは再生成していません。既存のnetwork Failed(-6)とUSB errno 22は残ります。
