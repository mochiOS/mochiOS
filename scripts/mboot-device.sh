#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ACTION=${1:-}
DEVICE=${DEVICE:-mboot-dev.local}
IMAGE=${IMAGE:-$ROOT/out/artifacts/disk.img}
DEBUG_DIR=${DEBUG_DIR:-$ROOT/out/device-debug}
SSH_IDENTITY=${SSH_IDENTITY:-}
SSH_KNOWN_HOSTS=${SSH_KNOWN_HOSTS:-}
SSH_CONFIG=${SSH_CONFIG:-/dev/null}

case "$DEVICE" in *@*) TARGET=$DEVICE;; *) TARGET=root@$DEVICE;; esac
SSH=(ssh -F "$SSH_CONFIG" -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=accept-new)
if [[ -n "$SSH_IDENTITY" ]]; then
	SSH+=(-i "$SSH_IDENTITY")
fi
if [[ -n "$SSH_KNOWN_HOSTS" ]]; then
	SSH+=(-o "UserKnownHostsFile=$SSH_KNOWN_HOSTS" -o StrictHostKeyChecking=yes)
fi

remote() {
	"${SSH[@]}" "$TARGET" "$@"
}

require_development() {
	remote 'test -f /etc/mboot-development' || {
		echo "fatal: $DEVICE is not an mBoot development image" >&2
		exit 1
	}
}

case "$ACTION" in
deploy)
	[[ -f "$IMAGE" ]] || { echo "fatal: image not found: $IMAGE" >&2; exit 1; }
	require_development
	remote 'rm -f /var/lib/mboot/mochiOS.img.new /var/lib/mboot/mochiOS.img.previous'
	bytes=$(wc -c < "$IMAGE")
	available_kib=$(remote "df -Pk /var/lib/mboot | tail -n 1 | tr -s ' ' | cut -d ' ' -f 4")
	required_kib=$(( (bytes + 1023) / 1024 + 65536 ))
	[[ "$available_kib" =~ ^[0-9]+$ ]] && (( available_kib >= required_kib )) || {
		echo "fatal: target needs ${required_kib} KiB free; available=${available_kib:-unknown}" >&2
		exit 1
	}
	hash=$(sha256sum "$IMAGE" | awk '{print $1}')
	# Development disk images contain large zero-filled and repeated regions.
	# Prefer zstd when the mBoot image provides it and retain gzip compatibility
	# with older development media.  The expanded-image hash is checked below.
	if command -v zstd >/dev/null && remote 'command -v zstd >/dev/null'; then
		echo "[deploy] $IMAGE -> $TARGET:/var/lib/mboot/mochiOS.img.new (zstd stream)"
		zstd -1 -T0 -q -c "$IMAGE" | remote 'umask 077; zstd -d -q -c > /var/lib/mboot/mochiOS.img.new'
	else
		echo "[deploy] $IMAGE -> $TARGET:/var/lib/mboot/mochiOS.img.new (gzip stream)"
		gzip -1 -c "$IMAGE" | remote 'umask 077; gzip -dc > /var/lib/mboot/mochiOS.img.new'
	fi
	remote "/usr/libexec/mboot-deploy install '$hash'"
	;;
rollback)
	require_development
	remote '/usr/libexec/mboot-deploy rollback'
	;;
restart)
	require_development
	remote '/usr/libexec/mboot-deploy restart'
	;;
logs)
	require_development
	mkdir -p "$DEBUG_DIR"
	timestamp=$(date -u +%Y%m%dT%H%M%SZ)
	output=$DEBUG_DIR/mboot-$timestamp.tar.gz
	remote 'tar -C /var/log -cf - mboot' | gzip -n > "$output"
	echo "[done] $output"
	;;
screenshot)
	require_development
	mkdir -p "$DEBUG_DIR"
	timestamp=$(date -u +%Y%m%dT%H%M%SZ)
	output=$DEBUG_DIR/mochios-$timestamp.ppm
	remote '/usr/libexec/mboot-qmp-screenshot /tmp/mochios.ppm >/dev/null && cat /tmp/mochios.ppm' > "$output"
	echo "[done] $output"
	;;
status)
	require_development
	remote 'hostname; ip address show; ip route show; /etc/init.d/S80mbootd status; /etc/init.d/S90mboot status; cat /run/mboot/qemu.pid 2>/dev/null || true'
	;;
*)
	echo 'usage: mboot-device.sh {deploy|rollback|restart|logs|screenshot|status}' >&2
	exit 2
	;;
esac
