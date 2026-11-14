#!/usr/bin/env bash
# Launch a built image through mdev; this script never builds or installs tools.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mdev="${root}/tools/mdev/mdev"
vm_name="${MDEV_NAME:-default}"
image="${ARTIFACT_DIR:-${root}/out/artifacts}/disk.img"
if ! "${mdev}" --directory "${root}/out/develop" show "${vm_name}" --json >/dev/null 2>&1; then
    "${mdev}" --directory "${root}/out/develop" create "${vm_name}" \
        --image "${image}" --runner "${root}/scripts/runner.sh" --if-missing
fi
exec "${mdev}" --directory "${root}/out/develop" run "${vm_name}"
