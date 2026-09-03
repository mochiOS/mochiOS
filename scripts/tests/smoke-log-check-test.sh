#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECKER="${SCRIPT_DIR}/../check-smoke-logs.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

SERIAL_LOG="${TMP_DIR}/serial.log"
SERVICE_MANAGER_LOG="${TMP_DIR}/service-manager.log"
DRIVERS_LOG="${TMP_DIR}/drivers.log"
NETWORK_LOG="${TMP_DIR}/network.log"

write_valid_logs() {
    cat > "${SERIAL_LOG}" <<'EOF'
[INFO] exec: loaded '/init' from initfs
[INFO] exec: loaded '/system/services/capability.service' from cext
[INFO] exec: loaded '/system/services/service-manager.service' from cext
[INFO] exec: loaded '/system/services/drivers.service' from cext
[INFO] exec: loaded '/system/services/input.service' from cext
[INFO] exec: loaded '/system/services/display.driver' from cext
[INFO] exec: loaded '/system/services/compositor.service' from cext
[INFO] exec: loaded '/bin/drivers/ps2/i8042.driver/entry.elf' from cext
[INFO] exec: loaded '/bin/drivers/network/virtio-net.driver/virtio-net.driver' from cext
[INFO] exec: loaded '/system/services/network.service' from cext
[INFO] exec: loaded '/system/services/user.service' from cext
[INFO] exec: loaded '/system/services/secure-ui.service' from cext
[INFO] exec: loaded '/system/services/linux.service' from cext
[INFO] exec: loaded '/applications/Binder.app/entry.elf' from cext
EOF
    cat > "${SERVICE_MANAGER_LOG}" <<'EOF'
service-manager.service: start
service-manager.service: drivers.service spawned pid=8
service-manager.service: registered drivers.service as driver delegate
service-manager.service: waiting for drivers.service hello
service-manager.service: drivers.service hello received
service-manager.service: input.service spawned pid=9
service-manager.service: display.driver spawned pid=10
service-manager.service: waiting for display.driver ready
service-manager.service: display.driver ready
service-manager.service: waiting for input.service ready
service-manager.service: input.service ready
service-manager.service: compositor.service spawned pid=11
service-manager.service: driver discovery requested
service-manager.service: driver discovery complete
service-manager.service: network.service spawned pid=13
service-manager.service: user.service spawned pid=14
service-manager.service: waiting for user.service ready
service-manager.service: user.service ready
service-manager.service: secure-ui.service spawned pid=15
service-manager.service: waiting for secure-ui.service login
service-manager.service: secure-ui.service login complete
service-manager.service: linux.service spawned pid=16
service-manager.service: Binder.app spawned pid=17
EOF
    cat > "${DRIVERS_LOG}" <<'EOF'
drivers.service: start
drivers.service: matched bundle=/bin/drivers/usb/qemu-usb.driver package=org.mochios.usb.qemu root=/bin/drivers/usb
drivers.service: spawn failed /bin/drivers/usb/qemu-usb.driver/entry.elf errno=22
drivers.service: matched bundle=/bin/drivers/ps2/i8042.driver package=org.mochios.ps2.i8042 root=/bin/drivers/ps2
drivers.service: spawned driver pid=12
drivers.service: active bundle=/bin/drivers/ps2/i8042.driver
drivers.service: matched bundle=/bin/drivers/network/virtio-net.driver package=org.mochios.network.virtio-net root=/bin/drivers/network
drivers.service: spawned driver pid=13
drivers.service: active bundle=/bin/drivers/network/virtio-net.driver
EOF
    cat > "${NETWORK_LOG}" <<'EOF'
network.service: interface id=1 mac=52:54:00:12:34:56 link=true mtu=1500
network.service: DHCPDISCOVER sent
network.service: DHCPOFFER received
network.service: DHCPREQUEST sent
network.service: DHCPACK received
network.service: configured ip=10.0.2.15
network.service: gateway ARP resolved ip=10.0.2.2
network.service: ICMP Echo Reply from 10.0.2.2
EOF
}

expect_failure() {
    local name="$1"
    local expected="$2"
    local output="${TMP_DIR}/${name}.out"

    if "${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 0 1 > "${output}" 2>&1; then
        echo "fatal: ${name} unexpectedly passed" >&2
        exit 1
    fi
    grep -Fq "${expected}" "${output}" || {
        echo "fatal: ${name} did not report '${expected}'" >&2
        cat "${output}" >&2
        exit 1
    }
}

