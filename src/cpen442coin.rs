//! Coin
//!
//!
use base64;
use reqwest::Client;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use crate::error::Error;
use crate::proxy::ProxyManager;
use openssl::hash;
use rand::{RngCore, rngs::OsRng};
use crate::util::*;
use std::thread;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

pub const COIN_PREFIX_STR : &str = "CPEN 442 Coin2019";

pub const MD5_BLOCK_LEN : usize = 512 / 8;
pub const MD5_HASH_LEN : usize = 128 / 8;
pub const MD5_HASH_HEX_LEN : usize = MD5_HASH_LEN * 2;

pub struct Tracker {
    miner_id : String,
    last_coin_thread : Option<thread::JoinHandle<()>>,
    proxy_manager : Arc<ProxyManager>,
    last_coin : Arc<Mutex<String>>,
    difficulty : Arc<Mutex<u64>>,
    last_coin_url : &'static str,
    claim_coin_url : &'static str,
    difficulty_url : &'static str,
    fake_last_coin : Option<String>,
    client : Client,
    client_reqs : VecDeque<Instant>,
}

#[derive(Deserialize)]
struct LastCoinResp {
    coin_id : String,
    #[allow(dead_code)]
    id_of_miner : String,
    #[allow(dead_code)]
    time_stamp : u64,
}

#[derive(Serialize)]
struct ClaimCoinReq {
    coin_blob : String,
    id_of_miner: String,
    hash_of_last_coin: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ClaimCoinResp {
    #[allow(dead_code)]
    Fail { fail: String },
    #[allow(dead_code)]
    Success { success: String },
}

#[derive(Deserialize)]
struct DifficultyResp {
    number_of_leading_zeros : u64,
    #[allow(dead_code)]
    time_stamp: u64,
}


const LAST_COIN_URL : &str = "http://cpen442coin.ece.ubc.ca/last_coin";
const DIFFICULTY_URL : &str = "http://cpen442coin.ece.ubc.ca/difficulty";
const CLAIM_COIN_URL : &str = "http://cpen442coin.ece.ubc.ca/claim_coin";

impl Tracker {
    pub fn new(miner_id: String, proxy_file : PathBuf) -> Result<Tracker, Error> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .gzip(false)
            .build()?;

        Ok(Tracker {
            miner_id,
            proxy_manager : Arc::new(ProxyManager::new(proxy_file)?),
            last_coin_thread : None,
            last_coin : Arc::from(Mutex::new("00000000008c70b237c12e2c25d278cc".into())), // Start with a random coin
            difficulty : Arc::from(Mutex::new(8)),
            last_coin_url : LAST_COIN_URL,
            difficulty_url : DIFFICULTY_URL,
            claim_coin_url : CLAIM_COIN_URL,
            fake_last_coin : None,
            client,
            client_reqs : VecDeque::new(),
        })
    }

    pub fn new_fake(miner_id: String) -> Result<Tracker, Error> {
        // Generate a random starting coin
        let mut hasher = hash::Hasher::new(hash::MessageDigest::md5()).unwrap();
        hasher.update(miner_id.as_bytes()).unwrap();
        hasher.update(&OsRng.next_u32().to_le_bytes()).unwrap();
        let mut fake_last_coin = hasher.finish().unwrap();
        fake_last_coin[0] = 0;
        fake_last_coin[1] = 0;
        fake_last_coin[2] = 0;
        fake_last_coin[3] = 0;

        let fake_last_coin = hex::encode(&fake_last_coin[..]);

        let mut t = Self::new(miner_id, PathBuf::new())?;
        t.last_coin_url = "FAKE";
        t.claim_coin_url = "FAKE";
        t.difficulty_url = "FAKE";
        t.fake_last_coin = Some(fake_last_coin);

        Ok(t)
    }

    pub fn start_last_coin_thread(&mut self, poll_ms: u32) {
        if let None = &self.fake_last_coin {
            let last_coin_url = String::from(self.last_coin_url);
            let difficulty_url = String::from(self.difficulty_url);
            let proxy_manager = self.proxy_manager.clone();
            let coin = self.last_coin.clone();
            let difficulty = self.difficulty.clone();

            self.last_coin_thread = Some(thread::spawn(move || {
                Self::get_thread(last_coin_url, difficulty_url,
                    proxy_manager, coin, difficulty, poll_ms);
            }));
        }
    }

    pub fn id(&self) -> &str {
        &self.miner_id
    }

    fn client_check_reqs(reqs: &mut VecDeque<Instant>) {
        loop {
            if let Some(t) = reqs.back() {
                if t.elapsed().as_millis() > 60000 {
                    reqs.pop_back();
                    continue;
                }
            }

            break;
        }
    }

