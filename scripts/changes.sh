#!/bin/bash

Tag="$1"

git log --pretty='- %B' "${Tag}"...HEAD | sed -e /^$/d
