
use hex;
use base64;
use std::collections::VecDeque;
use crate::{
    error::Error,
    cpen442coin,
    cryptowallet::Wallet,
    util::Timer,
    cpuminer::{CpuMiner, CpuMinerFunction},
    oclminer::{OclMiner, OclMinerFunction}
};
use openssl::hash::{Hasher, MessageDigest};
use console::Term;
use atomic_option::AtomicOption;
use std::time::{Duration, SystemTime};
use std::thread;
use std::sync::{
    atomic::{AtomicBool,Ordering},
    mpsc,
    Arc,
};

pub struct Stats {
    pub nhash : u64,
    pub loopms : Option<u64>,
}

pub struct Coin {
    pub previous_coin : String,
    pub blob : Vec<u8>
}

pub struct MiningManager {
    tracker : cpen442coin::Tracker,
    stats_rchan : mpsc::Receiver<Stats>,
    stats_schan : mpsc::Sender<Stats>,
    coins_rchan : mpsc::Receiver<Coin>,
    coins_schan : mpsc::SyncSender<Coin>,
    nproducers : usize,
    noclproducers : usize,
    poll_ms : u32,
    miners : VecDeque<CpuMiner>,
    oclminers : VecDeque<OclMiner>,
    oclminerf : Option<OclMinerFunction>,
}

impl MiningManager {
    pub fn new(tracker : cpen442coin::Tracker,
        ncpu : usize, nocl : usize,
        oclf : Option<OclMinerFunction>,
        poll_ms : u32) -> Self {
        let nproducers = ncpu;
        let noclproducers = nocl;
        let (stats_schan, stats_rchan) = mpsc::channel();
        let (coins_schan, coins_rchan) = mpsc::sync_channel(ncpu + nocl);
        let miners = VecDeque::new();
        let oclminers = VecDeque::new();

        MiningManager {
            tracker,
            stats_rchan,
            stats_schan,
            coins_rchan,
            coins_schan,
            nproducers,
            noclproducers,
            poll_ms,
            miners,
            oclminerf: oclf,
            oclminers,
        }
    }

    fn start_new_miner(&mut self, last_coin : &str) {
        let miner = Miner::new(
            CpuMinerFunction::new(),
            MinerParams {
            stats_schan : self.stats_schan.clone(),
            coin_schan : self.coins_schan.clone(),
            previous_coin : String::from(last_coin),
            miner_id : String::from(self.tracker.id())
        });

        self.miners.push_back(miner);

        let last = self.miners.len() - 1;

        self.miners[last].run();
    }

    fn start_ocl_miner(&mut self, last_coin : &str) {
        if let Some(oclminerf) = &self.oclminerf {
            let miner = Miner::new(
                oclminerf.clone(),
                MinerParams {
                    stats_schan : self.stats_schan.clone(),
                    coin_schan : self.coins_schan.clone(),
                    previous_coin : String::from(last_coin),
                    miner_id : String::from(self.tracker.id())
                });

            self.oclminers.push_back(miner);

            let last = self.oclminers.len() - 1;

            self.oclminers[last].run();
        }
    }

    fn stop_one_miner(&mut self) {
        if let Some(mut miner) = self.miners.pop_front() {
            miner.stop().unwrap_or_else(|e| eprintln!("Miner Join Err: {:?}", e));
        }

        if let Some(mut miner) = self.oclminers.pop_front() {
            miner.stop().unwrap_or_else(|e| eprintln!("Miner Join Err: {:?}", e));
        }
    }