    pub fn get_last_coin(&mut self) -> Result<String, Error> {
        if let Some(fake_coin) = &self.fake_last_coin {
            Ok(fake_coin.clone())
        } else {
            assert!(self.last_coin_thread.is_some());

            Self::client_check_reqs(&mut self.client_reqs);
            if self.client_reqs.len() < 5 {
                self.client_reqs.push_front(Instant::now());
                match Self::get_last_coin_c(self.claim_coin_url, &self.client) {
                    Ok(coin) => {
                        if coin.len() == MD5_HASH_HEX_LEN {
                            if let Ok(_) = hex::decode(&coin) {
                                *self.last_coin.lock().unwrap() = coin.clone();

                                return Ok(coin);
                            }
                        }
                    },
                    Err(_) => {},
                }
            }

            Ok(self.last_coin.lock().unwrap().clone())
        }
    }

    fn get_thread(last_coin_url : String,
        difficulty_url : String,
        proxy_manager: Arc<ProxyManager>,
        coin_ptr: Arc<Mutex<String>>,
        difficulty_ptr: Arc<Mutex<u64>>,
        poll_ms: u32) {

        let poll_ms = poll_ms as u64;
        let mut poll_timer = Timer::new(Duration::from_millis(poll_ms));
        let mut diff_poll_timer = Timer::new(Duration::from_millis(poll_ms * 2));
        let mut proxy_refresh_timer = Timer::new(Duration::from_secs(60));
        let mut print_error_timer = Timer::new(Duration::from_secs(30));

        let mut fail_count = 0;

        loop {
            let mut last_e = None;

            if poll_timer.check_and_reset_rt() {
                for mut proxyc in proxy_manager.get_clients(8) {
                    let proxyc = proxyc.proxy_client().access();
                    match Self::get_last_coin_c(&last_coin_url, proxyc.client()) {
                        Ok(coin) => {
                            proxyc.success();
                            if coin.len() == MD5_HASH_HEX_LEN {
                                if let Ok(_) = hex::decode(&coin) {
                                    *coin_ptr.lock().unwrap() = coin;

                                    last_e = None;
                                    fail_count = 0;
                                    break;
                                }
                            }
                        },
                        Err(e) => {
                            if ! Self::err_is_fatal(&e) {
                                proxyc.success();
                            }

                            last_e = Some(e);
                        },
                    }
                }
            }

            if diff_poll_timer.check_and_reset_rt() {
                for mut proxyc in proxy_manager.get_clients(8) {
                    let proxyc = proxyc.proxy_client().access();

                    match Self::get_difficulty_c(&difficulty_url, proxyc.client()) {
                        Ok(num_zeros) => {
                            proxyc.success();
                            *difficulty_ptr.lock().unwrap() = num_zeros;

                            last_e = None;
                            fail_count = 0;
                            break;
                        },
                        Err(e) => {
                            if ! Self::err_is_fatal(&e) {
                                proxyc.success();
                            }

                            last_e = Some(e);
                        }
                    }
                }
            }

            if let Some(e) = last_e {
                fail_count += 1;

                if print_error_timer.check_and_reset() {
                    println!("\nGet Last Thread Error: {:?}", e);
                }

                if fail_count > 5 {
                    println!("\nThrottling last thread due to too many errors: {:?}", e);
                    thread::sleep(Duration::from_secs(10));
                    fail_count = 0;
                }
            }

            if proxy_refresh_timer.check_and_reset() {
                proxy_manager.read_new_proxies().unwrap_or_else(|e| {
                    println!("\nFailed to read new proxies: {:?}", e);
                });
            }

            thread::sleep(Duration::from_millis(25));
        }
    }

