#!/usr/bin/env bash
# Download the frontend libraries pinned in frontend/vendor.lock into
# frontend/vendor/ and check their SHA-256 sums. Nothing in frontend/vendor/
# changes unless every file matches.
#
# A lock line whose sum is "-" is downloaded and its sum printed, so a version
# bump goes: edit the version, set the sum to "-", run this, review the file,
# paste the sum, run again.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock="$root/frontend/vendor.lock"
out="$root/frontend/vendor"
cdn="${VENDOR_CDN:-https://cdn.jsdelivr.net/npm}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
files=()
while read -r file pkg version path sum; do
  [[ -z "$file" || "$file" == \#* ]] && continue
  url="$cdn/$pkg@$version/$path"
  curl -fsSL --retry 3 -o "$tmp/$file" "$url"
  got="$(sha256sum "$tmp/$file" | cut -d' ' -f1)"
  if [[ "$sum" == "-" ]]; then
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
