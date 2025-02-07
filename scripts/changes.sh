#!/bin/bash

LatestTag=$(git tag | tail -n 1)
Tag="${1:-$LatestTag}"

if [[ -n "$1" ]]; then
  git log --pretty='- %B' "${Tag}"...HEAD | sed -e /^$/d | uniq
else
  V1=$(echo "$Tag" | cut -d'.' -f 1)
  V2=$(echo "$Tag" | cut -d'.' -f 2)
  V3=$(echo "$Tag" | cut -d'.' -f 3)

  V3N=$((V3 + 1))

  echo '###' "$V1.$V2.$V3N"
  echo
  git log --pretty='- %B' "${Tag}"...HEAD | sed -e /^$/d | uniq
fi
