# CPEN 442 Miner

# Building

```sh
cargo build --release
```

Note that openssl may need to be installed.

See https://docs.rs/openssl for details.

# Running

```sh
cargo run --release -- --identity myidentitymd5 [--ncpu N]
```
