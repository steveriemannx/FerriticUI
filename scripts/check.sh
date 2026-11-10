#!/usr/bin/env bash
# Builds the workspace, runs the interaction tests and compares the offscreen
# snapshots against the goldens. Exact matches pass immediately; small
# differences (other GPU/driver rounding) pass when SSIM >= 0.95.
# Use `./scripts/check.sh --bless` to regenerate the goldens.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo="$root/scripts/cargo.sh"
bless=0
if [[ "${1:-}" == "--bless" ]]; then bless=1; fi

echo "== build =="
"$cargo" build --workspace --examples

echo "== tests =="
"$cargo" test -p ferritic

echo "== snapshots =="
export TMPDIR="$root/target/snapshots"
mkdir -p "$TMPDIR" "$root/docs/images"
for example in counter widgets scroll form list; do
    "$root/target/debug/examples/$example" --snapshot
done

pairs=(
    "ferritic-counter:counter-snapshot.png"
    "ferritic-counter-states:counter-states.png"
    "ferritic-widgets:widgets-snapshot.png"
    "ferritic-scroll:scroll-snapshot.png"
    "ferritic-scroll-scrolled:scroll-scrolled.png"
    "ferritic-form:form-snapshot.png"
    "ferritic-form-selection:form-selection.png"
    "ferritic-list:list-snapshot.png"
    "ferritic-list-scrolled:list-scrolled.png"
)
for pair in "${pairs[@]}"; do
    name="${pair%%:*}"
    golden="$root/docs/images/${pair##*:}"
    actual="$TMPDIR/$name.png"
    if [[ $bless -eq 1 || ! -f "$golden" ]]; then
        cp "$actual" "$golden"
        echo "blessed $(basename "$golden")"
    elif cmp -s "$actual" "$golden"; then
        echo "$name: exact"
    else
        python3 "$root/scripts/ssim.py" "$golden" "$actual" 0.95
    fi
done
echo "snapshots ok"
echo "== ok =="
