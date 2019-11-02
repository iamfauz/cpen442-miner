#! /bin/bash

miner=target/release/miner

opts=(
    --cl-device 0
    --ncpu 2
    --poll-ms 400
    -o wallet4.txt
    "$@"
    --proxy-file proxy.txt
)
nice -n -5 "$miner" "${opts[@]}"