append_mpkg_success() {
    cat >> "${SERIAL_LOG}" <<'EOF'
/ $ mpk /system/samples/mpk-test.mpkg
[INFO] execve: loaded '/bin/mpk' from cext
Process exiting with code: 0
EOF
}

write_valid_logs
"${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 0 1 >/dev/null

append_mpkg_success
"${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 0 1 0 0 1 >/dev/null

write_valid_logs
sed -i '/matched bundle=\/bin\/drivers\/usb\//d' "${DRIVERS_LOG}"
sed -i '/spawn failed \/bin\/drivers\/usb\//d' "${DRIVERS_LOG}"
"${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 0 0 >/dev/null

write_valid_logs
cat >> "${SERIAL_LOG}" <<'EOF'
/ $ net resolve localhost
localhost -> 127.0.0.1
/ $ net tcp-connect 10.0.2.2 20000
Connected to 10.0.2.2:20000 (10.0.2.2)
/ $ net tcp-send 10.0.2.2 20000 mochios-tcp-smoke
Connected to 10.0.2.2:20000 (10.0.2.2)
sent=17 received=17 data=mochios-tcp-smoke
EOF
cat >> "${NETWORK_LOG}" <<'EOF'
network.service: DNS query sent name=localhost attempt=1
network.service: DNS response received name=localhost
network.service: DNS resolved name=localhost address=127.0.0.1
network.service: TCP SYN sent
network.service: TCP SYN+ACK received
network.service: TCP Established remote=10.0.2.2:20000
network.service: TCP FIN close complete
network.service: TCP SYN sent
network.service: TCP SYN+ACK received
network.service: TCP Established remote=10.0.2.2:20000
network.service: TCP payload acknowledged bytes=17
network.service: TCP payload received bytes=17
network.service: TCP FIN close complete
EOF
"${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 1 1 >/dev/null

cat >> "${NETWORK_LOG}" <<'EOF'
network.service: TCP SYN sent
network.service: TCP SYN+ACK received
network.service: TCP Established remote=203.0.113.10:443
network.service: TCP FIN close complete
EOF
"${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 1 1 >/dev/null

sed -i '/TCP payload received/d' "${NETWORK_LOG}"
if "${CHECKER}" "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" 1 1 \
    > "${TMP_DIR}/network-missing.out" 2>&1; then
    echo "fatal: missing network lifecycle log unexpectedly passed" >&2
    exit 1
fi
grep -Fq "wrong log count 'TCP payload receive'" "${TMP_DIR}/network-missing.out"

write_valid_logs
sed -i '/service-manager.service: display.driver ready/d' "${SERVICE_MANAGER_LOG}"
expect_failure "missing" "missing required log 'display ready'"

write_valid_logs
printf '%s\n' 'service-manager.service: input.service spawned pid=99' >> "${SERVICE_MANAGER_LOG}"
expect_failure "duplicate" "duplicate log 'input spawn'"

write_valid_logs
sed -i '/service-manager.service: Binder.app spawned pid=/d' "${SERVICE_MANAGER_LOG}"
sed -i '/service-manager.service: waiting for secure-ui.service login/a service-manager.service: Binder.app spawned pid=16' "${SERVICE_MANAGER_LOG}"
expect_failure "desktop-before-login" "order violation in service-manager.service log"

write_valid_logs
sed -i '/service-manager.service: display.driver ready/d' "${SERVICE_MANAGER_LOG}"
sed -i '/service-manager.service: input.service ready/a service-manager.service: display.driver ready' "${SERVICE_MANAGER_LOG}"
expect_failure "order" "order violation in service-manager.service log"

write_valid_logs
printf '%s\n' '[ERROR] kernel panic: test fixture' >> "${SERIAL_LOG}"
expect_failure "panic" "forbidden log 'boot failure'"

write_valid_logs
printf '%s\n' 'memory allocation of 1584 bytes failed' >> "${SERIAL_LOG}"
expect_failure "allocation" "forbidden log 'boot failure'"

write_valid_logs
printf '%s\n' 'Error: MochiOs(Syscall(13))' >> "${SERIAL_LOG}"
expect_failure "viewkit" "forbidden log 'boot failure'"

write_valid_logs
printf '%s\n' "[WARN] execve: signature verification failed for '/bin/test'" >> "${SERIAL_LOG}"
expect_failure "signature" "forbidden log 'signature failure'"

echo "[test] smoke log checker fixtures passed"
