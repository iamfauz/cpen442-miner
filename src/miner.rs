
use hex;
use base64;
use std::collections::VecDeque;
use crate::{error::Error, cpen442coin, cryptowallet::Wallet};
use rand::{Rng, RngCore};
use openssl::hash::{Hasher, MessageDigest};
use console::Term;
use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::sync::{
    mpsc,
    Arc,
    Mutex,
};

const MINER_MAX_BLOCKS : usize = 8;

struct Stats {
    nhash : u64
}

pub struct Coin {
    previous_coin : String,
    blob : Vec<u8>
}

pub struct MiningManager {
    tracker : cpen442coin::Tracker,
    stats_rchan : mpsc::Receiver<Stats>,
    stats_schan : mpsc::Sender<Stats>,
    coins_rchan : mpsc::Receiver<Coin>,
    coins_schan : mpsc::SyncSender<Coin>,
    nproducers : usize,
    miners : VecDeque<Miner>
}

struct Timer {
    start : Instant,
    period : Duration
}

impl Timer {
    fn new(period : Duration) -> Self {
        Timer {
            start : Instant::now(),
            period
        }
    }

    fn check_and_reset(&mut self) -> bool {
        if self.start.elapsed() > self.period {
            let rem = self.start.elapsed() - self.period;
            self.start = Instant::now() - rem;
            true
        } else {
            false
        }
    }

    fn reset(&mut self) {
        self.start = Instant::now();
    }
}

impl MiningManager {
    pub fn new(tracker : cpen442coin::Tracker, ncpu : usize) -> Self {
        let nproducers = ncpu;
        let (stats_schan, stats_rchan) = mpsc::channel();
        let (coins_schan, coins_rchan) = mpsc::sync_channel(nproducers);
        let miners = VecDeque::new();

        MiningManager {
            tracker,
            stats_rchan,
            stats_schan,
            coins_rchan,
            coins_schan,
            nproducers,
            miners
        }
    }

    fn start_new_miner(&mut self, last_coin : &str) {
        let miner = Miner::new(self.stats_schan.clone(), self.coins_schan.clone(),
        &last_coin, self.tracker.id());

        self.miners.push_back(miner);

        let last = self.miners.len() - 1;

        self.miners[last].run();
    }

    fn stop_one_miner(&mut self) {
        if let Some(mut miner) = self.miners.pop_front() {
            miner.stop().unwrap_or_else(|e| eprintln!("Miner Join Err: {:?}", e));
        }
    }

    fn update_miners(&self, coin : &String) {
        for miner in &self.miners {
            miner.update_prev_coin(coin.clone());
        }
    }

    fn prune_stopped_miners(&mut self) {
        for i in 0..self.miners.len() {
            if self.miners[i].is_stopped() {
                self.miners[i].stop().unwrap();
                self.miners.remove(i).unwrap();
                break;
            }
        }
    }

    fn claim_coin(&mut self, term: &Term, coin : &Coin) -> Result<(), ()> {
        let mut hasher = Hasher::new(MessageDigest::md5()).unwrap();
        hasher.update(cpen442coin::COIN_PREFIX_STR.as_bytes()).unwrap();
        hasher.update(coin.previous_coin.as_bytes()).unwrap();
        hasher.update(&coin.blob).unwrap();
        hasher.update(self.tracker.id().as_bytes()).unwrap();
        let h = hasher.finish().unwrap();

        let h = hex::encode(&h[..]);

        term.write_line(&format!("\nTrying to claim coin with hash: {}", h)).unwrap();

        match self.tracker.claim_coin(coin.blob.clone()) {
            Ok(_) => {
                term.write_line("Coin successfully claimed!").unwrap();
                Ok(())
            },
            Err(e) => {
                term.write_line(&format!("Failed to claim coin: {:?}", e)).unwrap();
                Err(())
            }
        }
    }

