
use hex;
use base64;
use std::collections::VecDeque;
use crate::{
    error::Error,
    cpen442coin,
    cryptowallet::Wallet,
    util::*,
    cpuminer::{CpuMiner, CpuMinerFunction},
    oclminer::{OclMiner, OclMinerFunction}
};
use openssl::hash::{Hasher, MessageDigest};
use console::Term;
use atomic_option::AtomicOption;
use std::time::{Instant, Duration};
use std::thread;
use std::sync::{
    atomic::{AtomicBool,Ordering},
    mpsc,
    Arc,
};

pub struct Stats {
    pub nhash : u64,
}

pub struct Coin {
    pub previous_coin : String,
    pub num_zeros : u64,
    pub blob : Vec<u8>
}

pub struct MiningManager {
    tracker : cpen442coin::Tracker,
    stats_rchan : mpsc::Receiver<Stats>,
    stats_schan : mpsc::SyncSender<Stats>,
    coins_rchan : mpsc::Receiver<Coin>,
    coins_schan : mpsc::SyncSender<Coin>,
    nproducers : usize,
    poll_ms : u32,
    miners : VecDeque<CpuMiner>,
    oclminers : VecDeque<OclMiner>,
    oclminerfuncs : VecDeque<OclMinerFunction>,
}

impl MiningManager {
    pub fn new(tracker : cpen442coin::Tracker,
        ncpu : usize,
        oclf : Vec<OclMinerFunction>,
        poll_ms : u32) -> Self {
        let nproducers = ncpu;
        let (stats_schan, stats_rchan) = mpsc::sync_channel(16 * ncpu + 16);
        let (coins_schan, coins_rchan) = mpsc::sync_channel(2);
        let miners = VecDeque::new();
        let oclminers = VecDeque::new();
        let oclminerfuncs = VecDeque::from(oclf);

        MiningManager {
            tracker,
            stats_rchan,
            stats_schan,
            coins_rchan,
            coins_schan,
            nproducers,
            poll_ms,
            miners,
            oclminerfuncs,
            oclminers,
        }
    }

    fn start_new_miner(&mut self, last_coin : &str, difficulty : u64) {
        let miner = Miner::new(
            CpuMinerFunction::new(),
            MinerParams {
            stats_schan : self.stats_schan.clone(),
            coin_schan : self.coins_schan.clone(),
            previous_coin : String::from(last_coin),
            difficulty,
            miner_id : String::from(self.tracker.id())
        });

        self.miners.push_back(miner);
        self.miners.back_mut().unwrap().run();
    }

    fn start_ocl_miner(&mut self, last_coin : &str, difficulty : u64) {
        // Go through miners in round robin order to prevent
        // multiple of the same miner running
        if let Some(oclminerfunc) = self.oclminerfuncs.pop_front() {
            let miner = Miner::new(
                oclminerfunc.clone(),
                MinerParams {
                    stats_schan : self.stats_schan.clone(),
                    coin_schan : self.coins_schan.clone(),
                    previous_coin : String::from(last_coin),
                    difficulty,
                    miner_id : String::from(self.tracker.id())
                }
            );

            self.oclminerfuncs.push_back(oclminerfunc);

            self.oclminers.push_back(miner);
            self.oclminers.back_mut().unwrap().run();
        }
    }

    fn update_miners_coin(&self, coin : &String) {
        for miner in &self.miners {
            miner.update_prev_coin(coin.clone());
        }

        for miner in &self.oclminers {
            miner.update_prev_coin(coin.clone());
        }
    }

