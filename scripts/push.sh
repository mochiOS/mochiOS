#!/bin/bash

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SUBTREES=(
  "src/apps/Binder"
  "src/apps/Dock"
  "src/apps/Kagami"
  "src/apps/Terminal"
  "src/apps/ViewKit"
)

BRANCH="main"

for path in "${SUBTREES[@]}"; do
  name="$(basename "$path")"
  remote="$(echo "$name" | tr '[:upper:]' '[:lower:]')"

  echo "==> Pushing subtree: $path -> $remote/$BRANCH"

  git subtree push --prefix="$path" "$remote" "$BRANCH"
done
