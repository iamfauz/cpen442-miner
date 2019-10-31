//! Coin
//!
//!
use base64;
use reqwest::{Client,Proxy};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use crate::error::Error;
use openssl::hash;
use rand::{RngCore, rngs::OsRng, seq::SliceRandom};

pub const COIN_PREFIX_STR : &str = "CPEN 442 Coin2019";

pub const MD5_BLOCK_LEN : usize = 512 / 8;
pub const MD5_HASH_LEN : usize = 128 / 8;
pub const MD5_HASH_HEX_LEN : usize = MD5_HASH_LEN * 2;

pub type CoinHash = String;

pub struct Tracker {
    miner_id : String,
    client : Client,
    proxyclients : Vec<Client>,
    last_coin_url : &'static str,
    claim_coin_url : &'static str,
    fake_last_coin : Option<String>
}

#[derive(Deserialize)]
struct LastCoinResp {
    coin_id : String
}

#[derive(Serialize)]
struct ClaimCoinReq {
    coin_blob : String,
    id_of_miner: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ClaimCoinResp {
    Fail { fail: String },
    #[allow(dead_code)]
    Success { success: String },
}


const LAST_COIN_URL : &str = "http://cpen442coin.ece.ubc.ca/last_coin";
const CLAIM_COIN_URL : &str = "http://cpen442coin.ece.ubc.ca/claim_coin";
const VERIFY_EXAMPLE_COIN_URL : &str = "http://cpen442coin.ece.ubc.ca/verify_example_coin";

impl Tracker {
    pub fn new(miner_id: String, proxies : Vec<String>) -> Result<Tracker, Error> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .gzip(false)
            .build()?;

        let mut proxyclients = Vec::new();
        for proxy in proxies {
            println!("HTTP Proxy: {}", proxy);

            let proxyc = Client::builder()
                .timeout(Duration::from_secs(3))
                .gzip(false) 
                .proxy(Proxy::http(&proxy)?)
                .build()?;

            proxyclients.push(proxyc);
        }

        Ok(Tracker {
            miner_id,
            client,
            proxyclients,
            last_coin_url : LAST_COIN_URL,
            claim_coin_url : CLAIM_COIN_URL,
            fake_last_coin : None
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

        let mut t = Self::new(miner_id, Vec::new())?;
        t.last_coin_url = "FAKE";
        t.claim_coin_url = "FAKE";
        t.fake_last_coin = Some(fake_last_coin);

        Ok(t)
    }

    #[allow(dead_code)]
    pub fn new_verify(miner_id: String) -> Result<Tracker, Error> {
        let mut t = Self::new(miner_id, Vec::new())?;

        t.claim_coin_url = VERIFY_EXAMPLE_COIN_URL;

        Ok(t)
    }

    pub fn id(&self) -> &str {
        &self.miner_id
    }

    pub fn get_last_coin(&mut self) -> Result<CoinHash, Error> {
        if let Some(fake_coin) = &self.fake_last_coin {
            Ok(fake_coin.clone())
        } else {
            for proxyc in self.proxyclients.choose_multiple(&mut OsRng,
                std::cmp::min(self.proxyclients.len(), 3)) {

                match Self::get_last_coin_c(self.last_coin_url, proxyc) {
                    Ok(coin) => {
                        if coin.len() == MD5_HASH_HEX_LEN {
                            if let Ok(_) = hex::decode(&coin) {
                                return Ok(coin)
                            }
                        }
                    },
                    Err(_) => {},
                }
            }

            Self::get_last_coin_c(self.last_coin_url, &self.client)
        }
    }

    fn get_last_coin_c(url : &str, client : &Client) -> Result<CoinHash, Error> {
        let mut response = client.post(url)
            .header("User-Agent", format!("CPEN442 Miner {}", OsRng.next_u64()))
            .header("X-Forwarded-For", format!("ARandomCPEN442Miner.{}.{}.x",
                    OsRng.next_u32(), OsRng.next_u32()))
            .send()?;

        let code = response.status();

        if code.is_success() {
            let response : LastCoinResp = response.json()?;

            Ok(response.coin_id)
        } else {
            Err(Error::new(format!("Get Last Coin Failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }

    pub fn claim_coin(&mut self, blob: Vec<u8>) -> Result<(), Error> {
        if let Some(fake_coin) = &self.fake_last_coin {
            let mut hasher = hash::Hasher::new(hash::MessageDigest::md5()).unwrap();
            hasher.update(COIN_PREFIX_STR.as_bytes()).unwrap();
            hasher.update(fake_coin.as_bytes()).unwrap();
            hasher.update(&blob).unwrap();
            hasher.update(self.miner_id.as_bytes()).unwrap();
            let h = hasher.finish().unwrap();
            let h_hex = hex::encode(&h[..]);
            if h[0] == 0 && h[1] == 0 && h[2] == 0 && h[3] == 0 {
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
            };

            for proxyc in self.proxyclients.choose_multiple(&mut OsRng,
                std::cmp::min(self.proxyclients.len(), 2)) {

                match Self::claim_coin_c(self.claim_coin_url, proxyc, &req) {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        match &e {
                            Error::Request(re) => {
                                if re.is_timeout() {
                                    continue;
                                }

                                if let Some(code) = re.status() {
                                    if code.as_u16() != 429 {
                                        return Err(e);
                                    }
                                } else {
                                    return Err(e);
                                }
                            },
                            _ => return Err(e)
                        }
                    },
                }
            }

            match Self::claim_coin_c(self.claim_coin_url, &self.client, &req) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if let Error::Request(re) = &e {
                        if let Some(code) = re.status() {
                            if code.as_u16() != 429 {
                                return Err(e);
                            }
                        } else {
                            return Err(e);
                        }
                    } else {
                        return Err(e);
                    }
                },
            }

            Err(Error::new("Claim Coin All Requests Failed".into()))
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
                Fail { fail } => Err(Error::new(
                        format!("Claim Coin failed with error: {}", fail))),
                        Success { success : _ } => Ok(())
            }
        } else {
            Err(Error::new(format!("Claim Coin failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64;

    #[test]
    fn test_last_coin_ok() {
        let mut t = Tracker::new("d41f33d21c5b2c49053c2b1cc2a8cc84".into(),
            Vec::new()).unwrap();

        let coin = t.get_last_coin().unwrap();

        assert_eq!(&coin[0..8], "00000000");

        println!("Last Coin: {}", coin);
    }

    #[test]
    fn test_claim_coin_ok() {
        let mut t = Tracker::new_verify("d41f33d21c5b2c49053c2b1cc2a8cc84".into()).unwrap();
        //t.claim_coin_url = "http://localhost:55555/verify_example_coin";

        let coin = base64::decode("WICbUP4soPxDWXV92qR6dpP7Rhs=").unwrap();

        t.claim_coin(coin).unwrap();
    }

    #[test]
    fn test_claim_coin_fail() {
        let mut t = Tracker::new_verify("d41f33d21c5b2c49052b1cc2a8cc84".into()).unwrap();
        //t.claim_coin_url = "http://localhost:55555/verify_coin";

        let coin = base64::decode("WICbUP4soPxDWXV92qR6dpP7Rhs=").unwrap();

        let e = t.claim_coin(coin).unwrap_err();

        if let Error::Msg(msg) = e {
            println!("Message = {}", msg);
            //assert!(msg.contains("unable to verify coin"));
        } else {
            panic!("Bad Error: {:?}", e);
        }
    }
}