    pub fn run(&mut self, wallet : &mut Option<Wallet>) -> Result<(), Error> {
        let term = Term::stderr();
        let mut last_coin = self.tracker.get_last_coin()?;
        let mut last_last_coin = last_coin.clone();
        let mut coins_to_claim = VecDeque::new();
        let mut check_now = false;
        let mut claim_now = false;

        let start_time = SystemTime::now();
        let mut coin_check_timer = Timer::new(Duration::from_millis(4000));
        let mut stats_print_timer = Timer::new(Duration::from_millis(1500));
        let mut stop_miner_timer = Timer::new(Duration::from_secs(64));

        let mut coin_count : u64 = 0;
        let mut mine_count : u64 = 0;

        term.write_line(&format!("Mining Coin: {}", last_coin)).unwrap();

        loop {
            if self.miners.len() < self.nproducers {
                self.start_new_miner(&last_coin);
            }

            if let Ok(stat) = self.stats_rchan.recv_timeout(Duration::from_millis(10)) {
                mine_count += stat.nhash;

                if stats_print_timer.check_and_reset() {
                    let elapsed = SystemTime::now().duration_since(start_time)
                        .unwrap().as_secs();

                    if elapsed > 0 {
                        let rate = mine_count / elapsed;
                        let expected_coin_rate =  3600 * rate / std::u32::MAX as u64;
                        let mut rate = rate as f64;
                        let mut prefix = "";

                        if rate > 1000.0 {
                            rate /= 1000.0;
                            prefix = "K";
                        }

                        if rate > 1000.0 {
                            rate /= 1000.0;
                            prefix = "M";
                        }

                        term.clear_line().unwrap();
                        term.write_str(&format!("Elapsed Time: {}s, Rate: {:.2} {}Hash/s, Predicted Coin Rate: {} Coins/Hour",
                                elapsed, rate, prefix, expected_coin_rate)).unwrap();
                    }
                }
            }

            if let Ok(coin) = self.coins_rchan.recv_timeout(Duration::from_millis(10)) {
                let blob = base64::encode(&coin.blob);

                term.write_line(&format!("\nFound Coin With Blob: {}", blob)).unwrap();
                coins_to_claim.push_front(coin);
                claim_now = true;
            }

            if coin_check_timer.check_and_reset() || check_now {
                match self.tracker.get_last_coin() {
                    Ok(coin) => {
                        if coin != last_coin && coin != last_last_coin {
                            last_last_coin = last_coin;
                            last_coin = coin;
                            term.write_line(&format!("\nCoin has changed to: {}", last_coin)).unwrap();
                        } else if coin != last_coin && coin == last_last_coin {
                            std::mem::swap(&mut last_coin, &mut last_last_coin);
                        }

                        self.update_miners(&last_coin);
                    },
                    Err(e) => {
                        term.write_line(&format!("\nFailed to get last coin: {:?}", e)).unwrap();
                    }
                };

                coin_check_timer.reset();
                check_now = false;
                claim_now = true;
            }

            if claim_now {
                coins_to_claim.retain(|coin| {
                    if last_coin == coin.previous_coin {
                        match self.claim_coin(&term, coin) {
                            Ok(_) => {
                                // Record the coin
                                if let Some(wallet) = wallet {
                                    let blob = base64::encode(&coin.blob);
                                    wallet.store(blob, coin.previous_coin.clone());
                                }

                                check_now = true;
                                coin_count += 1;
                                let elapsed = SystemTime::now().duration_since(start_time)
                                    .unwrap().as_secs();
                                let rate = 3600.0 * coin_count as f32 / elapsed as f32;
                                term.write_line(&format!("Coins Mined: {}, Rate: {:.2} Coins/Hour", coin_count, rate)).unwrap();
                                false
                            },
                            Err(_) => { true }
                        }
                    } else {
                        true
                    }
                });

                while coins_to_claim.len() > 32 {
                    let coin = coins_to_claim.pop_back().unwrap();
                    term.write_line(&format!("\nDropping Coin with previous_coin: {} blob: {}",
                            coin.previous_coin, hex::encode(coin.blob))).unwrap();
                }

                claim_now = false;
            }

            if stop_miner_timer.check_and_reset() {
                self.stop_one_miner();
            }

            self.prune_stopped_miners();
        }
    }
}


/// Data taken into the thread
struct MinerThreadData {
    stats_schan : mpsc::Sender<Stats>,
    coin_schan : mpsc::SyncSender<Coin>,
    miner_id : String,
}

