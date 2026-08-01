#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 6 || "$#" -gt 9 ]]; then
    echo "usage: $0 <serial-log> <service-manager-log> <drivers-log> <network-log> <network-client-smoke> <xhci-enabled> [tls-http-client-smoke] [accounts-https-smoke] [mpkg-runtime-smoke]" >&2
    exit 2
fi

SERIAL_LOG="$1"
SERVICE_MANAGER_LOG="$2"
DRIVERS_LOG="$3"
NETWORK_LOG="$4"
NETWORK_CLIENT_SMOKE="$5"
XHCI_ENABLED="$6"
TLS_HTTP_CLIENT_SMOKE="${7:-0}"
ACCOUNTS_HTTPS_SMOKE="${8:-0}"
MPKG_RUNTIME_SMOKE="${9:-0}"

die() {
    echo "fatal: $*" >&2
    exit 1
}

case "${NETWORK_CLIENT_SMOKE}" in
    0|1) ;;
    *) die "network-client-smoke must be 0 or 1" ;;
esac
case "${XHCI_ENABLED}" in
    0|1) ;;
    *) die "xhci-enabled must be 0 or 1" ;;
esac
case "${TLS_HTTP_CLIENT_SMOKE}" in
    0|1) ;;
    *) die "tls-http-client-smoke must be 0 or 1" ;;
esac
case "${ACCOUNTS_HTTPS_SMOKE}" in
    0|1) ;;
    *) die "accounts-https-smoke must be 0 or 1" ;;
esac
case "${MPKG_RUNTIME_SMOKE}" in
    0|1) ;;
    *) die "mpkg-runtime-smoke must be 0 or 1" ;;
esac

for log in "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}"; do
    [[ -f "${log}" ]] || die "log file not found: ${log}"
done

require_once() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    local matches
    local lines

    matches="$(grep -aFnF -- "${pattern}" "${file}" || true)"
    if [[ -z "${matches}" ]]; then
        die "missing required log '${label}': pattern='${pattern}' file=${file}"
    fi
    mapfile -t lines <<< "${matches}"
    if [[ "${#lines[@]}" -ne 1 ]]; then
        die "duplicate log '${label}': lines=$(printf '%s,' "${lines[@]%%:*}" | sed 's/,$//') pattern='${pattern}' file=${file}"
    fi
    printf '%s\n' "${lines[0]%%:*}"
}

require_count() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    local expected="$4"
    local actual

    actual="$(grep -aFc -- "${pattern}" "${file}" || true)"
    [[ "${actual}" -eq "${expected}" ]] ||
        die "wrong log count '${label}': expected=${expected} actual=${actual} pattern='${pattern}' file=${file}"
}

require_at_least() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    local minimum="$4"
    local actual

    actual="$(grep -aFc -- "${pattern}" "${file}" || true)"
    [[ "${actual}" -ge "${minimum}" ]] ||
        die "insufficient log count '${label}': minimum=${minimum} actual=${actual} pattern='${pattern}' file=${file}"
}

assert_absent() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    local matches

    matches="$(grep -aFnF -- "${pattern}" "${file}" || true)"
    [[ -z "${matches}" ]] ||
        die "forbidden log '${label}': lines=$(printf '%s\n' "${matches}" | cut -d: -f1 | paste -sd, -) pattern='${pattern}' file=${file}"
}

assert_absent_case_insensitive() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    local matches

    matches="$(grep -aFinF -- "${pattern}" "${file}" || true)"
    [[ -z "${matches}" ]] ||
        die "forbidden log '${label}': lines=$(printf '%s\n' "${matches}" | cut -d: -f1 | paste -sd, -) pattern='${pattern}' file=${file}"
}

assert_order() {
    local scope="$1"
    local file="$2"
    shift 2
    local -a labels=()
    local -a lines=()
    local expected=""
    local actual=""
    local label
    local pattern
    local line
    local index

    while [[ "$#" -gt 0 ]]; do
        label="$1"
        pattern="$2"
        shift 2
        line="$(require_once "${label}" "${file}" "${pattern}")"
        labels+=("${label}")
        lines+=("${line}")
    done

    for index in "${!labels[@]}"; do
        if [[ -n "${expected}" ]]; then
            expected+=" -> "
            actual+=", "
        fi
        expected+="${labels[index]}"
        actual+="${labels[index]}=${lines[index]}"
        if [[ "${index}" -gt 0 && "${lines[index]}" -le "${lines[index - 1]}" ]]; then
            die "order violation in ${scope}: expected ${expected}; actual lines: ${actual}"
        fi
    done
}