    fn update_miners_difficulty(&self, coin : u64) {
        for miner in &self.miners {
            miner.update_difficulty(coin);
        }

        for miner in &self.oclminers {
            miner.update_difficulty(coin);
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

        for i in 0..self.oclminers.len() {
            if self.oclminers[i].is_stopped() {
                self.oclminers[i].stop().unwrap();
                self.oclminers.remove(i).unwrap();
                break;
            }
        }
    }

    pub fn run(&mut self, wallet : &mut Option<Wallet>) -> Result<(), Error> {
        let term = Term::stderr();
        self.tracker.start_last_coin_thread(self.poll_ms);
        let mut last_coin = self.tracker.get_last_coin()?;
        let mut last_num_zeros = self.tracker.get_difficulty()?;

        let start_time = Instant::now();
        let mut stat_start_time = Instant::now();
        let mut coin_check_timer = Timer::new(Duration::from_millis(self.poll_ms.into()));
        let mut stats_print_timer = Timer::new(Duration::from_millis(5000));
        let mut bad_coin_start_time = Instant::now();
        let mut bad_coin_print_timer = Timer::new(Duration::from_millis(10000));

        let mut coin_count : u64 = 0;
        let mut lost_coin_count : u64 = 0;
        let mut hash_count : u64 = 0;
        let mut recent_bad_coin_count = 0;

        term.write_line(&format!("Mining Coin: {}", last_coin)).unwrap();
        term.write_line(&format!("Difficulty: {}", last_num_zeros)).unwrap();

        loop {
            if self.miners.len() < self.nproducers {
                self.start_new_miner(&last_coin, last_num_zeros);
            }

            if self.oclminers.len() < self.oclminerfuncs.len() {
                self.start_ocl_miner(&last_coin, last_num_zeros);
            }

            // Print the stats periodically
            if let Ok(stat) = self.stats_rchan.try_recv() {
                hash_count += stat.nhash;

                if stats_print_timer.check_and_reset() {
                    let elapsed = stat_start_time.elapsed().as_secs();

                    if elapsed > 0 {
                        let mut rate = hash_count as f64 / elapsed as f64;
                        let expected_coin_rate = 3600.0 * rate /
                            (1u64 << (4 * last_num_zeros)) as f64;
                        let mut prefix = "";

                        for p in &["K", "M", "G"] {
                            if rate > 1000.0 {
                                rate /= 1000.0;
                                prefix = p;
                            } else {
                                break;
                            }
                        }

                        if term.is_term() {
                            term.clear_line().unwrap();
                        } else {
                            term.write_line("").unwrap();
                        }
                        term.write_str(&format!("Difficulty: {}, Rate: {:.2} {}Hash/s, Predicted Coin Rate: {:.3} Coins/Hour",
                                last_num_zeros, rate, prefix, expected_coin_rate)).unwrap();
                    }

                    if elapsed > 600 {
                        stat_start_time = Instant::now();
                        hash_count = 0;
                    }
                }
            }

            if recent_bad_coin_count >= 5 {
                if bad_coin_print_timer.check_and_reset() {
                    term.write_line("\nWaiting on coin update due to too many bad requests...").unwrap();
                }

                if bad_coin_start_time.elapsed().as_secs() >= 60 {
                    recent_bad_coin_count = 0;
                }
            } else if let Ok(coin) = self.coins_rchan.try_recv() {
                let blob = base64::encode(&coin.blob);

                term.write_line(&format!("\nFound Coin With Blob: {}", blob)).unwrap();

                if last_coin == coin.previous_coin && last_num_zeros <= coin.num_zeros {
                    let mut hasher = Hasher::new(MessageDigest::md5()).unwrap();
                    hasher.update(cpen442coin::COIN_PREFIX_STR.as_bytes()).unwrap();
                    hasher.update(coin.previous_coin.as_bytes()).unwrap();
                    hasher.update(&coin.blob).unwrap();
                    hasher.update(self.tracker.id().as_bytes()).unwrap();
                    let h = hasher.finish().unwrap();
                    let coinhash = hex::encode(&h[..]);

                    if hash_starts_n_zeroes(&h[..], last_num_zeros) {
                        term.write_line(&format!("\nTrying to claim coin with hash: {}", coinhash)).unwrap();

                        match self.tracker.claim_coin(coin.blob.clone(), coin.previous_coin.clone(), &coinhash) {
                            Ok(_) => {
                                term.write_line("Coin successfully claimed!").unwrap();

                                // Record the coin
                                if let Some(wallet) = wallet {
                                    wallet.store(blob, coin.previous_coin.clone());
                                }

                                // After a coin is successfully claimed
                                // all older coins are guarenteed to be invalid
                                coin_count += 1;
                                recent_bad_coin_count = 0;
                                let elapsed = start_time.elapsed().as_secs();
                                let rate = 3600.0 * coin_count as f32 / elapsed as f32;
                                term.write_line(&format!("Coins Mined: {}, Coins Lost: {}, Rate: {:.3} Coins/Hour",
                                        coin_count, lost_coin_count, rate)).unwrap();
                                last_coin = coinhash;
                                self.update_miners_coin(&last_coin);
                            },
                            Err(e) => {
                                term.write_line(&format!("Failed to claim coin: {:?}", e)).unwrap();
                                lost_coin_count += 1;

                                match e {
                                    Error::BadCoin(_) => {
                                        recent_bad_coin_count += 1;
                                        bad_coin_start_time = Instant::now();
                                    }
                                    Error::AllRequestsFailed(_) => {
                                        recent_bad_coin_count += 1;
                                        bad_coin_start_time = Instant::now();
                                    }
                                    _ => {},
                                }
                            }
                        }
                    } else {
                        term.write_line(&format!("Bad Coin Hash: {}", coinhash)).unwrap();
                    }
                }
            }

            if coin_check_timer.check_and_reset() {
                match self.tracker.get_last_coin() {
                    Ok(coin) => {
                        if coin != last_coin {
                            last_coin = coin;
                            term.write_line(&format!("\nCoin has changed to: {}", last_coin)).unwrap();
                            recent_bad_coin_count = 0;

                            self.update_miners_coin(&last_coin);
                        }
                    },
                    Err(e) => {
                        term.write_line(&format!("\nFailed to get last coin: {:?}", e)).unwrap();
                    }
                };

                match self.tracker.get_difficulty() {
                    Ok(num_zeros) => {
                        if num_zeros != last_num_zeros {
                            last_num_zeros = num_zeros;
                            term.write_line(&format!("\nDifficulty has changed to: {} zeros", last_num_zeros)).unwrap();

                            recent_bad_coin_count = 0;

                            self.update_miners_difficulty(last_num_zeros);
                        }
                    },
                    Err(e) => {
                        term.write_line(&format!("\nFailed to get difficulty: {:?}", e)).unwrap();
                    }
                }
            }

            self.prune_stopped_miners();

            thread::sleep(Duration::from_millis(10));
        }
    }
}

struct MinerParams {
    pub stats_schan : mpsc::SyncSender<Stats>,
    pub coin_schan : mpsc::SyncSender<Coin>,
    pub previous_coin : String,
    pub difficulty : u64,
    pub miner_id : String,
}

/// Data taken into the thread
pub struct MinerThreadData {
    pub stats_schan : mpsc::SyncSender<Stats>,
    pub coin_schan : mpsc::SyncSender<Coin>,
    pub miner_id : String,
}

/// Data shared with the thread
/// managed by a mutex
pub struct MinerSharedData {
    pub previous_coin : AtomicOption<String>,
    pub difficulty : AtomicOption<u64>,
    pub should_stop : AtomicBool,
}

pub trait MinerFunction {
    fn run(self, tdata: MinerThreadData, tsdata: Arc<MinerSharedData>) -> Result<(), Error>;
}

pub struct Miner<T : MinerFunction + Sized + Send + 'static>
{
    miner_func : Option<T>,
    tdata : Option<MinerThreadData>,
    tsdata : Arc<MinerSharedData>,
    thread : Option<thread::JoinHandle<Result<(), Error>>>
}

impl<T: MinerFunction + Sized + Send + 'static> Miner<T>
{
    fn new(miner_func : T, params : MinerParams) -> Self {
        Miner {
            miner_func : Some(miner_func),
            tdata : Some(MinerThreadData {
                stats_schan : params.stats_schan,
                coin_schan : params.coin_schan,
                miner_id : params.miner_id,
            }),
            tsdata : Arc::from(MinerSharedData {
                previous_coin : AtomicOption::from(Some(Box::from(params.previous_coin))),
                difficulty : AtomicOption::from(Some(Box::from(params.difficulty))),
                should_stop : AtomicBool::new(false),
            }),
            thread : None
        }
    }

