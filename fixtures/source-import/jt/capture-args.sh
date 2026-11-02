#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then
    input="$2"
    shift 2
  elif [ "$1" = "--output" ]; then
    output="$2"
    shift 2
  else
    exit 9
  fi
done
printf '%s\n' "$input" "$output" > "$output.args"
cp "$root/synthetic-contract.raw.glb" "$output"
