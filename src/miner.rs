

use hex;
use crate::{Error, cpen442coin};
use rand::{Rng, rngs::OsRng};
use std::iter::Extend;
use std::thread;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc
};
use num_cpus;


struct Stats {
    last_hash : cpen442coin::CoinHash,
    nhash : u64
}

pub struct Coin {
    prefix_block: Vec<u8>,
    blob : Vec<u8>
}

pub fn generate_prefix_block(previous_coin : &str)-> Vec<u8> {
    let mut block = Vec::with_capacity(cpen442coin::MD5_BLOCK_LEN);

    block.extend_from_slice(cpen442coin::COIN_PREFIX_STR.as_bytes());
    block.extend_from_slice(previous_coin.as_bytes());

    block
}

pub struct MiningManager {
    tracker : cpen442coin::Tracker,
    stats_rchan : mpsc::Receiver<Stats>,
    stats_schan : mpsc::Sender<Stats>,
    coins_rchan : mpsc::Receiver<Coin>,
    coins_schan : mpsc::SyncSender<Coin>,
    nproducers : usize
}


impl MiningManager {
    pub fn new(tracker : cpen442coin::Tracker) -> Self {
        let nproducers = num_cpus::get();
        let (stats_schan, stats_rchan) = mpsc::channel();
        let (coins_schan, coins_rchan) = mpsc::sync_channel(nproducers);

        MiningManager {
            tracker,
            stats_rchan,
            stats_schan,
            coins_rchan,
            coins_schan,
            nproducers,
        }
    }

    pub fn run(&mut self, last_coin : Option<String>) {
        use std::time::{Duration, SystemTime};

        let mut miners = Vec::new();
        let mut last_coin = last_coin.unwrap();

        let start_time = SystemTime::now();

        let mut mine_count : u64 = 0;
        let mut print_count = 0;

        loop {
            if miners.len() < self.nproducers {
                let mut miner = Miner::new(self.stats_schan.clone(), self.coins_schan.clone(),
                    &last_coin, self.tracker.id());

                miners.push(miner);

                let last = miners.len() - 1;

                miners[last].run();
                thread::sleep(Duration::from_millis(250));
            }

            if let Ok(stat) = self.stats_rchan.recv_timeout(Duration::from_millis(25)) {
                mine_count += stat.nhash;

                print_count += 1;

                if print_count % 32 == 0 {
                    let elapsed = SystemTime::now().duration_since(start_time).unwrap();
                    let rate = (mine_count as f64) / (elapsed.as_secs() as f64);

                    println!("Elapsed Time: {}s, Rate: {} hash/s",
                        elapsed.as_secs(), rate);

                    //println!("Last Hash: {}", stat.last_hash.as_hex());
                    print_count = 0;
                }
            }

            if let Ok(coin) = self.coins_rchan.recv_timeout(Duration::from_millis(25)) {
                let blob = hex::encode(&coin.blob);

                println!("Found Coin With Blob: {}", blob);

                self.tracker.claim_coin(coin.blob).expect("Failed to verify Coin!");

                for miner in &mut miners {
                    miner.stop().unwrap();
                }

                miners.clear();

                break;
            }

            for i in 0..miners.len() {
                if miners[i].is_stopped() {
                    miners.remove(i);
                    break;
                }
            }
        }
    }
}


struct MinerThreadData {
    stats_schan : mpsc::Sender<Stats>,
    coin_schan : mpsc::SyncSender<Coin>,
    prefix_block : Vec<u8>,
    miner_id : Vec<u8>,
}

pub struct Miner {
    tdata : Option<MinerThreadData>,
    should_stop : Arc<AtomicBool>,
    thread : Option<thread::JoinHandle<Result<(), Error>>>
}

impl Miner {
    fn new(stats_schan : mpsc::Sender<Stats>,
        coin_schan : mpsc::SyncSender<Coin>,
        previous_coin : &str,
        miner_id : &str) -> Self {
        let should_stop = Arc::new(AtomicBool::new(false));
        let prefix_block = generate_prefix_block(previous_coin);
        let miner_id = Vec::from(miner_id.as_bytes());

        let tdata = MinerThreadData {
            stats_schan,
            coin_schan,
            prefix_block,
            miner_id,
        };

        Miner {
            should_stop,
            tdata : Some(tdata),
            thread : None
        }
    }

    fn run(&mut self) {
        assert!(self.tdata.is_some());
        let tdata = self.tdata.take().unwrap();

        let should_stop = self.should_stop.clone();

        self.thread = Some(thread::spawn(move || -> Result<(), Error> {
            match Miner::mine(&tdata, &should_stop) {
                Ok(_) => {
                    should_stop.store(true, Ordering::Relaxed);
                    Ok(())
                },
                Err(e) => {
                    should_stop.store(true, Ordering::Relaxed);
                    Err(e)
                }
            }
        }));
    }

    fn is_stopped(&self) -> bool {
        self.should_stop.load(Ordering::Relaxed)
    }

    fn stop(&mut self) -> Result<(), Error> {
        self.should_stop.store(true, Ordering::Relaxed);
        Ok(self.thread.take().unwrap().join().unwrap()?)
    }

    fn mine(tdata : &MinerThreadData, should_stop : &Arc<AtomicBool>) -> Result<(), Error> {
        use openssl::hash::{Hasher, MessageDigest};
        use rand::seq::SliceRandom;
        use rand::distributions::Standard;
        use cpen442coin::MD5_BLOCK_LEN;

        let mut rng = rand::thread_rng();

        let mut hasher = Hasher::new(MessageDigest::md5())?;

        let mut counter = 0;

        while ! should_stop.load(Ordering::Relaxed) {
            let mut coin_block : Vec<u8> = rng.sample_iter(Standard).take(MD5_BLOCK_LEN).collect();
            hasher.update(&tdata.prefix_block).unwrap();
            hasher.update(&coin_block).unwrap();
            hasher.update(&tdata.miner_id).unwrap();
            let h = hasher.finish()?;

            if h[0..3] == [0u8; 4] {
                let mut coin = Coin {
                    prefix_block : tdata.prefix_block.clone(),
                    blob : Vec::new()
                };

                std::mem::swap(&mut coin.blob, &mut coin_block);

                let blob = hex::encode(&coin.blob);

                println!("Found Coin With Blob: {}", blob);

                match tdata.coin_schan.send(coin) {
                    Ok(_) => {},
                    Err(e) => return Err(Error::new(
                            format!("Miner Mpsc Send Failed: {:?}", e)))
                };

                break;
            }

            //coin_block.partial_shuffle(&mut rng, MD5_BLOCK_LEN / 4);

            counter += 1;

            if counter % 16384 == 0 {
                let mut last_hash = cpen442coin::CoinHash {
                    bytes: [0u8; 16]
                };

                last_hash.bytes[..].copy_from_slice(&h[..]);

                tdata.stats_schan.send(Stats{ last_hash, nhash: 16384 }).unwrap();
            }

            if counter == 16777216 {
                break;
            }
        }

        Ok(())
    }
}