    fn get_last_coin_c(url : &str, client : &Client) -> Result<String, Error> {
        let mut response = client.post(url)
            .header("User-Agent", format!("CPEN442 Miner {}", OsRng.next_u64()))
            .header("X-Forwarded-For", format!("ARandomCPEN442Miner.{}.{}.x",
                    OsRng.next_u32(), OsRng.next_u32()))
            .send()?;

        let code = response.status();

        if code.is_success() {
            let response : LastCoinResp = response.json()?;

            Ok(response.coin_id)
        } else if code.as_u16() == 400 || code.as_u16() == 429 || code.as_u16() == 409 {
            Err(Error::ServerBusy)
        } else {
            Err(Error::new(format!("Get Last Coin Failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }

    pub fn get_difficulty(&mut self) -> Result<u64, Error> {
        if let Some(_) = &self.fake_last_coin {
            Ok(9)
        } else {
            assert!(self.last_coin_thread.is_some());

            Self::client_check_reqs(&mut self.client_reqs);
            if self.client_reqs.len() < 2 {
                self.client_reqs.push_front(Instant::now());
                match Self::get_difficulty_c(self.difficulty_url, &self.client) {
                    Ok(num_zeros) => {
                        if num_zeros < 16 {
                            *self.difficulty.lock().unwrap() = num_zeros;

                            return Ok(num_zeros);
                        } else {
                            println!("\nReceived Difficulty is too high ({})!", num_zeros)
                        }

                    },
                    Err(_) => {},
                }
            }

            Ok(*self.difficulty.lock().unwrap())
        }
    }

    fn get_difficulty_c(url : &str, client : &Client) -> Result<u64, Error> {
        let mut response = client.post(url)
            .header("User-Agent", format!("CPEN442 Miner {}", OsRng.next_u64()))
            .header("X-Forwarded-For", format!("ARandomCPEN442Miner.{}.{}.x",
                    OsRng.next_u32(), OsRng.next_u32()))
            .send()?;

        let code = response.status();

        if code.is_success() {
            let response : DifficultyResp = response.json()?;

            Ok(response.number_of_leading_zeros)
        } else if code.as_u16() == 400 || code.as_u16() == 429 || code.as_u16() == 409 {
            Err(Error::ServerBusy)
        } else {
            Err(Error::new(format!("Get Difficulty Failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }

    pub fn claim_coin(&mut self,
        blob: Vec<u8>,
        previous_coin: String,
        hash: &str) -> Result<(), Error> {

        if let Some(fake_coin) = &self.fake_last_coin {
            if *fake_coin != previous_coin {
                return Err(Error::new("Previous coin does not match!".into()));
            }

            let mut hasher = hash::Hasher::new(hash::MessageDigest::md5()).unwrap();
            hasher.update(COIN_PREFIX_STR.as_bytes()).unwrap();
            hasher.update(fake_coin.as_bytes()).unwrap();
            hasher.update(&blob).unwrap();
            hasher.update(self.miner_id.as_bytes()).unwrap();
            let h = hasher.finish().unwrap();
            let h_hex = hex::encode(&h[..]);
            if hex_starts_n_zeroes(hash, *self.difficulty.lock().unwrap()) {
                if hash != h_hex {
                    return Err(Error::new(
                            format!("Calculated hash {} does not match given hash {}!",
                                h_hex, hash)
                    ));
                }

                self.fake_last_coin = Some(h_hex);
                Ok(())
            } else {
                let mut msg = Vec::new();
                msg.extend_from_slice(COIN_PREFIX_STR.as_bytes());
                msg.extend_from_slice(fake_coin.as_bytes());
                msg.extend_from_slice(&blob);
                msg.extend_from_slice(self.miner_id.as_bytes());
                Err(Error::new(format!("Invalid Coin Hash: {} Coin: {}",
                            h_hex, hex::encode(msg))))
            }
        } else {

            let req = ClaimCoinReq {
                coin_blob: base64::encode(&blob),
                id_of_miner: self.miner_id.clone(),
                hash_of_last_coin : previous_coin,
            };

            Self::client_check_reqs(&mut self.client_reqs);
            if self.client_reqs.len() < 10 {
                self.client_reqs.push_front(Instant::now());
                match Self::claim_coin_c(self.claim_coin_url, &self.client, &req) {
                    Ok(_) => {
                        *self.last_coin.lock().unwrap() = String::from(hash);
                        return Ok(())
                    },
                    Err(e) => {
                        if Self::err_is_fatal(&e) {
                            return Err(e);
                        }
                    },
                }
            }

            let mut last_e = Error::new("No Clients".into());
            for mut proxyc in self.proxy_manager.get_clients(6) {
                let proxyc = proxyc.proxy_client().access();
                match Self::claim_coin_c(self.claim_coin_url, proxyc.client(), &req) {
                    Ok(_) => {
                        proxyc.success();
                        *self.last_coin.lock().unwrap() = String::from(hash);
                        return Ok(())
                    },
                    Err(e) => {
                        if Self::err_is_fatal(&e) {
                            if let Error::BadCoin(_) = &e {
                                proxyc.success();
                            }

                            return Err(e);
                        } else {
                            proxyc.success();
                        }
                        last_e = e;
                    },
                }
            }

            Err(Error::AllRequestsFailed(format!("All Requests Failed: {:?}", last_e)))
        }
    }

    fn claim_coin_c(url : &str, client : &Client, req : &ClaimCoinReq) -> Result<(), Error> {
        use reqwest::header::CONTENT_TYPE;

        let mut response = client.post(url)
            .header(CONTENT_TYPE, "application/json")
            .json(req).send()?;

        let code = response.status();

        if code.is_success() {
            let response : ClaimCoinResp = response.json()?;

            use ClaimCoinResp::*;
            match response {
                Success { success : _ } => Ok(()),
                Fail { fail } => Err(Error::new(
                        format!("Claim Coin failed with error: {}", fail))),
            }
        } else if code.as_u16() == 400 {
            if let Ok(ClaimCoinResp::Fail { fail }) = response.json() {
                Err(Error::BadCoin(fail))
            } else {
                Err(Error::BadCoin("".into()))
            }
        } else if code.as_u16() == 429 || code.as_u16() == 409 {
            Err(Error::ServerBusy)
        } else {
            Err(Error::new(format!("Claim Coin failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }

    fn err_is_fatal(e: &Error) -> bool {
        match e {
            Error::BadCoin(_) => true,
            Error::Request(re) => {
                if re.is_timeout() {
                    return false;
                }

                if let Some(code) = re.status() {
                    if code.as_u16() != 429 && code.as_u16() != 409 {
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            },
            Error::ServerBusy => false,
            _ => true
        }
    }
}
