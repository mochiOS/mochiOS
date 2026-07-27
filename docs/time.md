# 時刻

## clockの分離

mochiOSは用途の異なる2種類のclockを分離します。

| clock | 取得元 | 用途 |
| --- | --- | --- |
| monotonic | kernel timer tick | timeout、TCP再送、DNS retry、RTT、sleep |
| UTC realtime | 起動時RTC UTC + 起動後monotonic elapsed | X.509 `notBefore` / `notAfter` |

timeoutへUTCを使わず、証明書検証へbootからのtick値を使いません。

## UTC初期化

kernelは起動時にCMOS RTCをUpdate-In-Progress外で2回読み、同じsnapshotが得られた場合だけ採用します。
BCD/binary、12/24-hour、PM bit、century registerを解釈し、`mochios-time-core`でGregorian dateを
Unix secondsへ変換します。許可するyearは2020から2099です。月、閏日、日、時、分、秒を検証します。

runnerはQEMUを`-rtc base=utc`で起動するため、RTCはUTCです。local timezoneへ変換しません。起動時の
Unix secondsとmonotonic tickをbaseとして保存し、起動後はtick差を加算します。

RTCが安定しない、year/dateが不正、timer frequencyが無効な場合はrealtimeを未初期化にし、
`CLOCK_REALTIME`は`EAGAIN`を返します。時刻検証を無効化するfallbackはありません。rustlsの
`PlatformTimeProvider`もこの場合`None`を返し、Web PKI検証は`TimeUnavailable`でfail closedします。

## APIとCapability

`mochi_user_platform::time::ticks()`はmonotonic tick、`utc_seconds()`はCapability制御された
`CLOCK_REALTIME`です。UTC読み取りには`system.time.read`が必要で、network.serviceへだけ明示します。
診断applicationが通常のtimeoutを使うためにUTC Capabilityを持つ必要はありません。

`CLOCK_MONOTONIC`、process/thread CPU time互換値は従来どおりtickを返します。realtimeとmonotonicの
意味をAPI上で混在させません。

## 検証と制限

単体テストは正常UTC変換、閏年、不正date/year拒否、monotonic elapsedとの分離を確認します。TLS fixtureは
期限前、期限切れ、UTC取得失敗をそれぞれ拒否します。Accounts E2EはQEMU RTCの実UTCで公開証明書を
検証します。

現行実装にNTP、RTC再同期、clock set、leap second表、時刻逆行補正はありません。長時間稼働時のRTC drift
補正も未対応です。

