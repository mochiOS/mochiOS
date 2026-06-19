# ファイルツリー

```
/
├─ applications/ <- アプリを格納
│  └─ Example.app/ <- アプリバンドル（ディレクトリ）
│     ├─ about.toml <- アプリのメタ情報
│     ├─ manifest.toml <- Capabilityなど
│     └─ {entry}
├─ bin/ <- coreutilsなどcliツールのバイナリを配置
├─ libraries/ <- ライブラリを配置
│  ├─ include/ <- libcヘッダ配置
│  ├─ extensions/ <- PlugKitドライバ配置
│  └─ fonts/ <- フォント
├─ var/ <- 変動データ配置
│  └─ appdata/ <- 署名対象に入らないアプリの変動データを配置
│     └─ {Example.appのBundleID}
├─ system/ <- カーネルとかサービスとか
│  ├─ mnu(binary)
│  ├─ config/ <- kernel.confなど
│  └─ services/ <- サービス
├─ mnt/ <- マウントしたやつ配置
├─ home/ <- ホームディレクトリ
│  └─ User001/
│     ├─ Documents/
│     ├─ Desktop/
│     ├─ Movies/
│     ├─ Pictures/
│     ├─ Downloads/
│     ├─ Audio/
│     ├─ Develop/
│     └─ Libraries/ <- ユーザーごとのアプリデータ、ユーザーフォント
│        ├─ fonts/
│        └─ appdata/
│           └─ {bundleID}
└─ tmp/ <- tmp
```

また、posix.serviceにより、Linux互換のファイルシステムが仮想的に提供されますが、これはlsしても見えません。アクセスされたときに、仮想的にファイルツリーが提供されます。
例えば、/procや/sysなどのLinux互換のディレクトリは、posix.serviceがアクセスされたときに仮想的にハンドラが提供されます。