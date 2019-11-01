#! /bin/bash

miner=target/release/miner

opts=(
    --cl-device 0
    --ncpu 2
    --poll-ms 750
    -o wallet3.txt
    "$@"
    --proxy http://51.79.30.32:8080
    --proxy http://51.79.24.191:8080
    --proxy http://51.79.141.173:8080
    --proxy http://1.0.0.1:80
    --proxy http://1.0.0.104:80
    --proxy http://1.0.0.52:80
    --proxy http://114.217.74.219:3128
    --proxy http://51.79.141.1:8080
    --proxy http://51.79.140.141:8080
    --proxy http://51.79.141.195:8080
    --proxy http://51.79.143.225:8080
    --proxy http://51.79.26.31:808
    --proxy http://144.217.161.146:8080
    --proxy http://149.56.106.104:3128
    --proxy http://51.79.29.48:8080
    --proxy http://65.111.164.121:3131
    --proxy http://167.71.103.168:3128
    --proxy http://157.245.90.37:8080
    --proxy http://167.71.142.245:8080
    --proxy http://54.90.113.123:8080
    --proxy http://157.245.88.191:8080
    --proxy http://198.199.85.139:3128
    --proxy http://50.206.9.153:80
    --proxy http://123.57.235.56:3128
    --proxy http://5.196.132.112:3128
    --proxy http://109.86.229.189:8080
    --proxy http://46.45.129.16:80
    --proxy http://51.158.98.121:8811
    --proxy http://35.239.229.253:80
    --proxy http://132.148.241.241:8888
    --proxy http://51.158.123.35:8811
    --proxy http://65.111.164.121:3131
    --proxy http://185.111.88.52:3128
    --proxy http://46.45.129.16:80
    --proxy http://51.158.98.121:8811
    --proxy http://35.239.229.253:80
    --proxy http://132.148.241.241:8888
    --proxy http://51.158.123.35:8811
    --proxy http://96.30.53.124:3128
    --proxy http://65.111.164.121:3131
)
nice -n -5 "$miner" "${opts[@]}"
