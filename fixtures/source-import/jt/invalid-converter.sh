#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output" ]; then
    printf 'not-a-glb' > "$2"
    exit 0
  fi
  shift
done
exit 8
