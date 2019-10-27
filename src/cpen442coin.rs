//! Coin
//!
//!
use base64;
use reqwest::Client;
use serde::{Serialize, Deserialize};
use std::time::Duration;
use crate::Error;

pub const COIN_PREFIX_STR : &str = "CPEN 442 Coin2019";

pub const MD5_BLOCK_LEN : usize = 512 / 8;
#[allow(dead_code)]
pub const MD5_HASH_LEN : usize = 128 / 8;

pub type CoinHash = String;

pub struct Tracker {
    miner_id : String,
    client : Client,
    last_coin_url : &'static str,
    claim_coin_url : &'static str
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
    pub fn new(miner_id: String) -> Tracker {

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .gzip(false)
            .build()
            .expect("Failed to construct HTTP Client");

        Tracker {
            miner_id,
            client,
            last_coin_url : LAST_COIN_URL,
            claim_coin_url : CLAIM_COIN_URL
        }
    }

    #[allow(dead_code)]
    pub fn new_verify(miner_id: String) -> Tracker {
        let mut t = Self::new(miner_id);

        t.claim_coin_url = VERIFY_EXAMPLE_COIN_URL;

        t
    }

    pub fn id(&self) -> &str {
        &self.miner_id
    }

    pub fn get_last_coin(&self) -> Result<CoinHash, Error> {
        let mut response = self.client.get(self.last_coin_url).json("hello").send()?;

        let code = response.status();

        if code.is_success() {
            let response : LastCoinResp = response.json()?;

            Ok(response.coin_id)
        } else {
            Err(Error::new(format!("Get Last Coin Failed Http {}: {}",
                        code.as_u16(), code.canonical_reason().unwrap_or(""))))
        }
    }


    pub fn claim_coin(&self, blob: Vec<u8>) -> Result<(), Error> {
        let req = ClaimCoinReq {
            coin_blob: base64::encode(&blob),
            id_of_miner: self.miner_id.clone(),
        };

        use reqwest::header::CONTENT_TYPE;

        let mut response = self.client.post(self.claim_coin_url)
            .header(CONTENT_TYPE, "application/json")
            .json(&req).send()?;

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
        let t = Tracker::new("d41f33d21c5b2c49053c2b1cc2a8cc84".into());

        let coin = t.get_last_coin().unwrap();

        assert_eq!(&coin[0..8], "00000000");

        println!("Last Coin: {}", coin);
    }

    #[test]
    fn test_claim_coin_ok() {
        let t = Tracker::new_verify("d41f33d21c5b2c49053c2b1cc2a8cc84".into());
        //t.claim_coin_url = "http://localhost:55555/verify_example_coin";

        let coin = base64::decode("WICbUP4soPxDWXV92qR6dpP7Rhs=").unwrap();

        t.claim_coin(coin).unwrap();
    }

    #[test]
    fn test_claim_coin_fail() {
        let t = Tracker::new_verify("d41f33d21c5b2c49052b1cc2a8cc84".into());
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
