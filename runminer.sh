#! /bin/bash

set -e

script_dir=$(dirname "$0")

cd "$script_dir"

echo "Checking CPEN442 Coin is Working..."
while ! timeout 10 curl -X POST --silent http://cpen442coin.ece.ubc.ca/last_coin; do
    echo "Failed, CPEN442 Coin is down"
    sleep 5
    echo "Checking CPEN442 Coin is Working..."
done

cp -v all_proxies.txt proxy_in.txt

opts=(
    --identity 00ed0d3d8577f6c8c654c8c4527ce884
    --cl-device 0
    --cl-max-ms 150
    --cl-max-utilize 0.9
    --ncpu 2
    --poll-ms 8000
    -o wallet6.txt
    --proxy-file proxy_in.txt
)

exec ./cpen442-miner "${opts[@]}" "$@"
