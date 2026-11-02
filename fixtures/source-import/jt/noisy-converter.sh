#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then
    shift 2
  elif [ "$1" = "--output" ]; then
    output="$2"
    shift 2
  else
    exit 9
  fi
done
dd if=/dev/zero bs=65536 count=4 2>/dev/null
dd if=/dev/zero bs=65536 count=4 2>/dev/null >&2
cp "$root/synthetic-contract.raw.glb" "$output"
