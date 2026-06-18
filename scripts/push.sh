#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
SOURCE_SHA="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
SOURCE_SHORT_SHA="$(git -C "${ROOT_DIR}" rev-parse --short HEAD)"
SOURCE_BRANCH="$(git -C "${ROOT_DIR}" branch --show-current)"

AUTHOR_NAME="$(git -C "${ROOT_DIR}" config user.name || true)"
AUTHOR_EMAIL="$(git -C "${ROOT_DIR}" config user.email || true)"

if [[ -z "${AUTHOR_NAME}" || -z "${AUTHOR_EMAIL}" ]]; then
    echo "error: Gitのuser.nameまたはuser.emailが設定されていません" >&2
    exit 1
fi

need_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command not found: $1" >&2
        exit 1
    fi
}

need_command git
need_command rsync
need_command mktemp

usage() {
    cat <<'EOF'
Usage:
    ./scripts/push-components.sh
    ./scripts/push-components.sh user
    ./scripts/push-components.sh services
    ./scripts/push-components.sh user services

Components:
    user       src/user     -> mochiOS/syscalls
    services   src/services -> mochiOS/services

mnuを含むsrc/coreはpushしません。
EOF
}

sync_component() {
    local component="$1"
    local source_path
    local repository
    local branch
    local repository_name

    case "${component}" in
        user|syscalls)
            source_path="${ROOT_DIR}/src/user"
            repository="https://github.com/mochiOS/syscalls.git"
            repository_name="syscalls"
            branch="main"
            ;;
        services)
            source_path="${ROOT_DIR}/src/services"
            repository="https://github.com/mochiOS/services.git"
            repository_name="services"
            branch="main"
            ;;
        *)
            echo "error: unknown component: ${component}" >&2
            usage
            exit 1
            ;;
    esac

    if [[ ! -d "${source_path}" ]]; then
        echo "error: source directory not found: ${source_path}" >&2
        exit 1
    fi

    local relative_source="${source_path#"${ROOT_DIR}/"}"

    if [[ -n "$(git -C "${ROOT_DIR}" status --porcelain -- "${relative_source}")" ]]; then
        echo "error: ${relative_source}に未コミットの変更があります" >&2
        echo "先にmochiOS側でコミットしてください" >&2
        exit 1
    fi

    echo
    echo "[sync] ${relative_source} -> ${repository_name}:${branch}"

    (
        local temporary_dir
        temporary_dir="$(mktemp -d)"

        cleanup() {
            rm -rf "${temporary_dir}"
        }

        trap cleanup EXIT

        git -C "${temporary_dir}" init --quiet
        git -C "${temporary_dir}" remote add origin "${repository}"

        if git -C "${temporary_dir}" fetch \
            --quiet \
            --depth=1 \
            origin \
            "${branch}"
        then
            git -C "${temporary_dir}" checkout \
                --quiet \
                -B "${branch}" \
                FETCH_HEAD
        else
            echo "[info] remote branch ${branch} does not exist; creating it"
            git -C "${temporary_dir}" checkout \
                --quiet \
                --orphan \
                "${branch}"
        fi

        git -C "${temporary_dir}" config user.name "${AUTHOR_NAME}"
        git -C "${temporary_dir}" config user.email "${AUTHOR_EMAIL}"

        rsync \
            --archive \
            --delete \
            --exclude='.git/' \
            "${source_path}/" \
            "${temporary_dir}/"

        git -C "${temporary_dir}" add --all

        if git -C "${temporary_dir}" diff --cached --quiet; then
            echo "[skip] ${repository_name}に変更はありません"
            exit 0
        fi

        git -C "${temporary_dir}" commit \
            --quiet \
            -m "Sync from mochiOS ${SOURCE_SHORT_SHA}" \
            -m "Source repository: mochiOS/mochiOS" \
            -m "Source branch: ${SOURCE_BRANCH:-detached}" \
            -m "Source commit: ${SOURCE_SHA}"

        git -C "${temporary_dir}" push origin "HEAD:${branch}"

        echo "[done] ${repository_name}をpushしました"
    )
}

if [[ "$#" -eq 0 ]]; then
    components=(
        user
        services
    )
else
    components=("$@")
fi

for component in "${components[@]}"; do
    sync_component "${component}"
done

echo
echo "すべての同期が完了しました"