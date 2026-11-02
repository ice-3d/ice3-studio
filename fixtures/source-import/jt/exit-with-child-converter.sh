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
cp "$root/synthetic-contract.raw.glb" "$output"
(sleep 2; printf orphan > "$ICE3_JT_ORPHAN_MARKER") &
exit 0