    fn run(&mut self) {
        let miner_func = self.miner_func.take().unwrap();
        let tdata = self.tdata.take().unwrap();
        let tsdata = self.tsdata.clone();

        self.thread = Some(thread::spawn(move || -> Result<(), Error> {
            let miner_func = miner_func;
            let tdata = tdata;
            let tsdata = tsdata;
            match miner_func.run(tdata, tsdata.clone()) {
                Ok(_) => {
                    tsdata.should_stop.store(true, Ordering::Relaxed);
                    Ok(())
                },
                Err(e) => {
                    tsdata.should_stop.store(true, Ordering::Relaxed);
                    Err(e)
                }
            }
        }));
    }

    fn update_prev_coin(&self, coin : String) {
        self.tsdata.previous_coin.replace(Some(Box::from(coin)), Ordering::Relaxed);
    }

    fn update_difficulty(&self, difficulty : u64) {
        self.tsdata.difficulty.replace(Some(Box::from(difficulty)), Ordering::Relaxed);
    }

    fn is_stopped(&self) -> bool {
        self.tsdata.should_stop.load(Ordering::Relaxed)
    }

    fn stop(&mut self) -> Result<(), Error> {
        self.tsdata.should_stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            Ok(thread.join().unwrap()?)
        } else {
            Ok(())
        }
    }
}
