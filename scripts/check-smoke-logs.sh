#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 3 ]]; then
    echo "usage: $0 <serial-log> <service-manager-log> <drivers-log>" >&2
    exit 2
fi

SERIAL_LOG="$1"
SERVICE_MANAGER_LOG="$2"
DRIVERS_LOG="$3"

die() {
    echo "fatal: $*" >&2
    exit 1
}

for log in "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}"; do
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
    "signature: missing /signature.db" \
    "signature: invalid /signature.db"; do
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
    "tty.service" "exec: loaded '/system/services/tty.service'"

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
    "tty spawn" "service-manager.service: tty.service spawned pid=" \
    "service manager resident" "service-manager.service: resident phase reason=Running"

assert_order "drivers.service log" "${DRIVERS_LOG}" \
    "drivers start" "drivers.service: start" \
    "USB bundle discovery" "drivers.service: matched bundle=/bin/drivers/usb/" \
    "USB errno=22" "drivers.service: spawn failed /bin/drivers/usb/qemu-usb.driver/entry.elf errno=22" \
    "PS/2 bundle discovery" "drivers.service: matched bundle=/bin/drivers/ps2/" \
    "i8042 driver spawn" "drivers.service: spawned driver pid="

assert_absent "legacy input spawn" "${DRIVERS_LOG}" "drivers.service: input.service spawned pid="
assert_absent "legacy display spawn" "${DRIVERS_LOG}" "drivers.service: display.driver spawned pid="
assert_absent "legacy display ready wait" "${DRIVERS_LOG}" "drivers.service: waiting for display.driver ready"
assert_absent "legacy input ready wait" "${DRIVERS_LOG}" "drivers.service: waiting for input.service ready"
assert_absent "legacy compositor spawn" "${DRIVERS_LOG}" "drivers.service: compositor.service spawned pid="
assert_absent "legacy tty spawn" "${DRIVERS_LOG}" "drivers.service: tty.service spawned pid="

echo "[check] service-manager boot ordering verified"
