#!/usr/bin/env bash
# Launch a built image through mdev; this script never builds or installs tools.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mdev="${root}/tools/mdev/mdev"
vm_name="${MDEV_NAME:-default}"
image="${ARTIFACT_DIR:-${root}/out/artifacts}/disk.img"
"${mdev}" --directory "${root}/out/develop" create "${vm_name}" \
    --image "${image}" --runner "${root}/scripts/runner.sh" --if-missing
exec "${mdev}" --directory "${root}/out/develop" run "${vm_name}"
