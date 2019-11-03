#! /bin/bash

script_dir=$(dirname "$0")

cd "$script_dir"

echo "ulimit -Sn: $(ulimit -Sn)"

opts=(
    --identity 00ed0d3d8577f6c8c654c8c4527ce884
    --cl-device 0
    --cl-workgroup-factor 64
    --ncpu 2
    --poll-ms 500
    -o wallet5.txt
    "$@"
    --proxy-file proxy.txt
)

nice -n -5 ./cpen442-miner "${opts[@]}"
