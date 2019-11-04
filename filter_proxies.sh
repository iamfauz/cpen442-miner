#! /bin/bash

set -e
set -o pipefail

if [ $# -lt 2 ]; then
    echo "Usage: $0 <proxies in> <proxies out>"
    exit 1
fi

proxies_in=$1
proxies_out=$2

curl_opts=(
    -X POST
    --retry 3
    --retry-delay 1
    --max-time 5
)

while read proxy; do
    echo "Testing $proxy"
    if curl "${curl_opts[@]}" -x "$proxy" http://cpen442coin.ece.ubc.ca/last_coin | grep coin_id; then
        echo "$proxy is ok"
        echo "$proxy" >> "$proxies_out"
    else
        echo "$proxy is bad"
    fi
    sleep 0.5
done < "$proxies_in"

