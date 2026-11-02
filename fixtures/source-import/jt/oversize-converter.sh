#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then
    shift 2
  elif [ "$1" = "--output" ]; then
    truncate -s 536871000 "$2"
    exit 0
  else
    exit 9
  fi
done
exit 8