for fatal_pattern in "PAGE FAULT" "Faulting user context:" "EXCEPTION:" "panicked at" "kernel panic" "panic:"; do
    assert_absent_case_insensitive "boot failure" "${SERIAL_LOG}" "${fatal_pattern}"
done

for signature_pattern in \
    "signature allow test" \
    "signature deny test" \
    "signature verification failed" \
    "execution allowlist: missing /libraries/system/execution.allowlist" \
    "execution allowlist: invalid /libraries/system/execution.allowlist"; do
    assert_absent_case_insensitive "signature failure" "${SERIAL_LOG}" "${signature_pattern}"
done

assert_order "serial boot log" "${SERIAL_LOG}" \
    "core.service" "exec: loaded 'core.service' from initfs" \
    "capability.service" "exec: loaded '/system/services/capability.service'" \
    "service-manager.service" "exec: loaded '/system/services/service-manager.service'" \
    "drivers.service" "exec: loaded '/system/services/drivers.service'" \
    "input.service" "exec: loaded '/system/services/input.service'" \
    "display.driver" "exec: loaded '/system/services/display.driver'" \
    "compositor.service" "exec: loaded '/system/services/compositor.service'" \
    "i8042 driver" "exec: loaded '/bin/drivers/ps2/i8042.driver/entry.elf'" \
    "virtio-net driver" "exec: loaded '/bin/drivers/network/virtio-net.driver/virtio-net.driver'" \
    "network.service" "exec: loaded '/system/services/network.service'" \
    "Binder.app" "exec: loaded '/applications/Binder.app/entry.elf'" \
    "update.service" "exec: loaded '/system/services/update.service'"

assert_order "network state transitions" "${NETWORK_LOG}" \
    "virtio-net ready" "network.service: interface id=1 mac=52:54:00:12:34:56 link=true mtu=1500" \
    "DHCP discover" "network.service: DHCPDISCOVER sent" \
    "DHCP offer" "network.service: DHCPOFFER received" \
    "DHCP request" "network.service: DHCPREQUEST sent" \
    "DHCP ack" "network.service: DHCPACK received" \
    "IPv4 configured" "network.service: configured ip=" \
    "gateway ARP" "network.service: gateway ARP resolved ip=10.0.2.2" \
    "ICMP reply" "network.service: ICMP Echo Reply from 10.0.2.2"

if [[ "${NETWORK_CLIENT_SMOKE}" == "1" ]]; then
    require_once "DNS CLI result" "${SERIAL_LOG}" "localhost -> 127.0.0.1" >/dev/null
    require_count "TCP CLI connect" "${SERIAL_LOG}" \
        "Connected to 10.0.2.2:" 2
    require_once "TCP CLI echo" "${SERIAL_LOG}" \
        "sent=17 received=17 data=mochios-tcp-smoke" >/dev/null
    if [[ "${TLS_HTTP_CLIENT_SMOKE}" == "0" && "${ACCOUNTS_HTTPS_SMOKE}" == "0" ]]; then
        assert_order "DNS client lifecycle" "${NETWORK_LOG}" \
            "DNS query" "network.service: DNS query sent name=localhost attempt=1" \
            "DNS response" "network.service: DNS response received name=localhost" \
            "DNS resolution" "network.service: DNS resolved name=localhost address=127.0.0.1"
        require_at_least "TCP SYN" "${NETWORK_LOG}" "network.service: TCP SYN sent" 2
        require_at_least "TCP SYN+ACK" "${NETWORK_LOG}" \
            "network.service: TCP SYN+ACK received" 2
        require_count "TCP smoke connection" "${NETWORK_LOG}" \
            "network.service: TCP Established remote=10.0.2.2:" 2
        require_count "TCP payload ACK" "${NETWORK_LOG}" \
            "network.service: TCP payload acknowledged bytes=17" 1
        require_count "TCP payload receive" "${NETWORK_LOG}" \
            "network.service: TCP payload received bytes=17" 1
        require_at_least "TCP FIN" "${NETWORK_LOG}" \
            "network.service: TCP FIN close complete" 2
    else
        for pattern in \
            "network.service: DNS query sent name=localhost attempt=1" \
            "network.service: DNS response received name=localhost" \
            "network.service: DNS resolved name=localhost address=127.0.0.1" \
            "network.service: TCP SYN sent" \
            "network.service: TCP SYN+ACK received" \
            "network.service: TCP Established remote=10.0.2.2:" \
            "network.service: TCP payload acknowledged bytes=17" \
            "network.service: TCP payload received bytes=17" \
            "network.service: TCP FIN close complete"; do
            grep -aFq -- "${pattern}" "${NETWORK_LOG}" ||
                die "missing network lifecycle log in extended network smoke: ${pattern}"
        done
    fi
