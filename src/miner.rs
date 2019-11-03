
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
use std::time::{Instant, Duration, SystemTime};
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
        let (coins_schan, coins_rchan) = mpsc::sync_channel(2);
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

    pub fn run(&mut self, wallet : &mut Option<Wallet>) -> Result<(), Error> {
        let term = Term::stderr();
        self.tracker.start_last_coin_thread(self.poll_ms);
        let mut last_coin = self.tracker.get_last_coin()?;

        let start_time = SystemTime::now();
        let mut coin_check_timer = Timer::new(Duration::from_millis(self.poll_ms.into()));
        let mut stats_print_timer = Timer::new(Duration::from_millis(2000));
        let mut last_new_coin_time = Instant::now();

        let mut coin_count : u64 = 0;
        let mut lost_coin_count : u64 = 0;
        let mut mine_count : u64 = 0;
        let mut loop_time : u64 = 0;
        let mut recent_bad_coin_count = 0;

        term.write_line(&format!("Mining Coin: {}", last_coin)).unwrap();

        loop {
            if self.miners.len() < self.nproducers {
                self.start_new_miner(&last_coin);
            }

            if self.oclminers.len() < self.noclproducers {
                self.start_ocl_miner(&last_coin);
            }

            // Print the stats periodically
            if let Ok(stat) = self.stats_rchan.try_recv() {
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

                        for p in &["K", "M", "G"] {
                            if rate > 1000.0 {
                                rate /= 1000.0;
                                prefix = p;
                            } else {
                                break;
                            }
                        }

                        term.clear_line().unwrap();
                        term.write_str(&format!("Elapsed Time: {}s, Rate: {:.2} {}Hash/s, Predicted Coin Rate: {} Coins/Hour, OpenCL Loop Time: {} ms",
                                elapsed, rate, prefix, expected_coin_rate, loop_time)).unwrap();
                    }
                }
            }

            if let Ok(coin) = self.coins_rchan.try_recv() {
                let blob = base64::encode(&coin.blob);

                term.write_line(&format!("\nFound Coin With Blob: {}", blob)).unwrap();

                if recent_bad_coin_count >= 2 {
                    term.write_line("\nWaiting on new coin due to too many bad requests...").unwrap();
                } else if last_coin == coin.previous_coin {
                    let mut hasher = Hasher::new(MessageDigest::md5()).unwrap();
                    hasher.update(cpen442coin::COIN_PREFIX_STR.as_bytes()).unwrap();
                    hasher.update(coin.previous_coin.as_bytes()).unwrap();
                    hasher.update(&coin.blob).unwrap();
                    hasher.update(self.tracker.id().as_bytes()).unwrap();
                    let h = hasher.finish().unwrap();
                    let coinhash = hex::encode(&h[..]);

                    if h[0] == 0 && h[1] == 0 && h[2] == 0 && h[3] == 0 {
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
                                let elapsed = SystemTime::now().duration_since(start_time)
                                    .unwrap().as_secs();
                                let rate = 3600.0 * coin_count as f32 / elapsed as f32;
                                term.write_line(&format!("Coins Mined: {}, Coins Lost: {}, Rate: {:.2} Coins/Hour",
                                        coin_count, lost_coin_count, rate)).unwrap();
                                last_coin = coinhash;
                                self.update_miners(&last_coin);
                            },
                            Err(e) => {
                                term.write_line(&format!("Failed to claim coin: {:?}", e)).unwrap();
                                lost_coin_count += 1;

                                match e {
                                    Error::BadCoin(_) => { recent_bad_coin_count += 1; }
                                    Error::AllRequestsFailed(_) => { recent_bad_coin_count += 1; }
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

                            last_new_coin_time = Instant::now();

                            self.update_miners(&last_coin);
                        } else if last_new_coin_time.elapsed().as_secs() > 30 {
                            term.write_line("\nSleeping main thread due to no coin updates").unwrap();
                            thread::sleep(Duration::from_secs(10));
                        }
                    },
                    Err(e) => {
                        term.write_line(&format!("\nFailed to get last coin: {:?}", e)).unwrap();
                    }
                };
            }

            self.prune_stopped_miners();

            thread::sleep(Duration::from_millis(10));
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
