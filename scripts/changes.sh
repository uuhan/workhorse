#!/bin/bash

LatestTag=$(git tag | tail -n 1)
Tag="${1:-$LatestTag}"

git log --pretty='- %B' "${Tag}"...HEAD | sed -e /^$/d | uniq
