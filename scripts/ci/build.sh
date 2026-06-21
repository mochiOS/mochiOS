#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERSION_FILE="${ROOT_DIR}/version.toml"
BUILD_SCRIPT="${SCRIPT_DIR}/build.sh"
ARTIFACT_DIR="${ROOT_DIR}/out/artifacts"

die() {
    echo "fatal: $*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 ||
        die "required command not found: $1"
}

need_file() {
    [[ -f "$1" ]] ||
        die "required file not found: $1"
}

read_string_value() {
    local key="$1"

    perl -ne '
        my $key = $ENV{"VERSION_KEY"};

        if (/^\s*\Q$key\E\s*=\s*"([^"]*)"\s*$/) {
            print "$1\n";
            exit;
        }
    ' "${VERSION_FILE}"
}

read_integer_value() {
    local key="$1"

    VERSION_KEY="${key}" perl -ne '
        my $key = $ENV{"VERSION_KEY"};

        if (/^\s*\Q$key\E\s*=\s*(\d+)\s*$/) {
            print "$1\n";
            exit;
        }
    ' "${VERSION_FILE}"
}

need_cmd awk
need_cmd date
need_cmd find
need_cmd perl
need_cmd sha256sum
need_cmd sort
need_cmd xargs

need_file "${VERSION_FILE}"
need_file "${BUILD_SCRIPT}"

RELEASE="$(
    VERSION_KEY="release" read_string_value "release"
)"

CODENAME="$(
    VERSION_KEY="codename" read_string_value "codename"
)"

CHANNEL="$(
    VERSION_KEY="channel" read_string_value "channel"
)"

CURRENT_BUILD="$(
    read_integer_value "build"
)"

[[ -n "${RELEASE}" ]] ||
    die "release was not found in ${VERSION_FILE}"

[[ -n "${CODENAME}" ]] ||
    die "codename was not found in ${VERSION_FILE}"

[[ -n "${CHANNEL}" ]] ||
    die "channel was not found in ${VERSION_FILE}"

[[ "${CURRENT_BUILD}" =~ ^[0-9]+$ ]] ||
    die "invalid build number in ${VERSION_FILE}: ${CURRENT_BUILD}"

NEXT_BUILD="$((CURRENT_BUILD + 1))"
BUILD_DATE="$(date -u '+%Y-%m-%d %H:%M:%S')"

echo "[version] release: ${RELEASE}"
echo "[version] codename: ${CODENAME}"
echo "[version] channel: ${CHANNEL}"
echo "[version] current build: ${CURRENT_BUILD}"
echo "[version] next build: ${NEXT_BUILD}"
echo "[version] build date: ${BUILD_DATE} UTC"

export MOCHIOS_RELEASE="${RELEASE}"
export MOCHIOS_CODENAME="${CODENAME}"
export MOCHIOS_CHANNEL="${CHANNEL}"
export BUILD_NUMBER="${NEXT_BUILD}"
export BUILD_DATE="${BUILD_DATE}"

echo "[build] start mochiOS build"
"${BUILD_SCRIPT}"

[[ -d "${ARTIFACT_DIR}" ]] ||
    die "artifact directory was not created: ${ARTIFACT_DIR}"

echo "[version] update version.toml"

VERSION_TMP="$(mktemp "${ROOT_DIR}/.version.toml.XXXXXX")"

cleanup() {
    rm -f "${VERSION_TMP}"
}

trap cleanup EXIT

awk \
    -v build_date="${BUILD_DATE}" \
    -v build="${NEXT_BUILD}" \
    '
    /^[[:space:]]*build_date[[:space:]]*=/ {
        print "build_date = \"" build_date "\""
        next
    }

    /^[[:space:]]*build[[:space:]]*=/ {
        print "build = " build
        next
    }

    {
        print
    }
    ' \
    "${VERSION_FILE}" > "${VERSION_TMP}"

chmod --reference="${VERSION_FILE}" "${VERSION_TMP}"
mv "${VERSION_TMP}" "${VERSION_FILE}"

trap - EXIT

echo "[artifact] copy version.toml"
install -m 0644 \
    "${VERSION_FILE}" \
    "${ARTIFACT_DIR}/version.toml"

echo "[artifact] regenerate checksums"
(
    cd "${ARTIFACT_DIR}"

    find . \
        -maxdepth 1 \
        -type f \
        ! -name SHA256SUMS \
        -printf '%P\0' |
        sort -z |
        xargs -0 sha256sum > SHA256SUMS
)

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
        echo "release=${RELEASE}"
        echo "codename=${CODENAME}"
        echo "channel=${CHANNEL}"
        echo "build=${NEXT_BUILD}"
        echo "build_date=${BUILD_DATE}"
    } >> "${GITHUB_OUTPUT}"
fi

echo "[done] mochiOS ${RELEASE} ${CODENAME}"
echo "[done] build ${NEXT_BUILD}"