fi

if [[ "${MPKG_RUNTIME_SMOKE}" == "1" ]]; then
    require_once "MPKG installer load" "${SERIAL_LOG}" \
        "execve: loaded '/bin/mpk'" >/dev/null
fi

assert_order "service-manager.service log" "${SERVICE_MANAGER_LOG}" \
    "service manager start" "service-manager.service: start" \
    "drivers spawn" "service-manager.service: drivers.service spawned pid=" \
    "driver delegate registration" "service-manager.service: registered drivers.service as driver delegate" \
    "drivers hello wait" "service-manager.service: waiting for drivers.service hello" \
    "drivers hello" "service-manager.service: drivers.service hello received" \
    "input spawn" "service-manager.service: input.service spawned pid=" \
    "display spawn" "service-manager.service: display.driver spawned pid=" \
    "display ready wait" "service-manager.service: waiting for display.driver ready" \
    "display ready" "service-manager.service: display.driver ready" \
    "input ready wait" "service-manager.service: waiting for input.service ready" \
    "input ready" "service-manager.service: input.service ready" \
    "compositor spawn" "service-manager.service: compositor.service spawned pid=" \
    "driver discovery request" "service-manager.service: driver discovery requested" \
    "driver discovery complete" "service-manager.service: driver discovery complete" \
    "network spawn" "service-manager.service: network.service spawned pid=" \
    "Binder spawn" "service-manager.service: Binder.app spawned pid=" \
    "network ready wait" "service-manager.service: waiting for network.service ready" \
    "network ready" "service-manager.service: network.service ready" \
    "update spawn" "service-manager.service: update.service spawned pid=" \
    "service manager resident" "service-manager.service: resident phase reason=Running"

if [[ "${XHCI_ENABLED}" == "1" ]]; then
    assert_order "drivers.service log" "${DRIVERS_LOG}" \
        "drivers start" "drivers.service: start" \
        "USB bundle discovery" "drivers.service: matched bundle=/bin/drivers/usb/" \
        "USB errno=22" "drivers.service: spawn failed /bin/drivers/usb/qemu-usb.driver/entry.elf errno=22" \
        "PS/2 bundle discovery" "drivers.service: matched bundle=/bin/drivers/ps2/" \
        "i8042 driver spawn" "drivers.service: active bundle=/bin/drivers/ps2/i8042.driver" \
        "network bundle discovery" "drivers.service: matched bundle=/bin/drivers/network/virtio-net.driver" \
        "virtio-net driver spawn" "drivers.service: active bundle=/bin/drivers/network/virtio-net.driver"
else
    assert_order "drivers.service log" "${DRIVERS_LOG}" \
        "drivers start" "drivers.service: start" \
        "PS/2 bundle discovery" "drivers.service: matched bundle=/bin/drivers/ps2/" \
        "i8042 driver spawn" "drivers.service: active bundle=/bin/drivers/ps2/i8042.driver" \
        "network bundle discovery" "drivers.service: matched bundle=/bin/drivers/network/virtio-net.driver" \
        "virtio-net driver spawn" "drivers.service: active bundle=/bin/drivers/network/virtio-net.driver"
    assert_absent "disabled USB discovery" "${DRIVERS_LOG}" \
        "drivers.service: matched bundle=/bin/drivers/usb/"
fi

assert_absent "legacy input spawn" "${DRIVERS_LOG}" "drivers.service: input.service spawned pid="
assert_absent "legacy display spawn" "${DRIVERS_LOG}" "drivers.service: display.driver spawned pid="
assert_absent "legacy display ready wait" "${DRIVERS_LOG}" "drivers.service: waiting for display.driver ready"
assert_absent "legacy input ready wait" "${DRIVERS_LOG}" "drivers.service: waiting for input.service ready"
assert_absent "legacy compositor spawn" "${DRIVERS_LOG}" "drivers.service: compositor.service spawned pid="
assert_absent "legacy tty spawn" "${DRIVERS_LOG}" "drivers.service: tty.service spawned pid="
assert_absent "disabled VGA terminal" "${SERIAL_LOG}" "exec: loaded '/system/services/tty.service'"

echo "[check] service-manager boot ordering verified"