/// Data shared with the thread
/// managed by a mutex
struct MinerSharedData {
    previous_coin : Option<String>,
    should_stop : bool,
}

pub struct Miner {
    tdata : Option<MinerThreadData>,
    tsdata : Arc<Mutex<MinerSharedData>>,
    thread : Option<thread::JoinHandle<Result<(), Error>>>
}

impl Miner {
    fn new(stats_schan : mpsc::Sender<Stats>,
        coin_schan : mpsc::SyncSender<Coin>,
        previous_coin : &str,
        miner_id : &str) -> Self {
        let tdata = Some(MinerThreadData {
            stats_schan,
            coin_schan,
            miner_id : String::from(miner_id),
        });

        let tsdata = Arc::from(Mutex::from(MinerSharedData {
            previous_coin : Some(String::from(previous_coin)),
            should_stop : false,
        }));

        Miner {
            tdata,
            tsdata,
            thread : None
        }
    }

    fn run(&mut self) {
        assert!(self.tdata.is_some());
        let tdata = self.tdata.take().unwrap();

        let tsdata = self.tsdata.clone();

        self.thread = Some(thread::spawn(move || -> Result<(), Error> {
            let mut tsdata = tsdata;
            match Miner::mine(&tdata, &mut tsdata) {
                Ok(_) => {
                    tsdata.lock().unwrap().should_stop = true;
                    Ok(())
                },
                Err(e) => {
                    tsdata.lock().unwrap().should_stop = true;
                    Err(e)
                }
            }
        }));
    }

    fn update_prev_coin(&self, coin : String) {
        self.tsdata.lock().unwrap().previous_coin = Some(coin);
    }

    fn is_stopped(&self) -> bool {
        self.tsdata.lock().unwrap().should_stop
    }

    fn stop(&mut self) -> Result<(), Error> {
        self.tsdata.lock().unwrap().should_stop = true;
        if let Some(thread) = self.thread.take() {
            Ok(thread.join().unwrap()?)
        } else {
            Ok(())
        }
    }

    fn mine(tdata : &MinerThreadData, tsdata: &mut Arc<Mutex<MinerSharedData>>) -> Result<(), Error> {
        use rand::distributions;
        use cpen442coin::{COIN_PREFIX_STR, MD5_BLOCK_LEN};
        use arrayvec::ArrayVec;

        let mut rng = rand::thread_rng();
        let dist = distributions::Uniform::from(0..=255);
        let mut hasher = Hasher::new(MessageDigest::md5())?;

        let mut previous_coin;

        {
            previous_coin = tsdata.lock().unwrap().previous_coin.clone().unwrap();
        }

        let mut suffix_bytes : ArrayVec<[u8; MD5_BLOCK_LEN]> = ArrayVec::new();
        suffix_bytes.try_extend_from_slice(&tdata.miner_id.as_bytes()).unwrap();

        let mut coin_block : ArrayVec<[u8; MD5_BLOCK_LEN * MINER_MAX_BLOCKS]> = ArrayVec::new();

        let mut prefix_bytes : ArrayVec<[u8; MD5_BLOCK_LEN]> = ArrayVec::new();
        prefix_bytes.try_extend_from_slice(COIN_PREFIX_STR.as_bytes()).unwrap();
        prefix_bytes.try_extend_from_slice(previous_coin.as_bytes()).unwrap();

        let start = Instant::now();
        let mut last_report_timer = Timer::new(Duration::from_millis(1000));
        let mut counter = 0;

        loop {
            {
                let mut tsdata = tsdata.lock().unwrap();

                if tsdata.previous_coin.is_some() {
                    previous_coin = tsdata.previous_coin.take().unwrap();
                    unsafe { prefix_bytes.set_len(COIN_PREFIX_STR.len()); }
                    prefix_bytes.try_extend_from_slice(previous_coin.as_bytes()).unwrap();
                }

                if tsdata.should_stop {
                    break;
                }
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
                            previous_coin,
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
                tdata.stats_schan.send(Stats{ nhash: counter }).unwrap();
                counter = 0;
            }
        }

        tdata.stats_schan.send(Stats{ nhash: counter }).unwrap();

        Ok(())
    }
}