    fn update_miners(&self, coin : &String) {
        for miner in &self.miners {
            miner.update_prev_coin(coin.clone());
        }

        for miner in &self.oclminers {
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

        for i in 0..self.oclminers.len() {
            if self.oclminers[i].is_stopped() {
                self.oclminers[i].stop().unwrap();
                self.oclminers.remove(i).unwrap();
                break;
            }
        }
    }

    fn claim_coin(&mut self, term: &Term, coin : &Coin) -> Result<String, Error> {
        let mut hasher = Hasher::new(MessageDigest::md5()).unwrap();
        hasher.update(cpen442coin::COIN_PREFIX_STR.as_bytes()).unwrap();
        hasher.update(coin.previous_coin.as_bytes()).unwrap();
        hasher.update(&coin.blob).unwrap();
        hasher.update(self.tracker.id().as_bytes()).unwrap();
        let h = hasher.finish().unwrap();

        let h = hex::encode(&h[..]);

        term.write_line(&format!("\nTrying to claim coin with hash: {}", h)).unwrap();

        match self.tracker.claim_coin(coin.blob.clone(), coin.previous_coin.clone()) {
            Ok(_) => {
                Ok(h)
            },
            Err(e) => {
                Err(e)
            }
        }
    }

    pub fn run(&mut self, wallet : &mut Option<Wallet>) -> Result<(), Error> {
        let term = Term::stderr();
        self.tracker.start_last_coin_thread(self.poll_ms);
        let mut last_coin = self.tracker.get_last_coin()?;
        let mut last_last_coin = last_coin.clone();
        let mut coins_to_claim = VecDeque::new();
        let mut check_now = false;
        let mut claim_now = false;

        let start_time = SystemTime::now();
        let mut coin_check_timer = Timer::new(Duration::from_millis(1000));
        let mut stats_print_timer = Timer::new(Duration::from_millis(2000));
        let mut stop_miner_timer = Timer::new(Duration::from_secs(48));
        let mut claim_coin_timer = Timer::new(Duration::from_millis(1000));

        let mut coin_count : u64 = 0;
        let mut mine_count : u64 = 0;
        let mut loop_time : u64 = 0;

        term.write_line(&format!("Mining Coin: {}", last_coin)).unwrap();

        loop {
            if self.miners.len() < self.nproducers {
                self.start_new_miner(&last_coin);
            }

            if self.oclminers.len() < self.noclproducers {
                self.start_ocl_miner(&last_coin);
            }

            // Print the stats periodically
            if let Ok(stat) = self.stats_rchan.recv_timeout(Duration::from_millis(10)) {
                mine_count += stat.nhash;
                loop_time = (loop_time + stat.loopms.unwrap_or(loop_time)) / 2;

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
                        term.write_str(&format!("Elapsed Time: {}s, Rate: {:.2} {}Hash/s, Predicted Coin Rate: {} Coins/Hour, OpenCL Loop Time: {} ms",
                                elapsed, rate, prefix, expected_coin_rate, loop_time)).unwrap();
                    }
                }
            }

            if let Ok(coin) = self.coins_rchan.recv_timeout(Duration::from_millis(50)) {
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
                            // Haven't seen this coin, probably invalidates what we have now
                            coins_to_claim.clear();
                            term.write_line(&format!("\nCoin has changed to: {}", last_coin)).unwrap();
                        } else if coin != last_coin && coin == last_last_coin {
                            std::mem::swap(&mut last_coin, &mut last_last_coin);
                            if claim_coin_timer.check_and_reset() {
                                claim_now = true;
                            }
                        }

                        self.update_miners(&last_coin);
                    },
                    Err(e) => {
                        term.write_line(&format!("\nFailed to get last coin: {:?}", e)).unwrap();
                    }
                };

                check_now = false;
            }

            if claim_now {
                let mut claimed = false;

                coins_to_claim.retain(|coin| {
                    if last_coin == coin.previous_coin {
                        match self.claim_coin(&term, coin) {
                            Ok(coinhash) => {
                                if claimed {
                                    return false;
                                }

                                term.write_line("Coin successfully claimed!").unwrap();

                                // Record the coin
                                if let Some(wallet) = wallet {
                                    let blob = base64::encode(&coin.blob);
                                    wallet.store(blob, coin.previous_coin.clone());
                                }

                                // After a coin is successfully claimed
                                // all older coins are guarenteed to be invalid
                                claimed = true;
                                coin_count += 1;
                                let elapsed = SystemTime::now().duration_since(start_time)
                                    .unwrap().as_secs();
                                let rate = 3600.0 * coin_count as f32 / elapsed as f32;
                                term.write_line(&format!("Coins Mined: {}, Rate: {:.2} Coins/Hour", coin_count, rate)).unwrap();
                                last_coin = coinhash.clone();
                                last_last_coin = coinhash;
                                self.update_miners(&last_coin);
                                false
                            },
                            Err(e) => {
                                term.write_line(&format!("Failed to claim coin: {:?}", e)).unwrap();

                                if let Error::BadCoin(_) = e {
                                    false
                                } else {
                                    true
                                }
                            }
                        }
                    } else {
                        true
                    }
                });

                if claimed {
                    coins_to_claim.clear();
                }

                while coins_to_claim.len() > 2 {
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

struct MinerParams {
    pub stats_schan : mpsc::Sender<Stats>,
    pub coin_schan : mpsc::SyncSender<Coin>,
    pub previous_coin : String,
    pub miner_id : String,
}

/// Data taken into the thread
pub struct MinerThreadData {
    pub stats_schan : mpsc::Sender<Stats>,
    pub coin_schan : mpsc::SyncSender<Coin>,
    pub miner_id : String,
}

/// Data shared with the thread
/// managed by a mutex
pub struct MinerSharedData {
    pub previous_coin : AtomicOption<String>,
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
