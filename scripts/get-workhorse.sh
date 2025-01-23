#!/bin/bash

# first parameter is the tool to install
# work or horsed
Tool=$1

# second parameter is the platform to install
# Windows or Linux or macOS
Platform=$2

# third parameter is the architecture to install
# x64 or arm
Arch=$3

URL="https://github.com/uuhan/workhorse/releases/latest/download/${Tool}-${Platform}-${Arch}.zip"

wget $URL -O ${Tool}.zip
