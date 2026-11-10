#!/usr/bin/env bash
# Re-vendors the blade-ferritic branch from the local blade checkout.
#
#   ./scripts/update-blade.sh          # copy the branch into vendor/blade-ferritic
#   ./scripts/update-blade.sh --check  # fail when the vendored revision is stale
#
# Override the source with BLADE_REPO / BLADE_BRANCH.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
blade="${BLADE_REPO:-$root/../blade}"
branch="${BLADE_BRANCH:-blade-ferritic}"
target="$root/vendor/blade-ferritic"
revfile="$root/vendor/blade-ferritic.rev"

revision="$(git -C "$blade" rev-parse "$branch")"

if [[ "${1:-}" == "--check" ]]; then
    recorded="$(awk '/^revision:/ { print $2 }' "$revfile")"
    if [[ "$recorded" != "$revision" ]]; then
        echo "vendor is stale: recorded $recorded, $branch is $revision" >&2
        exit 1
    fi
    echo "vendor matches $branch@$revision"
    exit 0
fi

rm -rf "$target"
mkdir -p "$target"
git -C "$blade" archive "$branch" | tar -x -C "$target"
cat > "$revfile" <<EOF
branch: $branch
revision: $revision
repository: local ../blade (fork of https://github.com/kvark/blade)
d3d12: yes (blade-graphics feature "d3d12", Windows only; not wired into the HAL traits yet)
license: MIT; see vendor/blade-ferritic/docs/licensing.md
EOF
echo "vendored $branch@$revision into vendor/blade-ferritic"
echo "next: ./scripts/cargo.sh build --workspace --examples && ./scripts/check.sh"
