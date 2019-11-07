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
    --retry 1
    --retry-delay 1
    --max-time 3
)

while read proxy; do
    if grep "$proxy" "$proxies_out"; then
        echo "Proxy $proxy already tested"
        continue
    fi

    echo "Testing $proxy"
    if curl "${curl_opts[@]}" -x "$proxy" http://cpen442coin.ece.ubc.ca/last_coin | grep coin_id; then
        echo "$proxy is ok"
        echo "$proxy" >> "$proxies_out"
    else
        echo "$proxy is bad"
    fi
    sleep 0.5
done < "$proxies_in"

