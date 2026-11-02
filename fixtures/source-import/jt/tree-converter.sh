#!/bin/sh
trap '' TERM
(sleep 2; printf orphan > "$ICE3_JT_ORPHAN_MARKER") &
while :; do
  sleep 1
done
