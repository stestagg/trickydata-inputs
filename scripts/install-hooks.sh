#!/usr/bin/env bash
#
# Point this repo's git hooks at the versioned `.githooks/` directory, so the
# pre-commit hook that keeps the compiled corpus in sync is active. Run once
# after cloning.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
git -C "$repo_root" config core.hooksPath .githooks
chmod +x "$repo_root/.githooks/pre-commit"
echo "Installed git hooks: core.hooksPath -> .githooks"
