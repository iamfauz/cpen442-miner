# CPEN 442 Miner

A fast miner for mining CPEN-442 Coins.

## What is a CPEN-442 Coin

Mining a CPEN-442 coin consists of finding coin blobs that produce hashes with 8-12 zeros in
front using the following equation.

```
hash = MD5(“CPEN 442 Coin” + “2019” + hash_of_preceding_coin + coin_blob + id_of_miner)
```

Full details at https://blogs.ubc.ca/cpen442/assignments/assignment-4/coin-mining-contest/

## How fast can it mine?

The speed is heavily dependent on what hardware the miner has available.

For reference the miner was tested on the following hardware:

| Hardware                   | Hash Rate   | Coin Rate (8 zeroes) |
| -------------------------- | ----------- | -------------------- |
| 2 x Xeon E5-2670 (16 core) | 21 MHash/s  | 17.6 Coins/hour      |
| Nvidia GTX 970             | 2.7 GHash/s | 2250 Coins/hour      |
| AMD Rx 5700 XT             | 4.5 GHash/s | 3770 Coins/hour      |

Note how coin rate is calculated:

`Coin rate = (Hash Rate) / (2 ^ (4 * NumZeroes))`

# Building

```sh
$ cargo build --release
```

Note that OpenSSL library and a OpenCL development library need to be installed.

For OpenCL, most Linux distros have a `ocl-icd-dev` package which provides it.

For full details see: https://github.com/cogciprocate/ocl

# Running

```sh
$ cargo run --release -- --identity myidentitymd5
```

By default all CPU cores are used to mine. No GPUs are used.

## Options

```sh
$ cargo run --release -- --help

cpen442-miner 0.1.0

USAGE:
    cpen442-miner [FLAGS] [OPTIONS]

FLAGS:
        --list-cl-devices    List OpenCL Devices
        --fake               Use a fake coin tracker (Don't contact cpen442coin.ece.ubc.ca)
    -h, --help               Prints help information
        --md5identity        MD5 the Identity String
    -V, --version            Prints version information

OPTIONS:
        --cl-device <cl-device-idxs>...      The indexes of the OpenCL devices to use. --list-cl-devices to list the
                                             devices
        --cl-max-ms <cl-max-ms>              Don't allow OpenCL to take longer than this number of milliseconds
        --cl-max-utilize <cl-utilization>    Attempt to throttle OpenCL GPUs usage to this ratio [0 to 1]
        --proxy-file <http-proxies>          HTTP Proxies to use
    -i, --identity <identity>                Miner Identity String
    -j, --ncpu <ncpu>                        Use at most this many cores to mine
        --poll-ms <poll-ms>                  How often to poll last_coin in milliseconds [default: 6000]
    -o, --output <wallet>                    File to output mined coins to

```

## Running without the Coin Tracker

The coin tracker is not always up, one can use `--fake` to mine against a simulated tracker.
Note that the default difficulty on the fake tracker is 9 (1 higher/16x harder than the original competition).

```sh
$ cargo run --release -- --identity ... --fake
```

## Running with GPUs

In order to run on your GPU you must have the latest driver with OpenCL support
(E.g. in Linux with AMD gpu, amdgpu-pro needs to be installed).

### Discover GPUs

Find your GPU by running with the `--list-cl-devices` option.

```sh
$ cargo run --release -- --list-cl-devices
Device #0
  AMD Accelerated Parallel Processing OpenCL 2.1 AMD-APP (2906.7)
  Advanced Micro Devices, Inc. gfx1010
  Device Type: GPU
  Compute Units: 20
  Workgroup Size: 256
  Memory Size: 8176 MB
Device #1
  NVIDIA CUDA OpenCL 1.2 CUDA 10.1.0
  NVIDIA Corporation GeForce GTX 970
  Device Type: GPU
  Compute Units: 13
  Workgroup Size: 1024
  Memory Size: 4041 MB
```

### Run
Get the device number from `--list-cl-devices` and run with `--cl-device N`.

E.g. Use the GTX 970 from above
```sh
$ cargo run --release -- --identity ... --cl-device 1
```

By default the miner will try to maximize performance (and probably overheat your GPU).
If you are mining on the same GPU that drives your display, you may see rendering lag and screen tearing.
Thus it is recommended to use the following options to limit performance.

`--cl-max-utilize`

How much of the time it the GPU should work. Between 0 (Never run) to 1 (Always run).

E.g. to run the GPU 50% of the time use 0.5

`--cl-max-ms`

This option limits the maximum amount of time a OpenCL workgroup can take. The best way to set this
is simply through trial and error. Note that the miner prints out the OpenCL time as its running.

## Proxies

While the miner can mine nearly 1 coin per second it cannot claim them fast enough due to the request
limit on the tracking site. Thus the use of HTTP Proxy servers was added.

To use proxies, specify the file with the `--proxy-file` command.
```sh
$ cargo run --release -- --identity ... --proxy-file myproxies.txt
```

The proxy file should contain the proxies, 1 per line with the protocol:
```
http://1.2.12.22:8080
http://10.10.20.22:2222
https://1.2.12.22:1111
```
The [reqwest](https://github.com/seanmonstar/reqwest) package used in the miner should support
socks and https as well but this was never tested.

**Note:** that the miner will truncate the proxy file after reading it.
Then it will periodically check every 60s for new proxies.

Additionally the miner automatically ranks proxies by their latency and drops dead proxies so there is no need to
manually filter out the bad ones.

After adding your proxies make sure to reduce the polling time with `--poll-ms` (Around 1500-2000 is recommended):
```sh
$ cargo run --release -- --identity ... --proxy-file myproxies.txt --poll-ms 1500
```

## TODO

For anyone reading the code, please excuse the poor quality code as the miner was written with limited time
and was constantly changed to add functionality/improve performance. I realise that:
- There are almost no comments
- Some code/logic is duplicated in multiple places.

There are also some improvements that could be made:
- Use async for the HTTP requests
- Proper use of OpenCL queues (Multiple queue items and async checking of completion)
