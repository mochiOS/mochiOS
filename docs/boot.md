# Bootとサービス起動

`core.service`は`logger.service`、`capability.service`を起動します。Capability policy適用後、
`capability.service`が`signature.service`、`package.service`、`service-manager.service`を起動します。

`service-manager.service`の固定起動順は次です。

```text
drivers.service
input.service
display.driver
display Ready
input Ready
compositor.service
Driver discovery
network.service
tty.service
network Ready
update.service
resident
```

`update.service`はnetwork Ready後にだけ起動します。起動または同期の失敗はbootを止めず、保存済み
Developer PKI databaseを使用します。`signature.service`はnetwork Readyより前に起動し、ローカルDBを
読み込みます。新しいDBがない場合もserverとして常駐しますが、新規MPKG installはfail closedです。

正式な`make smoke-test`はサービス起動順、重複起動、Ready順、Driver探索、network lifecycle、
`update.service`起動、panic/exception不在をログ行番号と出現回数で検証します。xHCIがbuild設定で無効な
場合はUSB bundleを要求せず、PS/2とvirtio-net以降の順序を引き続き検証します。

