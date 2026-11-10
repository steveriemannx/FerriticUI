#!/usr/bin/env bash
# Runs cargo with the workspace-local CARGO_HOME (offline, pre-populated cache).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="$root/.cargo-home"
exec cargo "$@"
