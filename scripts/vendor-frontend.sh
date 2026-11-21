#!/usr/bin/env bash
# Download the frontend libraries pinned in frontend/vendor.lock into
# frontend/vendor/ and check their SHA-256 sums. Nothing in frontend/vendor/
# changes unless every file matches.
#
# Usage: vendor-frontend.sh [--update]
#
# Without --update the lock is authoritative: any mismatch fails. After a
# version bump, run with --update: it downloads every file at the version the
# lock names, rewrites the sums in the lock to match, and replaces the files in
# frontend/vendor/. Review the resulting diff. Renovate does exactly this in a
# post-upgrade task (see renovate.json5).
#
# A lock line whose sum is "-" is treated as "not yet known": it is reported
# and the run fails, unless --update fills it in.
set -euo pipefail

update=0
case "${1:-}" in
  "") ;;
  --update) update=1 ;;
  *) echo "usage: $0 [--update]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock="$root/frontend/vendor.lock"
out="$root/frontend/vendor"
cdn="${VENDOR_CDN:-https://cdn.jsdelivr.net/npm}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
files=()
newlock="$tmp/vendor.lock"
: >"$newlock"
while IFS= read -r line; do
  read -r file pkg version path sum <<<"$line"
  if [[ -z "$file" || "$file" == \#* ]]; then
    printf '%s\n' "$line" >>"$newlock"
    continue
  fi
  url="$cdn/$pkg@$version/$path"
  curl -fsSL --retry 3 -o "$tmp/$file" "$url"
  got="$(sha256sum "$tmp/$file" | cut -d' ' -f1)"
  printf '%s\n' "${line%"$sum"}$got" >>"$newlock"
  if ((update)); then
    echo "sum   $file  $pkg@$version  $got"
  elif [[ "$sum" == "-" ]]; then
    echo "new   $file  $pkg@$version  $got" >&2
    fail=1
  elif [[ "$got" != "$sum" ]]; then
    printf 'FAIL  %s  %s@%s\n      expected %s\n      got      %s\n' "$file" "$pkg" "$version" "$sum" "$got" >&2
    fail=1
  else
    echo "ok    $file  $pkg@$version"
  fi
  files+=("$file")
done <"$lock"

if ((fail)); then
  echo "frontend/vendor/ left unchanged." >&2
  exit 1
fi

mkdir -p "$out"
for file in "${files[@]}"; do
  mv "$tmp/$file" "$out/$file"
done
if ((update)); then
  cp "$newlock" "$lock"
fi
