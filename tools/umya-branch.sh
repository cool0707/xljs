#!/usr/bin/env bash
# CI: build against the umya-spreadsheet fork's branch of the same name as
# this branch, when there is one, without editing Cargo.toml.
#
# Cargo.toml patches umya-spreadsheet with the fork's main branch
# (excel-compat). A [patch] in .cargo/config.toml wins over the one in
# Cargo.toml, so this appends one that points at the same-named branch.
# Without such a branch, it warns when Cargo.lock is behind excel-compat
# (run `cargo update -p umya-spreadsheet` after merging the fork's branch).
#
# usage: tools/umya-branch.sh <branch>
set -eu

url=https://github.com/cool0707/umya-spreadsheet
main=excel-compat
branch=${1:?usage: umya-branch.sh <branch>}

if [ "$branch" != "$main" ] && git ls-remote --exit-code --heads "$url" "$branch" > /dev/null; then
  cat >> .cargo/config.toml <<EOF

# Added by tools/umya-branch.sh (CI only).
[patch.crates-io]
umya-spreadsheet = { git = "$url", branch = "$branch" }
EOF
  echo "::notice title=umya-spreadsheet::using the fork's branch $branch"
  exit 0
fi

locked=$(sed -n "s|^source = \"git+$url?branch=$main#\([0-9a-f]*\)\"|\1|p" Cargo.lock)
head=$(git ls-remote --heads "$url" "$main" | cut -f1)
if [ -z "$locked" ]; then
  echo "::warning title=umya-spreadsheet::Cargo.lock does not lock the fork's $main branch"
elif [ "$locked" != "$head" ]; then
  echo "::warning title=umya-spreadsheet::Cargo.lock locks $main at ${locked:0:8}, but $main is at ${head:0:8}: run cargo update -p umya-spreadsheet"
fi
