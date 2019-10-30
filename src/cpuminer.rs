use crate::{
    error::Error,
    cpen442coin::{COIN_PREFIX_STR, MD5_BLOCK_LEN},
    miner::{Coin, Stats, MinerFunction, Miner, MinerThreadData, MinerSharedData},
    util::Timer
};
use rand::{Rng, RngCore};
use arrayvec::ArrayVec;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};
use openssl::hash::{Hasher, MessageDigest};

const MINER_MAX_BLOCKS : usize = 8;

pub type CpuMiner = Miner<CpuMinerFunction>;

pub struct CpuMinerFunction {}

impl CpuMinerFunction {
    pub fn new() -> CpuMinerFunction {
        CpuMinerFunction {}
    }
}

impl MinerFunction for CpuMinerFunction {
    fn run(self, tdata : MinerThreadData, tsdata: Arc<MinerSharedData>) -> Result<(), Error> {
        let mut rng = rand::thread_rng();
        let dist = rand::distributions::Uniform::from(0..=255);
        let mut hasher = Hasher::new(MessageDigest::md5())?;

        let mut previous_coin = tsdata.previous_coin.take(Ordering::Relaxed).unwrap();

        let mut suffix_bytes : ArrayVec<[u8; MD5_BLOCK_LEN]> = ArrayVec::new();
        suffix_bytes.try_extend_from_slice(tdata.miner_id.as_bytes()).unwrap();

        let mut coin_block : ArrayVec<[u8; MD5_BLOCK_LEN * MINER_MAX_BLOCKS]> = ArrayVec::new();

        let mut prefix_bytes : ArrayVec<[u8; MD5_BLOCK_LEN]> = ArrayVec::new();
        prefix_bytes.try_extend_from_slice(COIN_PREFIX_STR.as_bytes()).unwrap();
        prefix_bytes.try_extend_from_slice(previous_coin.as_bytes()).unwrap();

        let start = Instant::now();
        let mut last_report_timer = Timer::new(Duration::from_millis(2000));
        let mut counter = 0;

        while ! tsdata.should_stop.load(Ordering::Relaxed) {
            if let Some(new_coin) = tsdata.previous_coin.take(Ordering::Relaxed) {
                previous_coin = new_coin;
                unsafe { prefix_bytes.set_len(COIN_PREFIX_STR.len()); }
                prefix_bytes.try_extend_from_slice(previous_coin.as_bytes()).unwrap();
            }

            coin_block.clear();
            // Add timestamp
            coin_block.try_extend_from_slice(&start.elapsed().as_nanos().to_ne_bytes()[..]).unwrap();
            // openssl RNG
            let mut b = [0u8; 16];
            openssl::rand::rand_bytes(&mut b[..]).unwrap();
            coin_block.try_extend_from_slice(&b[..]).unwrap();
            // rand RNG
            let mut sample_len = rng.next_u32() as usize % (MD5_BLOCK_LEN * MINER_MAX_BLOCKS);
            if sample_len + coin_block.len() > MD5_BLOCK_LEN * (MINER_MAX_BLOCKS - 1) {
                sample_len -= MD5_BLOCK_LEN;
            }
            let cur_len = prefix_bytes.len() + coin_block.len() + sample_len + suffix_bytes.len();
            let sample_len = cur_len + MD5_BLOCK_LEN - cur_len % MD5_BLOCK_LEN -
                prefix_bytes.len() - coin_block.len() - suffix_bytes.len();
            coin_block.extend(rng.sample_iter(dist).take(sample_len));

            for x in 0..=255 {
                for cb_idx in 0..coin_block.len() {
                    hasher.update(&prefix_bytes).unwrap();
                    hasher.update(&coin_block).unwrap();
                    hasher.update(&suffix_bytes).unwrap();
                    let h = hasher.finish()?;

                    if h[0] == 0 && h[1] == 0 && h[2] == 0 && h[3] == 0 {
                        let coin = Coin {
                            previous_coin : *previous_coin,
                            blob : Vec::from(&coin_block[..])
                        };

                        match tdata.coin_schan.send(coin) {
                            Ok(_) => {},
                            Err(e) => return Err(Error::new(
                                    format!("Miner Mpsc Send Failed: {:?}", e)))
                        };

                        return Ok(());
                    }

                    coin_block[cb_idx] += x;

                    counter += 1;
                }
            }

            if last_report_timer.check_and_reset() {
                tdata.stats_schan.send(Stats{
                    nhash: counter,
                    loopms : None,
                }).unwrap();
                counter = 0;
            }
        }

        tdata.stats_schan.send(Stats{
            nhash: counter,
            loopms : None,
        }).unwrap();

        Ok(())
    }
}
