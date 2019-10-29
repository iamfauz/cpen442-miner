#! /bin/bash

echo "Verify a Coin Script"

if ! command -v base64 > /dev/null; then
    echo "base64 command not found!"
    exit 1
fi

if ! command -v md5sum > /dev/null; then
    echo "md5sum command not found!"
    exit 1
fi

read -p "Miner ID String:" minerid
read -p "Coin Base64 Blob:" coinblob
read -p "Previous Coin:" previouscoin

cat <(printf 'CPEN 442 Coin2019') \
    <(printf '%s' "$previouscoin") \
    <(printf '%s' "$coinblob" | base64 -d) \
    <(printf '%s' "$minerid") |
    md5sum |
    cut -d ' ' -f 1

