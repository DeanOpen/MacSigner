#!/usr/bin/env bash
# Computes the next release version and writes it into every version-bearing file.
# Usage: scripts/bump-version.sh [auto|patch|minor|major|X.Y.Z]
#   auto (default) reads Conventional Commits since the last vX.Y.Z tag:
#   "type!:" or "BREAKING CHANGE" -> major, "feat:" -> minor, anything else -> patch.
# With no previous tag, the highest version already in the manifests is released as-is.
# Prints the new version (without "v") on stdout.
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-auto}"

manifest_version() { grep -m1 -E '^version = "' "$1" | sed -E 's/^version = "([^"]+)".*/\1/'; }

last_tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' 2>/dev/null || true)"
if [[ -n $last_tag ]]; then
  current="${last_tag#v}"
  range="$last_tag..HEAD"
else
  current="$(printf '%s\n' "$(manifest_version Cargo.toml)" "$(manifest_version app/src-tauri/Cargo.toml)" | sort -V | tail -1)"
  range=""
fi

if [[ $mode =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  new="$mode"
elif [[ -z $last_tag && $mode == auto ]]; then
  new="$current"
else
  if [[ $mode == auto ]]; then
    if git log --format='%B' "$range" | grep -qE '^[a-z]+(\([^)]*\))?!:|^BREAKING[ -]CHANGE'; then
      mode=major
    elif git log --format='%s' "$range" | grep -qE '^feat(\([^)]*\))?:'; then
      mode=minor
    else
      mode=patch
    fi
  fi
  IFS=. read -r major minor patch <<<"$current"
  case "$mode" in
    major) new="$((major + 1)).0.0" ;;
    minor) new="$major.$((minor + 1)).0" ;;
    patch) new="$major.$minor.$((patch + 1))" ;;
    *) echo "unknown bump mode: $mode" >&2; exit 2 ;;
  esac
fi

export NEW="$new"
# First "version = " line is the [package] version in both manifests.
perl -0pi -e 's/^version = "[^"]*"/version = "$ENV{NEW}"/m' Cargo.toml app/src-tauri/Cargo.toml
for lock in Cargo.lock app/src-tauri/Cargo.lock; do
  [[ -f $lock ]] || continue
  perl -0pi -e 's/(name = "macsigner(?:-app)?"\r?\nversion = )"[^"]*"/${1}"$ENV{NEW}"/g' "$lock"
done
perl -0pi -e 's/"version": "[^"]*"/"version": "$ENV{NEW}"/' app/package.json app/src-tauri/tauri.conf.json

echo "$new"
